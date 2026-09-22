//! Naming a markup so it stays the same markup everywhere.
//!
//! PDF gives every annotation an optional `/NM`, a name meant to be unique
//! within the document. Hyperview leans on it harder than that: it is what lets
//! the same markup be recognised after a save, after a reopen, and — the part
//! that matters — after it has been round a server and back from somebody
//! else's screen.
//!
//! Which means it has to be unique across **machines**, not merely within one
//! file. Six people marking up the same drawing set on the same morning must
//! not produce two markups with the same name, because the sync would treat
//! them as one and quietly lose somebody's work.
//!
//! No dependency for this: a name is drawn from the clock at nanosecond
//! resolution, a counter that rises for the life of the process, and the
//! address of a fresh allocation, which on any modern system is randomised.
//! That is not cryptography and does not need to be — it needs to not collide.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh name for a markup.
///
/// The `xhv` prefix says where it came from, which is worth having when
/// somebody is looking at a markup in another program and wondering.
pub fn fresh() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    // A fresh allocation's address carries whatever entropy the system's
    // layout randomisation provides, which is what separates two processes
    // started in the same nanosecond on the same machine.
    let here = Box::new(0u8);
    let where_ = Box::into_raw(here) as u64;
    // Safety: taken straight back and dropped.
    unsafe {
        drop(Box::from_raw(where_ as *mut u8));
    }

    let mixed = mix(nanos ^ mix(count.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ where_));
    format!("xhv-{nanos:016x}-{mixed:016x}")
}

/// A round of mixing, so two names a nanosecond apart do not merely differ in
/// their last digit.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    x = x.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    x ^ (x >> 33)
}

/// True when a name looks like one of ours, which is how a markup that came
/// from somewhere else is told apart from one that went round the houses and
/// came home.
pub fn is_ours(name: &str) -> bool {
    name.starts_with("xhv-") && name.len() >= 20
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn two_names_are_never_the_same() {
        let mut seen = HashSet::new();
        for _ in 0..50_000 {
            assert!(seen.insert(fresh()), "a name repeated");
        }
    }

    #[test]
    fn names_made_at_the_same_moment_on_different_threads_do_not_collide() {
        // Six seats is the real case; six threads is a harder one.
        let handles: Vec<_> = (0..6)
            .map(|_| std::thread::spawn(|| (0..5_000).map(|_| fresh()).collect::<Vec<_>>()))
            .collect();
        let mut seen = HashSet::new();
        for handle in handles {
            for name in handle.join().expect("a thread") {
                assert!(seen.insert(name), "two seats made the same name");
            }
        }
    }

    #[test]
    fn a_name_says_where_it_came_from() {
        let name = fresh();
        assert!(is_ours(&name), "{name}");
        assert!(!is_ours("BB-12345"), "Revu's own names are not ours");
        assert!(!is_ours(""));
        assert!(!is_ours("xhv-"), "a prefix alone is not a name");
    }

    #[test]
    fn a_name_is_only_the_characters_a_pdf_can_carry_plainly() {
        let name = fresh();
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "{name}"
        );
    }
}
