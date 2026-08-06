//! The mangled DES variant GRF uses for encrypted entries: a single round with
//! hardcoded S-boxes and no key, ported from the reference PHP/C# clients.

const BLOCK_SIZE: usize = 8;

/// Initial permutation.
const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61,
    53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];

/// Final permutation.
const FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];

/// P-box permutation.
const TP: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19,
    13, 30, 6, 22, 11, 4, 25,
];

const S: [[u8; 64]; 4] = [
    [
        0xef, 0x03, 0x41, 0xfd, 0xd8, 0x74, 0x1e, 0x47, 0x26, 0xef, 0xfb, 0x22, 0xb3, 0xd8, 0x84,
        0x1e, 0x39, 0xac, 0xa7, 0x60, 0x62, 0xc1, 0xcd, 0xba, 0x5c, 0x96, 0x90, 0x59, 0x05, 0x3b,
        0x7a, 0x85, 0x40, 0xfd, 0x1e, 0xc8, 0xe7, 0x8a, 0x8b, 0x21, 0xda, 0x43, 0x64, 0x9f, 0x2d,
        0x14, 0xb1, 0x72, 0xf5, 0x5b, 0xc8, 0xb6, 0x9c, 0x37, 0x76, 0xec, 0x39, 0xa0, 0xa3, 0x05,
        0x52, 0x6e, 0x0f, 0xd9,
    ],
    [
        0xa7, 0xdd, 0x0d, 0x78, 0x9e, 0x0b, 0xe3, 0x95, 0x60, 0x36, 0x36, 0x4f, 0xf9, 0x60, 0x5a,
        0xa3, 0x11, 0x24, 0xd2, 0x87, 0xc8, 0x52, 0x75, 0xec, 0xbb, 0xc1, 0x4c, 0xba, 0x24, 0xfe,
        0x8f, 0x19, 0xda, 0x13, 0x66, 0xaf, 0x49, 0xd0, 0x90, 0x06, 0x8c, 0x6a, 0xfb, 0x91, 0x37,
        0x8d, 0x0d, 0x78, 0xbf, 0x49, 0x11, 0xf4, 0x23, 0xe5, 0xce, 0x3b, 0x55, 0xbc, 0xa2, 0x57,
        0xe8, 0x22, 0x74, 0xce,
    ],
    [
        0x2c, 0xea, 0xc1, 0xbf, 0x4a, 0x24, 0x1f, 0xc2, 0x79, 0x47, 0xa2, 0x7c, 0xb6, 0xd9, 0x68,
        0x15, 0x80, 0x56, 0x5d, 0x01, 0x33, 0xfd, 0xf4, 0xae, 0xde, 0x30, 0x07, 0x9b, 0xe5, 0x83,
        0x9b, 0x68, 0x49, 0xb4, 0x2e, 0x83, 0x1f, 0xc2, 0xb5, 0x7c, 0xa2, 0x19, 0xd8, 0xe5, 0x7c,
        0x2f, 0x83, 0xda, 0xf7, 0x6b, 0x90, 0xfe, 0xc4, 0x01, 0x5a, 0x97, 0x61, 0xa6, 0x3d, 0x40,
        0x0b, 0x58, 0xe6, 0x3d,
    ],
    [
        0x4d, 0xd1, 0xb2, 0x0f, 0x28, 0xbd, 0xe4, 0x78, 0xf6, 0x4a, 0x0f, 0x93, 0x8b, 0x17, 0xd1,
        0xa4, 0x3a, 0xec, 0xc9, 0x35, 0x93, 0x56, 0x7e, 0xcb, 0x55, 0x20, 0xa0, 0xfe, 0x6c, 0x89,
        0x17, 0x62, 0x17, 0x62, 0x4b, 0xb1, 0xb4, 0xde, 0xd1, 0x87, 0xc9, 0x14, 0x3c, 0x4a, 0x7e,
        0xa8, 0xe2, 0x7d, 0xa0, 0x9f, 0xf6, 0x5c, 0x6a, 0x09, 0x8d, 0xf0, 0x0f, 0xe3, 0x53, 0x25,
        0x95, 0x36, 0x28, 0xcb,
    ],
];

const MASK: [u8; 8] = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];

/// Extensions whose payload the packer only scrambled, never DES-encrypted
/// past the first 20 blocks.
const PLAIN_EXTENSIONS: [&[u8]; 4] = [b".gnd", b".gat", b".act", b".str"];

/// How many blocks past the header take part in the cycle, derived from the
/// entry's packed size (and forced off for [`PLAIN_EXTENSIONS`]).
pub fn cycle(name: &[u8], pack_size: u32) -> (usize, bool) {
    let lowercased: Vec<u8> = name.to_ascii_lowercase();

    if PLAIN_EXTENSIONS
        .iter()
        .any(|extension| lowercased.ends_with(extension))
    {
        return (0, true);
    }

    let mut cycle = 1;
    let mut i = 10u64;

    while u64::from(pack_size) >= i {
        cycle += 1;
        i *= 10;
    }

    (cycle, false)
}

/// Decrypt the first 20 blocks, all the packer touched.
pub fn decrypt_header(data: &mut [u8]) {
    for block in data.chunks_exact_mut(BLOCK_SIZE).take(20) {
        decrypt_block(block.try_into().unwrap());
    }
}

/// Decrypt the header blocks, then every `cycle`-th block after them; blocks
/// in between get a byte shuffle instead, one in every eight.
pub fn decrypt_mixed(data: &mut [u8], mut cycle: usize, is_data_crypted: bool) {
    if !is_data_crypted {
        if cycle < 3 {
            cycle = 3;
        } else if cycle < 5 {
            cycle += 1;
        } else if cycle < 7 {
            cycle += 9;
        } else {
            cycle += 15;
        }
    }

    let mut count = 0;

    for (i, block) in data.chunks_exact_mut(BLOCK_SIZE).enumerate() {
        let block: &mut [u8; BLOCK_SIZE] = block.try_into().unwrap();

        if i < 20 || (!is_data_crypted && i % cycle == 0) {
            decrypt_block(block);
        } else {
            if count == 7 && !is_data_crypted {
                count = 0;
                shuffle_block(block);
            }
            count += 1;
        }
    }
}

fn shuffle_block(block: &mut [u8; BLOCK_SIZE]) {
    let tmp = *block;

    block[0] = tmp[3];
    block[1] = tmp[4];
    block[2] = tmp[6];
    block[3] = tmp[0];
    block[4] = tmp[1];
    block[5] = tmp[2];
    block[6] = tmp[5];
    block[7] = match tmp[7] {
        0x00 => 0x2b,
        0x2b => 0x00,
        0x01 => 0x68,
        0x68 => 0x01,
        0x48 => 0x77,
        0x77 => 0x48,
        0x60 => 0xff,
        0xff => 0x60,
        0x6c => 0x80,
        0x80 => 0x6c,
        0xb9 => 0xc0,
        0xc0 => 0xb9,
        0xeb => 0xfe,
        0xfe => 0xeb,
        other => other,
    };
}

fn decrypt_block(src: &mut [u8; BLOCK_SIZE]) {
    permute(src, &IP);
    round(src);
    permute(src, &FP);
}

fn permute(src: &mut [u8; BLOCK_SIZE], table: &[u8; 64]) {
    let mut block = [0u8; BLOCK_SIZE];

    for (i, &entry) in table.iter().enumerate() {
        let j = usize::from(entry) - 1;

        if src[(j >> 3) & 7] & MASK[j & 7] != 0 {
            block[(i >> 3) & 7] |= MASK[i & 7];
        }
    }

    *src = block;
}

fn round(src: &mut [u8; BLOCK_SIZE]) {
    let mut block = [0u8; BLOCK_SIZE];

    block[0] = ((src[7] << 5) | (src[4] >> 3)) & 0x3f;
    block[1] = ((src[4] << 1) | (src[5] >> 7)) & 0x3f;
    block[2] = ((src[4] << 5) | (src[5] >> 3)) & 0x3f;
    block[3] = ((src[5] << 1) | (src[6] >> 7)) & 0x3f;
    block[4] = ((src[5] << 5) | (src[6] >> 3)) & 0x3f;
    block[5] = ((src[6] << 1) | (src[7] >> 7)) & 0x3f;
    block[6] = ((src[6] << 5) | (src[7] >> 3)) & 0x3f;
    block[7] = ((src[7] << 1) | (src[4] >> 7)) & 0x3f;

    for i in 0..4 {
        block[i] =
            (S[i][usize::from(block[i * 2])] & 0xf0) | (S[i][usize::from(block[i * 2 + 1])] & 0x0f);
    }

    block[4] = 0;
    block[5] = 0;
    block[6] = 0;
    block[7] = 0;

    for i in 0..32 {
        let j = usize::from(TP[i]) - 1;

        if block[j >> 3] & MASK[j & 7] != 0 {
            block[(i >> 3) + 4] |= MASK[i & 7];
        }
    }

    src[0] ^= block[4];
    src[1] ^= block[5];
    src[2] ^= block[6];
    src[3] ^= block[7];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same sequence the reference PHP client was fed to produce the expected
    /// values below.
    fn sample(blocks: usize) -> Vec<u8> {
        let mut x = 1u64;

        (0..blocks * BLOCK_SIZE)
            .map(|_| {
                x = (x * 1103515245 + 12345) & 0x7fffffff;
                (x >> 16) as u8
            })
            .collect()
    }

    fn block_at(data: &[u8], i: usize) -> [u8; BLOCK_SIZE] {
        data[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE]
            .try_into()
            .unwrap()
    }

    #[test]
    fn decrypts_a_single_block() {
        let mut block = [0xc6, 0x7e, 0x81, 0x6b, 0x4b, 0xfb, 0xe2, 0xfb];
        decrypt_block(&mut block);
        assert_eq!(block, [0x86, 0x7f, 0x84, 0x6b, 0x5b, 0xff, 0xb2, 0xfe]);
    }

    #[test]
    fn header_decryption_stops_after_twenty_blocks() {
        let input = sample(30);
        let mut data = input.clone();

        decrypt_header(&mut data);

        assert_eq!(
            block_at(&data, 19),
            [0x37, 0x2d, 0xd1, 0xd1, 0x48, 0xf1, 0x0d, 0x5c]
        );
        assert_eq!(data[20 * BLOCK_SIZE..], input[20 * BLOCK_SIZE..]);
    }

    #[test]
    fn mixed_decryption_hits_every_cycle_th_block_past_the_header() {
        let input = sample(30);
        let mut data = input.clone();

        // A cycle of 1 is bumped up to 3 for non-scrambled entries.
        decrypt_mixed(&mut data, 1, false);

        assert_eq!(
            data[..20 * BLOCK_SIZE],
            {
                let mut header = input.clone();
                decrypt_header(&mut header);
                header
            }[..20 * BLOCK_SIZE]
        );

        assert_eq!(
            block_at(&data, 21),
            [0x39, 0x74, 0xe3, 0x2f, 0xf5, 0x86, 0xd3, 0x00]
        );
        assert_eq!(block_at(&data, 22), block_at(&input, 22));
        assert_eq!(
            block_at(&data, 24),
            [0xe1, 0x60, 0x24, 0xc6, 0x94, 0x23, 0x95, 0xed]
        );
    }

    #[test]
    fn mixed_decryption_shuffles_every_eighth_untouched_block() {
        let input = sample(40);
        let mut data = input.clone();

        decrypt_mixed(&mut data, 8, false);

        assert_eq!(
            block_at(&data, 28),
            [0xeb, 0xb7, 0x24, 0xac, 0xfb, 0xa0, 0x79, 0x72]
        );
        assert_eq!(
            block_at(&data, 35),
            [0x67, 0x64, 0x69, 0xa1, 0x7b, 0x75, 0x9a, 0xef]
        );
        assert_eq!(block_at(&data, 29), block_at(&input, 29));
    }

    #[test]
    fn scrambled_entries_only_get_their_header_decrypted() {
        let input = sample(30);
        let mut data = input.clone();
        let mut expected = input.clone();

        decrypt_mixed(&mut data, 0, true);
        decrypt_header(&mut expected);

        assert_eq!(data, expected);
    }

    #[test]
    fn cycle_grows_with_the_packed_size() {
        assert_eq!(cycle(b"data\\foo.gr2", 9), (1, false));
        assert_eq!(cycle(b"data\\foo.gr2", 10), (2, false));
        assert_eq!(cycle(b"data\\foo.gr2", 12_345), (5, false));
    }

    #[test]
    fn scrambled_extensions_skip_the_cycle() {
        assert_eq!(cycle(b"data\\prontera.GND", 12_345), (0, true));
        assert_eq!(cycle(b"data\\prontera.gat", 12_345), (0, true));
    }
}
