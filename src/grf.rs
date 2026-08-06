use encoding_rs::EUC_KR;
use flate2::bufread::ZlibDecoder;
use memmap2::Mmap;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    time::Instant,
};

pub struct Archive {
    pub version: u32,
    pub real_file_count: u64,
    table: Vec<u8>,
    index: HashMap<Vec<u8>, u32>,
    mmap: Mmap,
}

#[derive(Debug)]
pub struct Entry<'a> {
    pub name: &'a [u8],
    pack_size: u32,
    #[allow(dead_code)] // only used by DES-encrypted entries
    length_aligned: u32,
    real_size: u32,
    flags: u8,
    position: u64,
}

/// Stored compressed only, no DES.
const FLAG_FILE: u8 = 1;

impl Archive {
    fn meta_len(&self) -> usize {
        if self.version == 0x300 { 21 } else { 17 }
    }

    pub fn open(path: &Path) -> io::Result<Archive> {
        let mut file = File::open(&path)?;
        let metadata = file.metadata()?;

        let mut header = [0u8; 46];
        file.read_exact(&mut header)?;

        if !header.starts_with(b"Master of Magic") && !header.starts_with(b"Event Horizon") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: not a GRF archive, signature is {:?}",
                    path.display(),
                    &header[..16]
                ),
            ));
        }

        let version = u32::from_le_bytes(header[42..46].try_into().unwrap());

        if version != 0x200 && version != 0x300 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: unsupported GRF version {version:#x}, expected 0x200 or 0x300",
                    path.display(),
                ),
            ));
        }

        println!("version: {:#x}", version);

        let (table_offset, seeds, file_count) = if version == 0x200 {
            (
                u64::from(u32::from_le_bytes(header[30..34].try_into().unwrap())),
                u64::from(u32::from_le_bytes(header[34..38].try_into().unwrap())),
                u64::from(u32::from_le_bytes(header[38..42].try_into().unwrap())),
            )
        } else {
            (
                u64::from_le_bytes(header[30..38].try_into().unwrap()),
                0,
                u64::from_le_bytes(header[38..42].try_into().unwrap()),
            )
        };

        let real_file_count = if version == 0x200 {
            file_count.saturating_sub(seeds).saturating_sub(7)
        } else {
            file_count
        };

        if 46 + table_offset > metadata.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: file table offset {table_offset} lies past the end of a {}-byte file",
                    path.display(),
                    metadata.len()
                ),
            ));
        }

        file.seek(SeekFrom::Start(
            46 + table_offset + if version == 0x300 { 4 } else { 0 },
        ))?;

        let mut sizes = [0u8; 8];
        file.read_exact(&mut sizes)?;

        let pack_size = u32::from_le_bytes(sizes[0..4].try_into().unwrap());
        let real_size = u32::from_le_bytes(sizes[4..8].try_into().unwrap());

        if pack_size == 0 || real_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: empty file table, packed {pack_size} bytes and unpacked {real_size}",
                    path.display(),
                ),
            ));
        }

        let t = Instant::now();

        let mut compressed = vec![0u8; pack_size as usize];
        file.read_exact(&mut compressed)?;

        println!("read in {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);

        let t = Instant::now();

        let mut table = Vec::with_capacity(real_size as usize);
        ZlibDecoder::new(&compressed[..]).read_to_end(&mut table)?;

        println!("inflated in {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);

        if table.len() != real_size as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: file table inflated to {} bytes, header promised {real_size}",
                    path.display(),
                    table.len()
                ),
            ));
        }

        let mmap = unsafe { Mmap::map(&file)? };

        println!("vsize: {} MB", proc_status_kb("VmSize").unwrap_or(0) / 1024);

        let mut archive = Archive {
            table,
            version,
            real_file_count,
            index: HashMap::new(),
            mmap,
        };

        let walked = archive.build_index();

        if walked as u64 != archive.real_file_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: walked {walked} entries, header claims {}",
                    path.display(),
                    archive.real_file_count,
                ),
            ));
        }

        Ok(archive)
    }

    fn build_index(&mut self) -> usize {
        let mut index = HashMap::with_capacity(self.real_file_count as usize);
        let mut at = 0usize;
        let mut count = 0usize;

        let t = Instant::now();

        while let Some(entry) = self.entry_at(at) {
            if entry.name.iter().any(|&b| b >= 0x80) {
                index.insert(normalize(to_mojibake(entry.name).as_bytes()), at as u32);
                if let Some(korean) = decode_cp949(entry.name) {
                    index.insert(normalize(korean.as_bytes()), at as u32);
                }
            }

            index.insert(normalize(entry.name), at as u32);

            at += entry.name.len() + 1 + self.meta_len();
            count += 1;
        }

        println!("index built in {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);
        println!("index size: {}", index.len());
        println!("rss: {} MB", proc_status_kb("VmRSS").unwrap_or(0) / 1024);

        self.index = index;

        count
    }

    pub fn lookup(&self, path: &[u8]) -> Option<Entry<'_>> {
        let at = *self.index.get(normalize(path).as_slice())?;
        self.entry_at(at as usize)
    }

    pub fn entry_at(&self, at: usize) -> Option<Entry<'_>> {
        let rest = self.table.get(at..)?;
        let name_len = rest.iter().position(|&b| b == 0)?;
        let name = &rest[..name_len];

        let meta = rest.get(name_len + 1..name_len + 1 + self.meta_len())?;

        let pack_size = u32::from_le_bytes(meta[..4].try_into().unwrap());
        let length_aligned = u32::from_le_bytes(meta[4..8].try_into().unwrap());
        let real_size = u32::from_le_bytes(meta[8..12].try_into().unwrap());
        let flags = meta[12];

        let position = if self.version == 0x300 {
            u64::from_le_bytes(meta[13..21].try_into().ok()?)
        } else {
            u64::from(u32::from_le_bytes(meta[13..17].try_into().ok()?))
        };

        Some(Entry {
            name,
            pack_size,
            length_aligned,
            real_size,
            flags,
            position,
        })
    }

    pub fn raw(&self, entry: &Entry) -> Option<&[u8]> {
        let start = usize::try_from(entry.position).ok()? + 46;
        let len = entry.pack_size as usize;
        self.mmap.get(start..start.checked_add(len)?)
    }

    pub fn inflate(&self, entry: &Entry) -> io::Result<Vec<u8>> {
        if entry.flags != 1 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "{}: entry has flags {}, DES decryption is not implemented",
                    String::from_utf8_lossy(entry.name),
                    entry.flags
                ),
            ));
        }

        let slice = self.raw(entry).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: entry data at {} (+{} bytes) lies outside the archive",
                    String::from_utf8_lossy(entry.name),
                    entry.position,
                    entry.pack_size
                ),
            )
        })?;

        let mut data = Vec::with_capacity(entry.real_size as usize);
        ZlibDecoder::new(slice).read_to_end(&mut data)?;

        if data.len() != entry.real_size as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{}: inflated to {} bytes, entry header promised {}",
                    String::from_utf8_lossy(entry.name),
                    data.len(),
                    entry.real_size
                ),
            ));
        }

        Ok(data)
    }

    pub fn raw_if_plain(&self, entry: &Entry) -> Option<&[u8]> {
        if entry.flags != FLAG_FILE {
            return None;
        }
        self.raw(entry)
    }
}

impl<'a> Entry<'a> {
    pub fn identity(&self) -> String {
        format!("{:x}-{:x}", self.position, self.real_size)
    }
}

fn proc_status_kb(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(":"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

pub(crate) fn normalize(name: &[u8]) -> Vec<u8> {
    name.iter()
        .map(|&b| {
            if b == b'\\' {
                b'/'
            } else {
                b.to_ascii_lowercase()
            }
        })
        .collect()
}

fn decode_cp949(name: &[u8]) -> Option<String> {
    let (decoded, _, had_errors) = EUC_KR.decode(name);

    if had_errors {
        None
    } else {
        Some(decoded.into_owned())
    }
}

fn to_mojibake(name: &[u8]) -> String {
    name.iter().map(|&b| char::from(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_and_flips_separators() {
        assert_eq!(normalize(br"DATA\Texture\FOO.BMP"), b"data/texture/foo.bmp");
    }

    #[test]
    fn leaves_high_bytes_untouched() {
        assert_eq!(normalize(&[b'A', 0xC0, 0xAF]), vec![b'a', 0xC0, 0xAF]);
    }

    #[test]
    fn is_idempotent() {
        let once = normalize(br"DATA\Foo.BMP");
        assert_eq!(normalize(&once), once);
    }

    #[test]
    fn lowercases_bytes_that_may_be_cp949_trail_bytes() {
        assert_eq!(normalize(&[0xB0, 0x41]), vec![0xB0, 0x61]);
    }

    #[test]
    fn decodes_cp949_names() {
        assert_eq!(
            decode_cp949(b"\xc7\xc1\xb7\xd0\xc5\xd7\xb6\xf3\xc0\xfc\xc1\xfd01.txt").as_deref(),
            Some("프론테라전집01.txt")
        );
    }

    #[test]
    fn passes_ascii_through() {
        assert_eq!(
            decode_cp949(b"data/foo.txt").as_deref(),
            Some("data/foo.txt")
        );
    }

    #[test]
    fn rejects_invalid_cp949() {
        assert_eq!(decode_cp949(&[0x80]), None);
    }

    #[test]
    fn transform_names_to_mojibake() {
        assert_eq!(
            to_mojibake(b"\xc7\xc1\xb7\xd0\xc5\xd7\xb6\xf3\xc0\xfc\xc1\xfd01.txt"),
            "ÇÁ·ÐÅ×¶óÀüÁý01.txt"
        );
    }

    #[test]
    fn reinterprets_bytes_as_latin1() {
        assert_eq!(
            to_mojibake(b"\xc7\xc1\xb7\xd0\xc5\xd7\xb6\xf3\xc0\xfc\xc1\xfd01.txt"),
            "ÇÁ·ÐÅ×¶óÀüÁý01.txt"
        );
    }

    #[test]
    fn mojibake_is_reversible() {
        let raw: &[u8] = b"data/imf/abyss_chaser_\xb3\xb2.imf";
        let recovered: Vec<u8> = to_mojibake(raw).chars().map(|c| c as u8).collect();
        assert_eq!(recovered, raw);
    }
}
