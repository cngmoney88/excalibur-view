//! Opening a set somebody else locked.
//!
//! [`crate::crypt`] writes one kind of lock and one only — AES-256 at revision
//! 6 — because that is the only kind still worth calling security. Reading is a
//! different job with a different rule. What arrives from a general contractor
//! was locked by whatever they happened to have, which is often Acrobat from
//! 2006, and "we cannot open your drawings" is not an answer that wins a bid.
//!
//! So this reads four:
//!
//! | In the file | What it is | Worth anything? |
//! |---|---|---|
//! | V1, R2 | RC4 at 40 bits | No. Broken by anyone who cares. |
//! | V2, R3 | RC4 at 128 bits | No. RC4 itself is broken. |
//! | V4, AESV2 | AES-128 | Sound cipher, weak key derivation (MD5). |
//! | V5, R6, AESV3 | AES-256 | Yes. This is what we write. |
//!
//! [`Lock::weak`] says which of those are the first three, and the window puts
//! it on screen when a file is opened. Somebody working on a set locked with
//! RC4 should know that the lock is decorative, and telling them is more use
//! than refusing to open it.
//!
//! What is *not* here: guessing. There is no password list, no cracking, no
//! stripping a lock off a file that was not opened with its password. A file
//! opens because somebody typed the password, or it does not open.

use crate::object::{Dict, Object};

/// The 32 bytes every password in the old handlers is padded out to. Straight
/// from the format; it has no purpose beyond making every password the same
/// length.
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Which lock a file uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    /// RC4 at 40 bits. 1993, and it was not strong then.
    Rc4Short,
    /// RC4 at 128 bits. A longer key on a broken cipher.
    Rc4Long,
    /// AES-128, with the key worked out through MD5.
    Aes128,
    /// AES-256 at revision 6. What this program writes.
    Aes256,
}

impl Lock {
    /// True when the lock no longer protects anything, whatever it says on the
    /// tin. Said out loud rather than hidden, because somebody deciding
    /// whether a set is safe to send deserves to know.
    pub fn weak(&self) -> bool {
        !matches!(self, Lock::Aes256)
    }

    /// One line for the Security row, in the properties panel.
    pub fn in_words(&self) -> &'static str {
        match self {
            Lock::Rc4Short => "RC4 at 40 bits — obsolete, and no longer any protection at all",
            Lock::Rc4Long => "RC4 at 128 bits — obsolete; RC4 itself is broken",
            Lock::Aes128 => "AES-128 — a sound cipher, but the password is turned into a key the old way",
            Lock::Aes256 => "AES-256",
        }
    }
}

/// How one sort of thing in the file — its streams, or its strings — is
/// scrambled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum How {
    /// Not scrambled at all. Legal, and some writers do it for one or the other.
    Plain,
    Rc4,
    Aes,
}

/// A file that has been opened, and what it takes to read the rest of it.
pub struct Opened {
    key: Vec<u8>,
    /// Revision 5 and 6 use the file key directly for every object; everything
    /// earlier mixes the object's own number into it.
    per_object: bool,
    streams: How,
    strings: How,
    pub lock: Lock,
    /// What the file asks a reader to allow. A request, not a lock — see
    /// [`crate::crypt`].
    pub allowed: crate::crypt::Allowed,
}

impl Opened {
    /// The key for one object, which for the old handlers depends on which
    /// object it is.
    fn key_for(&self, number: u32, generation: u16, aes: bool) -> Vec<u8> {
        if !self.per_object {
            return self.key.clone();
        }
        let mut input = self.key.clone();
        input.extend_from_slice(&number.to_le_bytes()[..3]);
        input.extend_from_slice(&generation.to_le_bytes()[..2]);
        if aes {
            // The format's own four bytes, added for AES and for nothing else.
            input.extend_from_slice(b"sAlT");
        }
        let digest = md5(&input);
        let want = (self.key.len() + 5).min(16);
        digest[..want].to_vec()
    }

    fn undo(&self, how: How, number: u32, generation: u16, data: &[u8]) -> Option<Vec<u8>> {
        match how {
            How::Plain => Some(data.to_vec()),
            How::Rc4 => Some(rc4(&self.key_for(number, generation, false), data)),
            How::Aes => crate::aes::decrypt_cbc(&self.key_for(number, generation, true), data),
        }
    }

    /// One stream's bytes, as they were before the file was locked. Still
    /// encoded — [`crate::filters`] does that part.
    pub fn stream(&self, number: u32, generation: u16, data: &[u8]) -> Option<Vec<u8>> {
        self.undo(self.streams, number, generation, data)
    }

    /// One string.
    pub fn string(&self, number: u32, generation: u16, data: &[u8]) -> Option<Vec<u8>> {
        self.undo(self.strings, number, generation, data)
    }

    /// Walks an object as it came off the page and puts every string and every
    /// stream back the way it was written.
    ///
    /// A string that will not decrypt is emptied rather than left as
    /// ciphertext: a title block showing forty bytes of rubbish looks like a
    /// corrupt drawing, and an empty one looks like what it is.
    pub fn object(&self, number: u32, generation: u16, object: &mut Object) {
        match object {
            Object::String(bytes, _) => {
                *bytes = self.string(number, generation, bytes).unwrap_or_default();
            }
            Object::Array(items) => {
                for item in items {
                    self.object(number, generation, item);
                }
            }
            Object::Dict(dict) => self.dict(number, generation, dict),
            Object::Stream(stream) => {
                self.dict(number, generation, &mut stream.dict);
                // A cross-reference stream is never encrypted -- it has to be
                // readable before anybody knows the file is locked at all --
                // and neither is a signature's own contents.
                if !matches!(stream.dict.get("Type").and_then(|o| o.as_name()), Some(n) if n.is("XRef"))
                {
                    if let Some(plain) = self.stream(number, generation, &stream.data) {
                        stream.data = plain;
                    }
                }
            }
            _ => {}
        }
    }

    fn dict(&self, number: u32, generation: u16, dict: &mut Dict) {
        for (_, value) in dict.0.iter_mut() {
            self.object(number, generation, value);
        }
    }
}

/// Opens a locked file, or does not.
///
/// `id` is the first half of the trailer's `/ID`, which the old handlers mix
/// into the key. A file with no `/ID` is legal and rare; an empty slice is the
/// right thing to pass for one.
///
/// Both passwords are tried, the user's first. The owner's opens the file just
/// as widely — the difference between them is what a *reader* is asked to
/// allow, and that was never enforcement.
pub fn open(encrypt: &Dict, id: &[u8], password: &str) -> Option<Opened> {
    let filter = encrypt.get("Filter").and_then(|o| o.as_name());
    if !matches!(filter, Some(n) if n.is("Standard")) {
        // A publisher's own handler, which needs their software and their
        // licence server. Nothing this program can do with it.
        return None;
    }
    let v = encrypt.get("V").and_then(|o| o.as_i64()).unwrap_or(0);
    let r = encrypt.get("R").and_then(|o| o.as_i64()).unwrap_or(0);
    let p = encrypt.get("P").and_then(|o| o.as_i64()).unwrap_or(0) as i32;
    let allowed = crate::crypt::Allowed::from_bits(p);

    if r >= 5 {
        // Revision 5 was Adobe's draft of what became 6. The key unwrapping is
        // the same; only the hardening loop differs, and `crypt::unlock`
        // handles both.
        let key = crate::crypt::unlock(encrypt, password)?;
        let (streams, strings) = filters(encrypt, How::Aes);
        return Some(Opened {
            key: key.to_vec(),
            per_object: false,
            streams,
            strings,
            lock: Lock::Aes256,
            allowed,
        });
    }

    let o = encrypt.get("O")?.as_bytes()?;
    let u = encrypt.get("U")?.as_bytes()?;
    if o.len() < 32 || u.len() < 32 {
        return None;
    }
    let bits = encrypt.get("Length").and_then(|x| x.as_i64()).unwrap_or(40);
    let n = if r == 2 { 5 } else { ((bits / 8).clamp(5, 16)) as usize };
    let metadata_too = encrypt
        .get("EncryptMetadata")
        .and_then(|x| x.as_bool())
        .unwrap_or(true);

    let typed = password.as_bytes();
    let key = if let Some(key) = as_user(typed, o, p, id, r, n, metadata_too, u) {
        key
    } else {
        // Not the user's password, so try it as the owner's: that one is used
        // to recover the user's password, which is then checked the usual way.
        let recovered = user_password_from_owner(typed, o, r, n)?;
        as_user(&recovered, o, p, id, r, n, metadata_too, u)?
    };

    let default = if v >= 4 { How::Plain } else { How::Rc4 };
    let (streams, strings) = if v >= 4 {
        filters(encrypt, How::Aes)
    } else {
        (default, default)
    };
    let lock = match (v, streams, strings) {
        (_, How::Aes, _) | (_, _, How::Aes) => Lock::Aes128,
        _ if n <= 5 => Lock::Rc4Short,
        _ => Lock::Rc4Long,
    };
    Some(Opened {
        key,
        per_object: true,
        streams,
        strings,
        lock,
        allowed,
    })
}

/// What `/StmF` and `/StrF` name in `/CF`, for the handlers that have them.
fn filters(encrypt: &Dict, when_v5: How) -> (How, How) {
    let cf = match encrypt.get("CF").and_then(|o| o.as_dict()) {
        Some(cf) => cf,
        // Revision 5 and 6 without a crypt filter dictionary: AES throughout,
        // which is the only thing they can be.
        None => return (when_v5, when_v5),
    };
    let named = |key: &str| -> How {
        let Some(name) = encrypt.get(key).and_then(|o| o.as_name()) else {
            return How::Plain;
        };
        if name.is("Identity") {
            return How::Plain;
        }
        let Some(entry) = cf.get(name.as_str()).and_then(|o| o.as_dict()) else {
            return How::Plain;
        };
        match entry.get("CFM").and_then(|o| o.as_name()) {
            Some(m) if m.is("AESV2") || m.is("AESV3") => How::Aes,
            Some(m) if m.is("V2") => How::Rc4,
            _ => How::Plain,
        }
    };
    (named("StmF"), named("StrF"))
}

/// Algorithm 2, then the check against `/U`. `Some` only when it was right.
#[allow(clippy::too_many_arguments)]
fn as_user(
    password: &[u8],
    o: &[u8],
    p: i32,
    id: &[u8],
    r: i64,
    n: usize,
    metadata_too: bool,
    u: &[u8],
) -> Option<Vec<u8>> {
    let mut input = padded(password).to_vec();
    input.extend_from_slice(&o[..32]);
    input.extend_from_slice(&p.to_le_bytes());
    input.extend_from_slice(id);
    if r >= 4 && !metadata_too {
        input.extend_from_slice(&[0xFF; 4]);
    }
    let mut digest = md5(&input);
    if r >= 3 {
        // Fifty more times over the first n bytes. Deliberate slowness, which
        // was a reasonable idea and is no longer nearly enough of it.
        for _ in 0..50 {
            digest = md5(&digest[..n]);
        }
    }
    let key = digest[..n].to_vec();
    checks_out(&key, id, r, u).then_some(key)
}

/// Algorithms 4 and 5: does this key produce the file's `/U`?
fn checks_out(key: &[u8], id: &[u8], r: i64, u: &[u8]) -> bool {
    if r == 2 {
        return rc4(key, &PAD) == u[..32];
    }
    let mut input = PAD.to_vec();
    input.extend_from_slice(id);
    let mut block = md5(&input).to_vec();
    block = rc4(key, &block);
    for round in 1u8..=19 {
        let turned: Vec<u8> = key.iter().map(|b| b ^ round).collect();
        block = rc4(&turned, &block);
    }
    // Only the first sixteen bytes mean anything; the rest of /U is padding
    // that writers fill differently.
    block[..16] == u[..16]
}

/// Algorithm 7: the owner's password gives back the user's.
fn user_password_from_owner(password: &[u8], o: &[u8], r: i64, n: usize) -> Option<Vec<u8>> {
    let mut digest = md5(&padded(password));
    if r >= 3 {
        for _ in 0..50 {
            digest = md5(&digest);
        }
    }
    let key = &digest[..n];
    if r == 2 {
        return Some(rc4(key, &o[..32]));
    }
    let mut block = o[..32].to_vec();
    for round in (0u8..=19).rev() {
        let turned: Vec<u8> = key.iter().map(|b| b ^ round).collect();
        block = rc4(&turned, &block);
    }
    Some(block)
}

/// A password as the old handlers want it: exactly 32 bytes, topped up from
/// [`PAD`] or cut short.
fn padded(password: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let take = password.len().min(32);
    out[..take].copy_from_slice(&password[..take]);
    out[take..].copy_from_slice(&PAD[..32 - take]);
    out
}

fn md5(data: &[u8]) -> [u8; 16] {
    use md5::Digest;
    let mut hasher = md5::Md5::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// RC4. Here because the format still uses it, not because it should be used.
fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = [0; 256];
    for (at, byte) in s.iter_mut().enumerate() {
        *byte = at as u8;
    }
    let mut j = 0u8;
    for i in 0..256 {
        j = j
            .wrapping_add(s[i])
            .wrapping_add(key[i % key.len().max(1)]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            let at = s[i as usize].wrapping_add(s[j as usize]);
            byte ^ s[at as usize]
        })
        .collect()
}
