//! AES, written out rather than pulled in.
//!
//! This is here because a drawing set locked with a password has to be a
//! drawing set that opens — in Acrobat, in Revu, in a browser, on a general
//! contractor's machine — and the only way to be sure of that is to write the
//! standard algorithm and check it against the published answers. The tests at
//! the bottom are the vectors from FIPS-197 and NIST SP 800-38A, which is what
//! makes this trustworthy rather than merely present.
//!
//! Only what a PDF needs: AES-256 and AES-128, in CBC with the initialisation
//! vector at the front of the data, which is exactly what a PDF's `AESV3` and
//! `AESV2` crypt filters are.

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

fn inverse_sbox() -> [u8; 256] {
    let mut out = [0u8; 256];
    for (at, value) in SBOX.iter().enumerate() {
        out[*value as usize] = at as u8;
    }
    out
}

/// Multiplication in the field the cipher is built on.
fn times(a: u8, b: u8) -> u8 {
    let mut a = a;
    let mut b = b;
    let mut out = 0u8;
    for _ in 0..8 {
        if b & 1 != 0 {
            out ^= a;
        }
        let high = a & 0x80;
        a <<= 1;
        if high != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    out
}

/// An expanded key, ready to encrypt blocks with.
pub struct Key {
    round_keys: Vec<[u8; 16]>,
    rounds: usize,
}

impl Key {
    /// From 16 bytes (AES-128) or 32 (AES-256). Any other length is refused
    /// rather than padded, because a key of the wrong length is a mistake and
    /// quietly fixing it would produce a file nothing else can open.
    pub fn new(key: &[u8]) -> Option<Key> {
        let (words, rounds) = match key.len() {
            16 => (4usize, 10usize),
            24 => (6, 12),
            32 => (8, 14),
            _ => return None,
        };
        let total = 4 * (rounds + 1);
        let mut schedule: Vec<[u8; 4]> = Vec::with_capacity(total);
        for chunk in key.chunks(4) {
            schedule.push([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let mut rcon = 1u8;
        for at in words..total {
            let mut word = schedule[at - 1];
            if at % words == 0 {
                word = [
                    SBOX[word[1] as usize] ^ rcon,
                    SBOX[word[2] as usize],
                    SBOX[word[3] as usize],
                    SBOX[word[0] as usize],
                ];
                rcon = times(rcon, 2);
            } else if words > 6 && at % words == 4 {
                word = [
                    SBOX[word[0] as usize],
                    SBOX[word[1] as usize],
                    SBOX[word[2] as usize],
                    SBOX[word[3] as usize],
                ];
            }
            let before = schedule[at - words];
            schedule.push([
                before[0] ^ word[0],
                before[1] ^ word[1],
                before[2] ^ word[2],
                before[3] ^ word[3],
            ]);
        }

        let mut round_keys = Vec::with_capacity(rounds + 1);
        for round in 0..=rounds {
            let mut block = [0u8; 16];
            for word in 0..4 {
                let w = schedule[round * 4 + word];
                block[word * 4..word * 4 + 4].copy_from_slice(&w);
            }
            round_keys.push(block);
        }
        Some(Key { round_keys, rounds })
    }

    fn encrypt_block(&self, block: &mut [u8; 16]) {
        add_round_key(block, &self.round_keys[0]);
        for round in 1..self.rounds {
            sub_bytes(block);
            shift_rows(block);
            mix_columns(block);
            add_round_key(block, &self.round_keys[round]);
        }
        sub_bytes(block);
        shift_rows(block);
        add_round_key(block, &self.round_keys[self.rounds]);
    }

    fn decrypt_block(&self, block: &mut [u8; 16]) {
        let inverse = inverse_sbox();
        add_round_key(block, &self.round_keys[self.rounds]);
        for round in (1..self.rounds).rev() {
            inv_shift_rows(block);
            inv_sub_bytes(block, &inverse);
            add_round_key(block, &self.round_keys[round]);
            inv_mix_columns(block);
        }
        inv_shift_rows(block);
        inv_sub_bytes(block, &inverse);
        add_round_key(block, &self.round_keys[0]);
    }
}

fn add_round_key(block: &mut [u8; 16], key: &[u8; 16]) {
    for at in 0..16 {
        block[at] ^= key[at];
    }
}

fn sub_bytes(block: &mut [u8; 16]) {
    for byte in block.iter_mut() {
        *byte = SBOX[*byte as usize];
    }
}

fn inv_sub_bytes(block: &mut [u8; 16], inverse: &[u8; 256]) {
    for byte in block.iter_mut() {
        *byte = inverse[*byte as usize];
    }
}

fn shift_rows(block: &mut [u8; 16]) {
    let was = *block;
    for row in 1..4 {
        for column in 0..4 {
            block[column * 4 + row] = was[((column + row) % 4) * 4 + row];
        }
    }
}

fn inv_shift_rows(block: &mut [u8; 16]) {
    let was = *block;
    for row in 1..4 {
        for column in 0..4 {
            block[((column + row) % 4) * 4 + row] = was[column * 4 + row];
        }
    }
}

fn mix_columns(block: &mut [u8; 16]) {
    for column in 0..4 {
        let at = column * 4;
        let a = [block[at], block[at + 1], block[at + 2], block[at + 3]];
        block[at] = times(a[0], 2) ^ times(a[1], 3) ^ a[2] ^ a[3];
        block[at + 1] = a[0] ^ times(a[1], 2) ^ times(a[2], 3) ^ a[3];
        block[at + 2] = a[0] ^ a[1] ^ times(a[2], 2) ^ times(a[3], 3);
        block[at + 3] = times(a[0], 3) ^ a[1] ^ a[2] ^ times(a[3], 2);
    }
}

fn inv_mix_columns(block: &mut [u8; 16]) {
    for column in 0..4 {
        let at = column * 4;
        let a = [block[at], block[at + 1], block[at + 2], block[at + 3]];
        block[at] = times(a[0], 14) ^ times(a[1], 11) ^ times(a[2], 13) ^ times(a[3], 9);
        block[at + 1] = times(a[0], 9) ^ times(a[1], 14) ^ times(a[2], 11) ^ times(a[3], 13);
        block[at + 2] = times(a[0], 13) ^ times(a[1], 9) ^ times(a[2], 14) ^ times(a[3], 11);
        block[at + 3] = times(a[0], 11) ^ times(a[1], 13) ^ times(a[2], 9) ^ times(a[3], 14);
    }
}

/// Encrypts in CBC, padding the way PKCS#7 says and putting the
/// initialisation vector at the front — which is what a PDF's AES crypt
/// filter expects to find there.
pub fn encrypt_cbc(key: &[u8], iv: [u8; 16], data: &[u8]) -> Option<Vec<u8>> {
    let key = Key::new(key)?;
    let mut out = Vec::with_capacity(data.len() + 32);
    out.extend_from_slice(&iv);
    let mut previous = iv;
    // PKCS#7: always at least one byte of padding, so the length of the
    // original is never in doubt.
    let pad = 16 - (data.len() % 16);
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat(pad as u8).take(pad));
    for chunk in padded.chunks(16) {
        let mut block = [0u8; 16];
        for at in 0..16 {
            block[at] = chunk[at] ^ previous[at];
        }
        key.encrypt_block(&mut block);
        out.extend_from_slice(&block);
        previous = block;
    }
    Some(out)
}

/// The other way, for reading a file somebody else locked.
pub fn decrypt_cbc(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 32 || (data.len() - 16) % 16 != 0 {
        return None;
    }
    let key = Key::new(key)?;
    let mut previous = [0u8; 16];
    previous.copy_from_slice(&data[..16]);
    let mut out = Vec::with_capacity(data.len() - 16);
    for chunk in data[16..].chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        let was = block;
        key.decrypt_block(&mut block);
        for at in 0..16 {
            block[at] ^= previous[at];
        }
        out.extend_from_slice(&block);
        previous = was;
    }
    let pad = *out.last()? as usize;
    if pad == 0 || pad > 16 || pad > out.len() {
        return None;
    }
    out.truncate(out.len() - pad);
    Some(out)
}

/// Encrypts without an initialisation vector and without padding, which is
/// what a PDF's `/Perms` entry and the key-unwrapping steps use.
pub fn encrypt_ecb_nopad(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() % 16 != 0 {
        return None;
    }
    let key = Key::new(key)?;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        key.encrypt_block(&mut block);
        out.extend_from_slice(&block);
    }
    Some(out)
}

/// CBC with no padding and no leading vector, which is what PDF 2.0 uses to
/// wrap the file key into `/UE` and `/OE`.
pub fn cbc_nopad(key: &[u8], iv: [u8; 16], data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    if data.len() % 16 != 0 {
        return None;
    }
    let key = Key::new(key)?;
    let mut previous = iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        if encrypt {
            for at in 0..16 {
                block[at] ^= previous[at];
            }
            key.encrypt_block(&mut block);
            previous = block;
        } else {
            let was = block;
            key.decrypt_block(&mut block);
            for at in 0..16 {
                block[at] ^= previous[at];
            }
            previous = was;
        }
        out.extend_from_slice(&block);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    }

    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_fips_197_answer_for_aes_128() {
        // The worked example in the standard itself. If this is wrong,
        // everything built on it is wrong, and a locked drawing set is one
        // nobody can open.
        let key = hex("000102030405060708090a0b0c0d0e0f");
        let plain = hex("00112233445566778899aabbccddeeff");
        let mut block = [0u8; 16];
        block.copy_from_slice(&plain);
        Key::new(&key).unwrap().encrypt_block(&mut block);
        assert_eq!(to_hex(&block), "69c4e0d86a7b0430d8cdb78070b4c55a");
    }

    #[test]
    fn the_fips_197_answer_for_aes_256() {
        let key = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let plain = hex("00112233445566778899aabbccddeeff");
        let mut block = [0u8; 16];
        block.copy_from_slice(&plain);
        Key::new(&key).unwrap().encrypt_block(&mut block);
        assert_eq!(to_hex(&block), "8ea2b7ca516745bfeafc49904b496089");
    }

    #[test]
    fn the_fips_197_answer_for_aes_192() {
        let key = hex("000102030405060708090a0b0c0d0e0f1011121314151617");
        let plain = hex("00112233445566778899aabbccddeeff");
        let mut block = [0u8; 16];
        block.copy_from_slice(&plain);
        Key::new(&key).unwrap().encrypt_block(&mut block);
        assert_eq!(to_hex(&block), "dda97ca4864cdfe06eaf70a0ec0d7191");
    }

    #[test]
    fn a_block_decrypts_back_to_what_it_was() {
        let key = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let plain = hex("00112233445566778899aabbccddeeff");
        let mut block = [0u8; 16];
        block.copy_from_slice(&plain);
        let key = Key::new(&key).unwrap();
        key.encrypt_block(&mut block);
        key.decrypt_block(&mut block);
        assert_eq!(block.to_vec(), plain);
    }

    #[test]
    fn the_nist_cbc_answer_for_aes_256() {
        // SP 800-38A, F.2.5. The first block of the worked example.
        let key = hex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4");
        let iv = hex("000102030405060708090a0b0c0d0e0f");
        let plain = hex("6bc1bee22e409f96e93d7e117393172a");
        let mut vector = [0u8; 16];
        vector.copy_from_slice(&iv);
        let out = encrypt_cbc(&key, vector, &plain).unwrap();
        // Past the vector at the front, the first block is the published one.
        assert_eq!(to_hex(&out[16..32]), "f58c4c04d6e5f1ba779eabfb5f7bfbd6");
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused_rather_than_padded() {
        // Quietly fixing it would produce a file nothing else can open.
        assert!(Key::new(&[0u8; 20]).is_none());
        assert!(Key::new(&[]).is_none());
        assert!(encrypt_cbc(&[0u8; 7], [0u8; 16], b"hello").is_none());
    }

    #[test]
    fn what_goes_in_comes_back_out_whatever_its_length() {
        let key = [7u8; 32];
        for length in [0usize, 1, 15, 16, 17, 100, 1000] {
            let data: Vec<u8> = (0..length).map(|i| (i * 7 % 251) as u8).collect();
            let locked = encrypt_cbc(&key, [3u8; 16], &data).unwrap();
            assert!(locked.len() > data.len(), "there is always padding");
            assert_eq!(decrypt_cbc(&key, &locked).unwrap(), data, "length {length}");
        }
    }

    #[test]
    fn the_wrong_key_does_not_give_back_the_right_answer() {
        let locked = encrypt_cbc(&[7u8; 32], [3u8; 16], b"W12x26 AT GRID 4").unwrap();
        let wrong = decrypt_cbc(&[8u8; 32], &locked);
        assert!(wrong.map(|v| v != b"W12x26 AT GRID 4").unwrap_or(true));
    }

    #[test]
    fn the_no_padding_form_round_trips() {
        let key = [9u8; 32];
        let data = [4u8; 32];
        let locked = cbc_nopad(&key, [0u8; 16], &data, true).unwrap();
        assert_eq!(locked.len(), 32, "no padding means no growth");
        let back = cbc_nopad(&key, [0u8; 16], &locked, false).unwrap();
        assert_eq!(back, data);
    }
}
