use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    time::Instant,
};

fn main() -> io::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: <program> <path-to-grf>",
        )
    })?;

    println!("path: {}", path);

    let mut file = File::open(&path)?;

    let metadata = file.metadata()?;
    println!("metadata size: {}", metadata.len());

    let mut header = [0u8; 46];
    file.read_exact(&mut header)?;

    println!("{:?}", &header[..16]);

    if !header.starts_with(b"Master of Magic") && !header.starts_with(b"Event Horizon") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{path}: not a GRF archive, signature is {:?}",
                &header[..16]
            ),
        ));
    }

    let version = u32::from_le_bytes(header[42..46].try_into().unwrap());

    if version != 0x200 && version != 0x300 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{path}: unsupported GRF version {version:#x}, expected 0x200 or 0x300"),
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
                "{path}: file table offset {table_offset} lies past the end of a {}-byte file",
                metadata.len()
            ),
        ));
    }

    println!("table_offset: {table_offset}");
    println!("file_count: {real_file_count}");

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
            format!("{path}: empty file table, packed {pack_size} bytes and unpacked {real_size}"),
        ));
    }

    println!("pack_size: {}", pack_size);
    println!("real_size: {}", real_size);

    let t = Instant::now();

    let mut compressed = vec![0u8; pack_size as usize];
    file.read_exact(&mut compressed)?;

    println!("read in {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);

    let t = Instant::now();

    let mut table = Vec::with_capacity(real_size as usize);
    flate2::read::ZlibDecoder::new(&compressed[..]).read_to_end(&mut table)?;

    println!("inflated in {:.1}ms", t.elapsed().as_secs_f64() * 1000.0);

    if table.len() != real_size as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{path}: file table inflated to {} bytes, header promised {real_size}",
                table.len()
            ),
        ));
    }

    Ok(())
}
