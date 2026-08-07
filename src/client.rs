use crate::grf::{self, normalize};
use bytes::Bytes;
use flate2::{Compression, write::ZlibEncoder};
use regex::bytes::Regex;
use std::{
    collections::{BTreeSet, HashMap},
    ffi::OsStr,
    fs, io,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

/// Loose files are compressed once and kept, so the expensive setting is the
/// right one: level 9 costs ~25x the CPU of level 1 for ~30% fewer bytes, which
/// only pays off when the result is reused.
const DISK_COMPRESSION: Compression = Compression::new(9);

/// Below this, framing overhead eats the gain.
const MIN_COMPRESSED_SIZE: u64 = 256;

/// Cap on the deflated copies held in memory. Files past it are left
/// uncompressed rather than evicting earlier ones: the table is built once at
/// startup over a mostly static client, so there is nothing to age out.
const CACHE_BUDGET: usize = 256 * 1024 * 1024;

/// Already-compressed payloads, where deflate spends CPU to gain nothing.
const INCOMPRESSIBLE: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "mp3", "ogg", "wav", "zip", "gz", "grf", "rgz",
];

pub struct Client {
    root: PathBuf,
    archives: Vec<grf::Archive>,
    files: HashMap<Vec<u8>, PathBuf>,
    deflated: RwLock<DeflateCache>,
}

/// A loose file plus the mtime and length it had when it was compressed.
type CacheKey = (PathBuf, u64, u64);

#[derive(Default)]
struct DeflateCache {
    entries: HashMap<CacheKey, Bytes>,
    bytes: usize,
}

pub enum Located<'a> {
    Disk(&'a Path),
    Archive(usize, grf::Entry<'a>),
}

impl Client {
    pub fn open(root: &Path) -> io::Result<Client> {
        let ini_path = root.join("DATA.INI");

        let text = match fs::read_to_string(&ini_path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                eprintln!(
                    "warning: {} does not exist, no archives will be served",
                    ini_path.display()
                );
                String::new()
            }
            Err(error) => return Err(error),
        };

        let mut in_data = false;
        let mut entries: Vec<(u32, String)> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') {
                in_data = line.trim_matches(['[', ']']).eq_ignore_ascii_case("data");
                continue;
            }
            if !in_data {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Ok(index) = key.trim().parse::<u32>() else {
                continue;
            };
            entries.push((index, value.trim().to_string()));
        }

        println!(
            "{}: {} archive(s) listed",
            ini_path.display(),
            entries.len()
        );

        entries.sort_by_key(|(index, _)| *index);

        let mut archives = Vec::new();
        for (index, name) in entries {
            println!("opening {name} (priority {index})");
            archives.push(grf::Archive::open(&root.join(&name))?);
        }

        let t = Instant::now();

        let mut files: HashMap<Vec<u8>, PathBuf> = HashMap::new();

        for name in ["data", "BGM", "System", "AI"] {
            let dir = root.join(name);
            if dir.is_dir() {
                index_dir(&dir, root, &mut files)?;
            }
        }

        println!(
            "disk files indexed in {:.1}ms",
            t.elapsed().as_secs_f64() * 1000.0
        );

        Ok(Client {
            root: root.to_path_buf(),
            archives,
            files,
            deflated: RwLock::new(DeflateCache::default()),
        })
    }

    /// Find where a path lives, without touching the filesystem: loose files
    /// win over the archives, and lower DATA.INI priority wins among those.
    pub fn locate(&self, path: &[u8]) -> Option<Located<'_>> {
        if let Some(full) = self.files.get(normalize(path).as_slice()) {
            return Some(Located::Disk(full));
        }

        for (index, archive) in self.archives.iter().enumerate() {
            if let Some(entry) = archive.lookup(path) {
                return Some(Located::Archive(index, entry));
            }
        }

        None
    }

    /// Apply a regex to every known file name, in the backslash-separated form
    /// the client uses, and return the matched substrings. This is what the GRF
    /// and map viewers call to list a directory.
    pub fn search(&self, filter: &Regex) -> Vec<Vec<u8>> {
        let mut out: BTreeSet<Vec<u8>> = BTreeSet::new();

        for archive in &self.archives {
            for name in archive.names() {
                out.extend(filter.find_iter(name).map(|m| m.as_bytes().to_vec()));
            }
        }

        for path in self.files.values() {
            let relative = path
                .strip_prefix(&self.root)
                .expect("indexed paths are always under root");

            let name: Vec<u8> = relative
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .map(|&b| if b == b'/' { b'\\' } else { b })
                .collect();

            out.extend(filter.find_iter(&name).map(|m| m.as_bytes().to_vec()));
        }

        out.into_iter().collect()
    }

    pub fn read_located(&self, located: &Located) -> io::Result<Vec<u8>> {
        match located {
            Located::Disk(path) => fs::read(path),
            Located::Archive(index, entry) => self.archives[*index].inflate(entry),
        }
    }

    /// Deflated bytes for a located file, or `None` to send it as it is. Both
    /// arms are lookups, never work: archive entries hand over the stream the
    /// GRF already stores, and loose files answer from the table
    /// `compress_files` filled at startup. A file the table missed goes out as
    /// it is.
    ///
    /// The result is a [`Bytes`] on purpose: it is what the response body wants,
    /// so a hit costs a refcount bump rather than a copy of the payload.
    pub fn deflated_located(&self, located: &Located) -> Option<Bytes> {
        match located {
            Located::Archive(index, entry) => self.archives[*index].raw_if_plain(entry),
            Located::Disk(path) => {
                let key = cache_key(path)?;

                self.deflated
                    .read()
                    .expect("deflate cache is never poisoned")
                    .entries
                    .get(&key)
                    .cloned()
            }
        }
    }

    /// Compress every loose file under the served directories, with the same
    /// zlib stream a GRF stores, and keep the results in memory.
    ///
    /// Archives are left alone on purpose: their entries are already deflated at
    /// full strength -- recompressing them at level 9 measures out at +1% on a
    /// `.gnd` and -1% on a `.gat` -- so the only files with something to gain are
    /// the loose ones, which sit on disk uncompressed.
    ///
    /// Runs to completion before the server starts listening, so a request never
    /// races a half-filled table.
    pub fn compress_files(&self) {
        let candidates: Vec<PathBuf> = self
            .files
            .values()
            .filter(|path| is_compressible(path))
            .cloned()
            .collect();

        let total = candidates.len();

        if total == 0 {
            return;
        }

        let threads = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(2).max(1))
            .unwrap_or(1);

        println!("compressing {total} disk file(s) on {threads} thread(s)");

        let queue = Mutex::new(candidates);
        let skipped = AtomicUsize::new(0);
        let over_budget = AtomicUsize::new(0);
        let t = Instant::now();

        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    loop {
                        let Some(path) = queue
                            .lock()
                            .expect("compression queue is never poisoned")
                            .pop()
                        else {
                            break;
                        };

                        let Some((key, deflated)) = deflate_file(&path) else {
                            skipped.fetch_add(1, Ordering::Relaxed);
                            continue;
                        };

                        let mut cache = self
                            .deflated
                            .write()
                            .expect("deflate cache is never poisoned");

                        if cache.bytes + deflated.len() > CACHE_BUDGET {
                            over_budget.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }

                        cache.bytes += deflated.len();
                        cache.entries.insert(key, deflated);
                    }
                });
            }
        });

        let cache = self
            .deflated
            .read()
            .expect("deflate cache is never poisoned");

        println!(
            "compressed {} of {total} file(s) into {:.1} MB in {:.1}s ({} not worth it)",
            cache.entries.len(),
            cache.bytes as f64 / 1048576.0,
            t.elapsed().as_secs_f64(),
            skipped.load(Ordering::Relaxed),
        );

        let dropped = over_budget.load(Ordering::Relaxed);

        if dropped > 0 {
            println!(
                "warning: {dropped} file(s) left uncompressed, the {} MB budget was reached",
                CACHE_BUDGET / 1048576
            );
        }
    }

    pub fn etag(&self, located: &Located) -> Option<String> {
        match located {
            Located::Disk(path) => {
                let metadata = fs::metadata(path).ok()?;
                let mtime = metadata
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                Some(format!("{mtime:x}-{:x}", metadata.len()))
            }
            Located::Archive(index, entry) => {
                Some(format!("{}-{}", self.archives[*index].id, entry.identity()))
            }
        }
    }
}

/// Cache key for a loose file. The mtime and length make it self-invalidating:
/// a file edited while the server runs no longer matches its entry, so it falls
/// back to being served uncompressed instead of serving the stale copy.
fn cache_key(path: &Path) -> Option<CacheKey> {
    let metadata = fs::metadata(path).ok()?;
    let mtime = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Some((path.to_path_buf(), mtime, metadata.len()))
}

/// Whether a loose file is worth offering to the compressor at all: not an
/// already-packed format, and big enough for the framing to pay for itself.
fn is_compressible(path: &Path) -> bool {
    let packed = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| INCOMPRESSIBLE.contains(&extension.as_str()));

    if packed {
        return false;
    }

    fs::metadata(path).is_ok_and(|metadata| metadata.len() >= MIN_COMPRESSED_SIZE)
}

/// Compress one loose file. `None` if it cannot be read or barely shrank, in
/// which case sending it as it is costs less than the round trip through zlib.
fn deflate_file(path: &Path) -> Option<(CacheKey, Bytes)> {
    let key = cache_key(path)?;
    let plain = fs::read(path).ok()?;

    let mut encoder = ZlibEncoder::new(Vec::new(), DISK_COMPRESSION);
    encoder.write_all(&plain).ok()?;
    let deflated = encoder.finish().ok()?;

    if deflated.len() * 10 > plain.len() * 9 {
        return None;
    }

    Some((key, Bytes::from(deflated)))
}

fn index_dir(dir: &Path, root: &Path, files: &mut HashMap<Vec<u8>, PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            index_dir(&path, root, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("walked paths are always under root");
            if let Some(prev) =
                files.insert(normalize(relative.as_os_str().as_encoded_bytes()), path)
            {
                eprintln!(
                    "warning: {} is shadowed by another file with the same case-insensitive name",
                    prev.display()
                );
            }
        }
    }

    Ok(())
}
