//! Bytes nobody can guess.
//!
//! Wanted in two places and for the same reason: a key that locks a drawing
//! set, and the initialisation vector on every piece of it. Both have to be
//! unpredictable to somebody holding the file, or the lock is decorative.
//!
//! Straight from the operating system where there is one to ask. The fallback
//! underneath is deliberately the last resort rather than the first: it mixes
//! things that differ between runs, which is better than a constant and worse
//! than the real thing, and it only ever runs on a machine whose own generator
//! would not answer.

/// `N` bytes from the operating system.
pub fn bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut source) = std::fs::File::open("/dev/urandom") {
            if source.read_exact(&mut out).is_ok() {
                return out;
            }
        }
    }
    #[cfg(windows)]
    {
        // Windows fills a buffer through the system's own generator.
        //
        // The `link` attribute is not optional and its absence is not
        // obvious: this crate pulls in nothing else from advapi32, so
        // without it the symbol is unresolved and the link fails -- not
        // here, but in whatever example or binary happens to depend on this
        // crate, with an error naming a file nobody has heard of.
        #[link(name = "advapi32")]
        extern "system" {
            fn SystemFunction036(buffer: *mut u8, length: u32) -> u8;
        }
        let ok = unsafe { SystemFunction036(out.as_mut_ptr(), N as u32) };
        if ok != 0 {
            return out;
        }
    }
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    sha2::Digest::update(&mut hasher, now.to_le_bytes());
    sha2::Digest::update(&mut hasher, std::process::id().to_le_bytes());
    let here = Box::new(0u8);
    sha2::Digest::update(&mut hasher, (&*here as *const u8 as usize).to_le_bytes());
    sha2::Digest::update(
        &mut hasher,
        std::time::Instant::now().elapsed().as_nanos().to_le_bytes(),
    );
    let mut seed = sha2::Digest::finalize(hasher).to_vec();
    while seed.len() < N {
        let mut again = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut again, &seed);
        seed.extend_from_slice(&sha2::Digest::finalize(again));
    }
    out.copy_from_slice(&seed[..N]);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn two_goes_do_not_come_back_the_same() {
        assert_ne!(super::bytes::<32>(), super::bytes::<32>());
    }

    #[test]
    fn it_is_not_all_zeroes() {
        assert_ne!(super::bytes::<32>(), [0u8; 32]);
    }
}
