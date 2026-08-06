use crate::grf::{self, normalize};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

pub struct Client {
    archives: Vec<grf::Archive>,
    files: HashMap<Vec<u8>, PathBuf>,
}

pub enum Located<'a> {
    Disk(&'a Path),
    Archive(usize, grf::Entry<'a>),
}

impl Client {
    pub fn open(root: &Path) -> io::Result<Client> {
        let ini_path = root.join("DATA.INI");
        let text = fs::read_to_string(&ini_path)?;

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

        Ok(Client { archives, files })
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

    pub fn read_located(&self, located: &Located) -> io::Result<Vec<u8>> {
        match located {
            Located::Disk(path) => fs::read(path),
            Located::Archive(index, entry) => self.archives[*index].inflate(entry),
        }
    }

    pub fn raw_located(&self, located: &Located) -> Option<&[u8]> {
        match located {
            Located::Disk(_) => None,
            Located::Archive(index, entry) => self.archives[*index].raw_if_plain(entry),
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
            Located::Archive(index, entry) => Some(format!("{index:x}-{}", entry.identity())),
        }
    }
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
