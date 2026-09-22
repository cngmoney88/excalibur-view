//! Locking a drawing set with a password.
//!
//! One kind of encryption and one only: AES-256 under the standard security
//! handler at revision 6, which is what PDF 2.0 specifies and what Acrobat X
//! and everything after it, Revu, and every current reader opens. The older
//! kinds — RC4 at forty and a hundred and twenty-eight bits — are still legal
//! in the format and are no longer worth calling security, so Hyperview reads
//! nothing into them and writes none of them.
//!
//! What this is honest about:
//!
//! * A **password to open** really keeps the set shut. Without it the bytes are
//!   ciphertext and nothing gets in.
//! * **Permissions** — may not print, may not copy text — are a request the
//!   format makes of the reader, not a lock. Acrobat honours them; a
//!   determined person with a different program does not have to. That is how
//!   the format works, and telling somebody their drawings cannot be printed
//!   when they can is worse than telling them the truth.
//!
//! Both sentences appear in the window, because the difference between them is
//! the difference between a set that is safe to send and one that only feels
//! like it.

use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::object::{Dict, Object};

/// What a reader is asked to allow. The bits are the ones in the format; the
/// names are what they mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowed {
    pub print: bool,
    /// Change anything: insert sheets, rotate them, edit the drawing.
    pub change: bool,
    /// Copy text and pictures out.
    pub copy: bool,
    /// Add or change markups.
    pub annotate: bool,
    /// Fill in form fields, even where changing is not allowed.
    pub fill_forms: bool,
    /// Pull the text out for a screen reader.
    pub read_aloud: bool,
    /// Take sheets out, put sheets in, rotate — without the rest of change.
    pub assemble: bool,
    /// Print at full resolution rather than as a coarse picture.
    pub print_well: bool,
}

impl Default for Allowed {
    fn default() -> Allowed {
        // Everything, which is what a file with no security means.
        Allowed {
            print: true,
            change: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            read_aloud: true,
            assemble: true,
            print_well: true,
        }
    }
}

impl Allowed {
    /// Nothing but opening and reading it on the screen.
    pub fn read_only() -> Allowed {
        Allowed {
            print: false,
            change: false,
            copy: false,
            annotate: false,
            fill_forms: false,
            read_aloud: true,
            assemble: false,
            print_well: false,
        }
    }

    /// The permissions word the format uses: a 32-bit field where a bit set
    /// means allowed, and every bit the format does not define is set.
    pub fn as_bits(&self) -> i32 {
        // Bits 1 and 2 are reserved and always 0; the rest of the unused ones
        // are 1. Counting from bit 1 as the lowest.
        let mut bits: u32 = 0xFFFF_FFFC;
        let mut set = |bit: u32, on: bool| {
            let mask = 1u32 << (bit - 1);
            if on {
                bits |= mask;
            } else {
                bits &= !mask;
            }
        };
        set(3, self.print);
        set(4, self.change);
        set(5, self.copy);
        set(6, self.annotate);
        set(9, self.fill_forms);
        set(10, self.read_aloud);
        set(11, self.assemble);
        set(12, self.print_well);
        bits as i32
    }

    pub fn from_bits(bits: i32) -> Allowed {
        let bits = bits as u32;
        let on = |bit: u32| bits & (1u32 << (bit - 1)) != 0;
        Allowed {
            print: on(3),
            change: on(4),
            copy: on(5),
            annotate: on(6),
            fill_forms: on(9),
            read_aloud: on(10),
            assemble: on(11),
            print_well: on(12),
        }
    }

    /// What this comes to in a sentence, for somebody deciding.
    pub fn in_words(&self) -> String {
        let mut not_allowed = Vec::new();
        if !self.print {
            not_allowed.push("print it");
        } else if !self.print_well {
            not_allowed.push("print it at full size");
        }
        if !self.copy {
            not_allowed.push("copy anything out of it");
        }
        if !self.change {
            not_allowed.push("change it");
        }
        if !self.annotate {
            not_allowed.push("mark it up");
        }
        if !self.assemble {
            not_allowed.push("take sheets out or put sheets in");
        }
        if not_allowed.is_empty() {
            return "Anybody who can open it may do anything with it.".into();
        }
        format!(
            "A reader is asked not to let anybody {}.",
            join_words(&not_allowed)
        )
    }
}

fn join_words(list: &[&str]) -> String {
    match list.len() {
        0 => String::new(),
        1 => list[0].to_string(),
        2 => format!("{} or {}", list[0], list[1]),
        _ => format!("{}, or {}", list[..list.len() - 1].join(", "), list[list.len() - 1]),
    }
}

/// The hardened hash the format calls Algorithm 2.B.
///
/// Deliberately slow and deliberately awkward: it is what makes guessing a
/// password expensive. Written out from the specification rather than
/// simplified, because a shortcut here produces a file that opens nowhere.
fn hardened(password: &[u8], salt: &[u8], extra: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    hasher.update(extra);
    let mut k: Vec<u8> = hasher.finalize().to_vec();

    let mut round = 0usize;
    loop {
        // K1 is the password, K and the extra data, sixty-four times over.
        let mut k1 = Vec::with_capacity((password.len() + k.len() + extra.len()) * 64);
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(extra);
        }

        // Encrypted with the first sixteen bytes of K as the key and the next
        // sixteen as the vector, with no padding.
        let mut iv = [0u8; 16];
        iv.copy_from_slice(&k[16..32]);
        let Some(e) = crate::aes::cbc_nopad(&k[..16], iv, &k1, true) else {
            return [0u8; 32];
        };

        // Which hash comes next is decided by the first sixteen bytes of E,
        // summed and taken modulo three.
        let sum: u32 = e[..16].iter().map(|b| *b as u32).sum();
        k = match sum % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };

        round += 1;
        // At least sixty-four rounds, then until the last byte of E is no
        // bigger than the number of rounds already done past that.
        if round >= 64 && (*e.last().unwrap_or(&0) as usize) <= round - 32 {
            break;
        }
        if round > 512 {
            // The specification's loop always ends; this is a belt in case a
            // file is built to make it not.
            break;
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&k[..32]);
    out
}

/// A password as the format wants it: UTF-8, at most 127 bytes.
fn prepared(password: &str) -> Vec<u8> {
    let mut bytes = password.as_bytes().to_vec();
    bytes.truncate(127);
    bytes
}

/// Everything that goes in the file's `/Encrypt` dictionary, and the key the
/// rest of the file is locked with.
pub struct Locked {
    pub encrypt: Dict,
    pub key: [u8; 32],
}

/// Works out the encryption dictionary for a set of passwords and permissions.
///
/// `randomness` must be 32 bytes nobody can guess: sixteen bytes of salt for
/// each of the two passwords. It is passed in rather than made here so the
/// tests can be repeatable and the caller decides where its randomness comes
/// from.
pub fn lock(
    user_password: &str,
    owner_password: &str,
    allowed: Allowed,
    file_key: [u8; 32],
    randomness: [u8; 32],
) -> Locked {
    let user = prepared(user_password);
    // An owner password nobody set is the user's, which is what every other
    // program does: it means "the person who can open it can change it".
    let owner = if owner_password.is_empty() {
        user.clone()
    } else {
        prepared(owner_password)
    };

    let user_validation = &randomness[0..8];
    let user_salt = &randomness[8..16];
    let owner_validation = &randomness[16..24];
    let owner_salt = &randomness[24..32];

    // /U is the hash the reader checks a typed password against, with both
    // salts appended so it can do the check.
    let mut u = Vec::with_capacity(48);
    u.extend_from_slice(&hardened(&user, user_validation, &[]));
    u.extend_from_slice(user_validation);
    u.extend_from_slice(user_salt);

    // /UE wraps the file key so the user's password can unwrap it.
    let user_key = hardened(&user, user_salt, &[]);
    let ue = crate::aes::cbc_nopad(&user_key, [0u8; 16], &file_key, true)
        .unwrap_or_else(|| vec![0u8; 32]);

    // The owner's hashes take /U as extra data, which is what ties the owner
    // password to this particular file.
    let mut o = Vec::with_capacity(48);
    o.extend_from_slice(&hardened(&owner, owner_validation, &u));
    o.extend_from_slice(owner_validation);
    o.extend_from_slice(owner_salt);
    let owner_key = hardened(&owner, owner_salt, &u);
    let oe = crate::aes::cbc_nopad(&owner_key, [0u8; 16], &file_key, true)
        .unwrap_or_else(|| vec![0u8; 32]);

    // /Perms is the permissions word, encrypted with the file key, so a reader
    // can tell whether somebody edited the number in the dictionary.
    let bits = allowed.as_bits();
    let mut perms = [0u8; 16];
    perms[..4].copy_from_slice(&(bits as u32).to_le_bytes());
    perms[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    // 'T': the metadata is encrypted too.
    perms[8] = b'T';
    perms[9] = b'a';
    perms[10] = b'd';
    perms[11] = b'b';
    perms[12..16].copy_from_slice(&[0, 0, 0, 0]);
    let perms_encrypted =
        crate::aes::encrypt_ecb_nopad(&file_key, &perms).unwrap_or_else(|| vec![0u8; 16]);

    let mut crypt_filter = Dict::new();
    crypt_filter.set("Type", Object::name("CryptFilter"));
    crypt_filter.set("CFM", Object::name("AESV3"));
    crypt_filter.set("Length", Object::Int(32));
    crypt_filter.set("AuthEvent", Object::name("DocOpen"));
    let mut filters = Dict::new();
    filters.set("StdCF", Object::Dict(crypt_filter));

    let mut encrypt = Dict::new();
    encrypt.set("Filter", Object::name("Standard"));
    encrypt.set("V", Object::Int(5));
    encrypt.set("R", Object::Int(6));
    encrypt.set("Length", Object::Int(256));
    encrypt.set("CF", Object::Dict(filters));
    encrypt.set("StmF", Object::name("StdCF"));
    encrypt.set("StrF", Object::name("StdCF"));
    encrypt.set("P", Object::Int(bits as i64));
    encrypt.set("U", Object::bytes(&u));
    encrypt.set("UE", Object::bytes(&ue));
    encrypt.set("O", Object::bytes(&o));
    encrypt.set("OE", Object::bytes(&oe));
    encrypt.set("Perms", Object::bytes(&perms_encrypted));
    encrypt.set("EncryptMetadata", Object::Bool(true));

    Locked {
        encrypt,
        key: file_key,
    }
}

/// Whether a typed password opens a file, and the key it unlocks.
///
/// Used to check a password before a set is written out under it, so nobody
/// locks a drawing set with a password that turns out not to work.
pub fn unlock(encrypt: &Dict, password: &str) -> Option<[u8; 32]> {
    let u = encrypt.get("U")?.as_bytes()?;
    if u.len() < 48 {
        return None;
    }
    let typed = prepared(password);
    let validation = &u[32..40];
    let salt = &u[40..48];
    if hardened(&typed, validation, &[]) != u[..32] {
        // Not the user's password. It may still be the owner's.
        let o = encrypt.get("O")?.as_bytes()?;
        if o.len() < 48 {
            return None;
        }
        let validation = &o[32..40];
        let salt = &o[40..48];
        if hardened(&typed, validation, &u[..48]) != o[..32] {
            return None;
        }
        let key = hardened(&typed, salt, &u[..48]);
        let oe = encrypt.get("OE")?.as_bytes()?;
        let out = crate::aes::cbc_nopad(&key, [0u8; 16], oe, false)?;
        let mut file_key = [0u8; 32];
        file_key.copy_from_slice(&out[..32]);
        return Some(file_key);
    }
    let key = hardened(&typed, salt, &[]);
    let ue = encrypt.get("UE")?.as_bytes()?;
    let out = crate::aes::cbc_nopad(&key, [0u8; 16], ue, false)?;
    let mut file_key = [0u8; 32];
    file_key.copy_from_slice(&out[..32]);
    Some(file_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        for (at, byte) in key.iter_mut().enumerate() {
            *byte = (at as u8).wrapping_mul(7).wrapping_add(3);
        }
        key
    }

    fn some_randomness() -> [u8; 32] {
        let mut out = [0u8; 32];
        for (at, byte) in out.iter_mut().enumerate() {
            *byte = (at as u8).wrapping_mul(11).wrapping_add(29);
        }
        out
    }

    #[test]
    fn the_password_that_locked_it_opens_it() {
        let locked = lock("bolt", "", Allowed::default(), a_key(), some_randomness());
        assert_eq!(unlock(&locked.encrypt, "bolt"), Some(a_key()));
    }

    #[test]
    fn another_password_does_not() {
        let locked = lock("bolt", "", Allowed::default(), a_key(), some_randomness());
        assert!(unlock(&locked.encrypt, "bolts").is_none());
        assert!(unlock(&locked.encrypt, "").is_none());
        assert!(unlock(&locked.encrypt, "BOLT").is_none());
    }

    #[test]
    fn the_owner_password_opens_it_as_well() {
        // It has to: the owner is the person who set the permissions, and they
        // must be able to get back in to change them.
        let locked = lock("reader", "creede", Allowed::read_only(), a_key(), some_randomness());
        assert_eq!(unlock(&locked.encrypt, "reader"), Some(a_key()));
        assert_eq!(unlock(&locked.encrypt, "creede"), Some(a_key()));
    }

    #[test]
    fn a_file_anybody_may_open_still_locks_its_bytes() {
        // An empty user password means no prompt, and the file is still
        // encrypted — which is what "permissions only" means in this format.
        let locked = lock("", "creede", Allowed::read_only(), a_key(), some_randomness());
        assert_eq!(unlock(&locked.encrypt, ""), Some(a_key()));
        assert_eq!(unlock(&locked.encrypt, "creede"), Some(a_key()));
        assert!(unlock(&locked.encrypt, "nonsense").is_none());
    }

    #[test]
    fn the_dictionary_says_what_every_reader_needs_to_know() {
        let locked = lock("bolt", "", Allowed::default(), a_key(), some_randomness());
        let d = &locked.encrypt;
        assert_eq!(d.get("V").and_then(|o| o.as_i64()), Some(5));
        assert_eq!(d.get("R").and_then(|o| o.as_i64()), Some(6));
        assert_eq!(d.get("Length").and_then(|o| o.as_i64()), Some(256));
        assert_eq!(d.get("U").and_then(|o| o.as_bytes()).map(|b| b.len()), Some(48));
        assert_eq!(d.get("O").and_then(|o| o.as_bytes()).map(|b| b.len()), Some(48));
        assert_eq!(d.get("UE").and_then(|o| o.as_bytes()).map(|b| b.len()), Some(32));
        assert_eq!(d.get("OE").and_then(|o| o.as_bytes()).map(|b| b.len()), Some(32));
        assert_eq!(d.get("Perms").and_then(|o| o.as_bytes()).map(|b| b.len()), Some(16));
    }

    #[test]
    fn the_permission_bits_are_the_ones_the_format_defines() {
        let all = Allowed::default();
        // Every allowed bit set, and the two reserved low bits clear.
        assert_eq!(all.as_bits() as u32 & 0b11, 0);
        assert_eq!(Allowed::from_bits(all.as_bits()), all);

        let none = Allowed::read_only();
        assert_eq!(Allowed::from_bits(none.as_bits()), none);
        assert!(none.as_bits() != all.as_bits());
    }

    #[test]
    fn what_is_not_allowed_is_said_in_words_somebody_can_check() {
        let none = Allowed::read_only();
        let said = none.in_words();
        assert!(said.contains("print it"), "{said}");
        assert!(said.contains("copy"), "{said}");
        assert_eq!(
            Allowed::default().in_words(),
            "Anybody who can open it may do anything with it."
        );
    }

    #[test]
    fn a_password_longer_than_the_format_allows_is_cut_rather_than_refused() {
        // 127 bytes is the limit. Cutting it and saying nothing would be
        // wrong; cutting it consistently at both ends is what makes the long
        // password still work.
        let long = "x".repeat(300);
        let locked = lock(&long, "", Allowed::default(), a_key(), some_randomness());
        assert_eq!(unlock(&locked.encrypt, &long), Some(a_key()));
    }

    #[test]
    fn two_files_locked_with_the_same_password_do_not_look_alike() {
        // Different randomness means different hashes, which is what stops one
        // cracked file helping with the next.
        let first = lock("bolt", "", Allowed::default(), a_key(), some_randomness());
        let mut other = some_randomness();
        other[0] ^= 0xFF;
        let second = lock("bolt", "", Allowed::default(), a_key(), other);
        assert_ne!(
            first.encrypt.get("U").and_then(|o| o.as_bytes()),
            second.encrypt.get("U").and_then(|o| o.as_bytes())
        );
    }
}

// ---- writing the locked file -----------------------------------------------

/// Writes a whole file out with every string and every stream encrypted.
///
/// A complete rewrite rather than an incremental update, and it has to be:
/// encryption applies to every object in the file, so appending would leave
/// the original bytes sitting in plain sight underneath. Objects that live
/// inside object streams are lifted out and written as ordinary objects,
/// because an object stream in an encrypted file is one more place for a
/// reader to disagree.
pub fn write_locked(doc: &crate::Document, locked: &Locked, id: [u8; 16]) -> Vec<u8> {
    use crate::object::Ref;

    let mut numbers: Vec<u32> = doc.xref.slots.keys().copied().collect();
    numbers.sort_unstable();

    let mut out: Vec<u8> = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<(u32, usize)> = Vec::new();
    let mut highest = 0u32;

    for number in &numbers {
        let reference = Ref {
            number: *number,
            generation: 0,
        };
        let object = doc.get(reference);
        // The old cross-reference machinery is being replaced, so the objects
        // that made it up have no place in the new file.
        if let Some(dict) = object.as_dict() {
            let kind = dict
                .get("Type")
                .and_then(|o| o.as_name())
                .map(|n| n.as_str().to_string())
                .unwrap_or_default();
            if kind == "ObjStm" || kind == "XRef" {
                continue;
            }
        }
        if matches!(*object, Object::Null) {
            continue;
        }

        let mut locked_object = (*object).clone();
        encrypt_in_place(&mut locked_object, &locked.key, reference);

        offsets.push((*number, out.len()));
        highest = highest.max(*number);
        crate::write::write_indirect(reference, &locked_object, &mut out);
    }

    // The encryption dictionary itself is never encrypted: a reader has to be
    // able to read it in order to know how to read anything else.
    let encrypt_ref = Ref {
        number: highest + 1,
        generation: 0,
    };
    offsets.push((encrypt_ref.number, out.len()));
    crate::write::write_indirect(
        encrypt_ref,
        &Object::Dict(locked.encrypt.clone()),
        &mut out,
    );
    highest += 1;

    let xref_at = out.len();
    let mut table: std::collections::BTreeMap<u32, usize> = Default::default();
    for (number, offset) in offsets {
        table.insert(number, offset);
    }
    out.extend_from_slice(format!("xref\n0 {}\n", highest + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for number in 1..=highest {
        match table.get(&number) {
            Some(offset) => {
                out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }

    let mut trailer = Dict::new();
    trailer.set("Size", Object::Int(highest as i64 + 1));
    if let Some(root) = doc.xref.trailer.get("Root") {
        trailer.set("Root", root.clone());
    }
    if let Some(info) = doc.xref.trailer.get("Info") {
        trailer.set("Info", info.clone());
    }
    trailer.set("Encrypt", Object::Ref(encrypt_ref));
    // The identifier is part of what a reader checks, and it is not encrypted.
    trailer.set(
        "ID",
        Object::Array(vec![Object::bytes(&id), Object::bytes(&id)]),
    );
    out.extend_from_slice(b"trailer\n");
    crate::write::write_object(&Object::Dict(trailer), &mut out);
    out.extend_from_slice(format!("\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
    out
}

/// Encrypts every string and every stream inside one object.
fn encrypt_in_place(object: &mut Object, key: &[u8; 32], reference: crate::object::Ref) {
    // Revision 6 uses the file key for everything; there is no per-object key
    // the way there was with the older handlers.
    let iv = vector_for(reference);
    match object {
        Object::String(bytes, kind) => {
            if let Some(locked) = crate::aes::encrypt_cbc(key, iv, bytes) {
                *bytes = locked;
                *kind = crate::object::StringKind::Literal;
            }
        }
        Object::Array(items) => {
            for item in items.iter_mut() {
                encrypt_in_place(item, key, reference);
            }
        }
        Object::Dict(dict) => {
            encrypt_dict(dict, key, reference);
        }
        Object::Stream(stream) => {
            encrypt_dict(&mut stream.dict, key, reference);
            if let Some(locked) = crate::aes::encrypt_cbc(key, iv, &stream.data) {
                stream.dict.set("Length", Object::Int(locked.len() as i64));
                stream.data = locked;
            }
        }
        _ => {}
    }
}

fn encrypt_dict(dict: &mut Dict, key: &[u8; 32], reference: crate::object::Ref) {
    let keys: Vec<String> = dict.iter().map(|(k, _)| k.as_str().to_string()).collect();
    for name in keys {
        // The length of a stream is a number about the file, not content, and
        // it is rewritten anyway.
        if name == "Length" {
            continue;
        }
        if let Some(value) = dict.get(&name).cloned() {
            let mut value = value;
            encrypt_in_place(&mut value, key, reference);
            dict.set(name.as_str(), value);
        }
    }
}

/// The initialisation vector for one object.
///
/// Different for every object so that two identical strings in a file do not
/// come out as identical ciphertext, which would say something about what is
/// in them.
fn vector_for(reference: crate::object::Ref) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(reference.number.to_le_bytes());
    hasher.update(reference.generation.to_le_bytes());
    hasher.update(b"Excalibur Hyperview vector");
    let digest = hasher.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}
