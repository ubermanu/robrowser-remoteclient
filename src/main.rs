use std::{io, path::Path, time::Instant};

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

    println!(
        "{:?}",
        archive
            .lookup("data/book/프론테라전집01.txt".as_bytes())
            .is_some()
    );

    let data = archive.inflate(&archive.lookup(b"data/06guild_r.gat").unwrap())?;
    println!("{} bytes, first 4: {:?}", data.len(), &data[..4]);

    let entry = archive.lookup(b"data/06guild_r.gat").unwrap();
    let t = Instant::now();
    for _ in 0..1000 {
        let _ = archive.inflate(&entry)?;
    }
    println!(
        "1000 extracts in {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    for path in [
        b"data/book/\xc7\xc1\xb7\xd0\xc5\xd7\xb6\xf3\xc0\xfc\xc1\xfd01.txt".as_slice(),
        "data/book/프론테라전집01.txt".as_bytes(),
        "data/book/ÇÁ·ÐÅ×¶óÀüÁý01.txt".as_bytes(),
        b"data/nope.txt".as_slice(),
    ] {
        println!("{:?}", archive.read(path)?.map(|data| data.len()));
    }

    Ok(())
}
