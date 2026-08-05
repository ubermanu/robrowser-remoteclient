use std::{io, path::Path};

mod grf;

fn main() -> io::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: <program> <path-to-grf>",
        )
    })?;

    println!("path: {}", path);

    let archive = grf::Archive::open(Path::new(&path))?;

    println!("{:?}", archive.entry_at(0));

    let count = archive.walk();

    if count as u64 != archive.real_file_count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{path}: walked {count} entries, header claims {}",
                archive.real_file_count
            ),
        ));
    }

    println!("entries found: {count}");

    println!("{:?}", archive.lookup(b"data/06guild_r.gat"));

    Ok(())
}
