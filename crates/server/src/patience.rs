//! How many wrong guesses a server will sit through.
//!
//! There was no limit at all. Not on signing in, not on the join code, not
//! anywhere. On a shop network that is defensible: to guess at anything you
//! have to already be in the building. The moment a server is reachable from
//! a jobsite it is not, and the join code is why — one server-wide secret
//! that names nobody, never expires, is not used up, and gets whoever types
//! it an account.
//!
//! So this counts wrong answers and makes them cost time. Nothing here locks
//! anybody out: the wait grows and then stops growing, and one right answer
//! clears it. An estimator who has forgotten which password they used waits
//! a few seconds. Somebody working through a dictionary from a hotel wi-fi
//! gets a quarter of an hour per guess, which is the end of that.
//!
//! **In memory, on purpose.** A failed guess is not worth a disk write, and
//! writing one would hand an attacker a way to fill the disk. Restarting the
//! server clears the counts, and that is fine: restarting a server is not
//! something anybody outside the building can make happen.
//!
//! **Bounded, on purpose.** Somebody with a thousand addresses must not be
//! able to grow this without limit, so it holds a fixed number of entries and
//! forgets the least recently touched. That does blunt per-address counting
//! for an attacker who rotates addresses — which is exactly why the count is
//! kept per account as well. Rotating addresses does not change whose
//! password you are guessing.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Entries kept before the least recently touched is forgotten. Roughly a
/// megabyte of short strings, and far more than any real shop has people or
/// addresses.
const MOST_KEPT: usize = 4096;

/// What a kind of guess costs.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    /// Wrong answers allowed before any wait at all. Somebody typing their
    /// own password badly should never meet this.
    pub free: u32,
    /// The wait after the first one past `free`. It doubles from there.
    pub first: Duration,
    /// The longest it ever gets. Past this the answer is simply no, at a
    /// rate nobody can work with.
    pub longest: Duration,
}

/// Signing in. Five tries free, because people mistype passwords, then a
/// wait that doubles to a quarter of an hour.
pub const SIGNING_IN: Rule = Rule {
    free: 5,
    first: Duration::from_secs(2),
    longest: Duration::from_secs(15 * 60),
};

/// Typing a join code. Harsher, and deliberately: a join code has no
/// username in front of it, so a guess is a guess at the whole secret.
/// Nobody legitimately types one twenty times.
pub const JOINING: Rule = Rule {
    free: 3,
    first: Duration::from_secs(5),
    longest: Duration::from_secs(60 * 60),
};

#[derive(Clone, Copy)]
struct Wrong {
    times: u32,
    last: Instant,
}

/// The counts, and the lock around them.
#[derive(Default)]
pub struct Patience {
    wrong: Mutex<HashMap<String, Wrong>>,
}

impl std::fmt::Debug for Patience {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.wrong.lock().map(|w| w.len()).unwrap_or(0);
        write!(f, "Patience({held} watched)")
    }
}

impl Patience {
    /// How much longer this one has to wait, if it does.
    ///
    /// Ask before doing the work, so a refusal costs the server nothing. An
    /// empty `who` is not counted at all: that is the case where nothing is
    /// known about where a request came from, and counting every one of those
    /// together would let one bad actor lock out a whole shop behind the same
    /// proxy.
    pub fn wait_for(&self, rule: Rule, who: &str) -> Option<Duration> {
        if who.is_empty() {
            return None;
        }
        let held = self.wrong.lock().ok()?;
        let seen = held.get(who)?;
        // `free` wrong answers cost nothing. The attempt after that waits.
        // So the test is on what has already been got wrong, not on what is
        // being tried now: at `free` wrongs the allowance is spent.
        let spent = seen.times.checked_sub(rule.free)?;
        let owed = penalty(rule, spent);
        let waited = seen.last.elapsed();
        (waited < owed).then(|| owed - waited)
    }

    /// The first of these that has to wait, in the order given.
    pub fn wait_for_any(&self, rule: Rule, who: &[&str]) -> Option<Duration> {
        who.iter().filter_map(|one| self.wait_for(rule, one)).max()
    }

    /// That was wrong. Count it.
    pub fn wrong(&self, who: &str) {
        if who.is_empty() {
            return;
        }
        let Ok(mut held) = self.wrong.lock() else { return };
        if held.len() >= MOST_KEPT && !held.contains_key(who) {
            forget_the_stalest(&mut held);
        }
        let seen = held.entry(who.to_string()).or_insert(Wrong { times: 0, last: Instant::now() });
        seen.times = seen.times.saturating_add(1);
        seen.last = Instant::now();
    }

    /// That was right. Forget everything about them.
    pub fn right(&self, who: &str) {
        if who.is_empty() {
            return;
        }
        if let Ok(mut held) = self.wrong.lock() {
            held.remove(who);
        }
    }

    /// How many are being watched. For the tests and for a look at the box.
    pub fn watching(&self) -> usize {
        self.wrong.lock().map(|w| w.len()).unwrap_or(0)
    }
}

/// The wait owed once the free ones are spent: the first wait, then double
/// it for each further wrong answer, then flat forever.
fn penalty(rule: Rule, spent: u32) -> Duration {
    let doublings = spent.min(20);
    rule.first
        .saturating_mul(1u32 << doublings)
        .min(rule.longest)
}

/// Drops a tenth of the entries, oldest first, so this is not done on every
/// call once the map is full.
fn forget_the_stalest(held: &mut HashMap<String, Wrong>) {
    let mut ages: Vec<(Instant, String)> =
        held.iter().map(|(k, v)| (v.last, k.clone())).collect();
    ages.sort_by_key(|(when, _)| *when);
    for (_, key) in ages.into_iter().take(MOST_KEPT / 10) {
        held.remove(&key);
    }
}

/// What to tell somebody who has to wait. Never says how many tries are left
/// or what the limit is; that is a hint about the thing being guarded.
pub fn refusal(owed: Duration) -> String {
    let seconds = owed.as_secs().max(1);
    let how_long = if seconds < 90 {
        format!("{seconds} seconds")
    } else {
        format!("{} minutes", seconds.div_ceil(60))
    };
    format!(
        "Too many wrong tries. Wait {how_long} and try again. \
         If you have forgotten it, an administrator can set it for you."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rule with no waiting in it, so the tests do not sleep.
    const QUICK: Rule = Rule {
        free: 2,
        first: Duration::from_secs(10),
        longest: Duration::from_secs(40),
    };

    #[test]
    fn the_free_ones_really_are_free_and_the_next_one_is_not() {
        let p = Patience::default();
        for attempt in 1..=QUICK.free {
            assert!(
                p.wait_for(QUICK, "somebody").is_none(),
                "attempt {attempt} of {} is free", QUICK.free
            );
            p.wrong("somebody");
        }
        assert!(
            p.wait_for(QUICK, "somebody").is_some(),
            "the allowance is spent, so the next one waits"
        );
    }

    #[test]
    fn the_wait_doubles_and_then_stops() {
        assert_eq!(penalty(QUICK, 0), Duration::from_secs(10), "the first wait");
        assert_eq!(penalty(QUICK, 1), Duration::from_secs(20));
        assert_eq!(penalty(QUICK, 2), Duration::from_secs(40));
        assert_eq!(penalty(QUICK, 3), Duration::from_secs(40), "capped");
        assert_eq!(penalty(QUICK, 99), Duration::from_secs(40), "still capped");
        // And the real rules cannot overflow their way to nothing.
        assert_eq!(penalty(SIGNING_IN, 1000), SIGNING_IN.longest);
        assert_eq!(penalty(JOINING, 1000), JOINING.longest);
    }

    #[test]
    fn getting_it_right_clears_it() {
        let p = Patience::default();
        for _ in 0..10 {
            p.wrong("somebody");
        }
        assert!(p.wait_for(QUICK, "somebody").is_some());
        p.right("somebody");
        assert!(p.wait_for(QUICK, "somebody").is_none(), "one right answer is the end of it");
        assert_eq!(p.watching(), 0);
    }

    #[test]
    fn one_person_guessing_does_not_hold_up_anybody_else() {
        let p = Patience::default();
        for _ in 0..20 {
            p.wrong("the-guesser");
        }
        assert!(p.wait_for(QUICK, "the-guesser").is_some());
        assert!(p.wait_for(QUICK, "somebody-else").is_none());
    }

    /// The case that matters when a server is on the internet: nothing is
    /// known about where the request came from, so nothing is counted. The
    /// alternative -- counting them all together -- would let one bad actor
    /// lock out every seat behind the same proxy.
    #[test]
    fn a_caller_we_know_nothing_about_is_not_counted_with_the_others() {
        let p = Patience::default();
        for _ in 0..50 {
            p.wrong("");
        }
        assert!(p.wait_for(QUICK, "").is_none());
        assert_eq!(p.watching(), 0, "nothing was kept");
    }

    #[test]
    fn it_cannot_be_grown_without_limit() {
        let p = Patience::default();
        for n in 0..(MOST_KEPT + 500) {
            p.wrong(&format!("address-{n}"));
        }
        assert!(p.watching() <= MOST_KEPT, "held {} entries", p.watching());
    }

    #[test]
    fn the_worst_of_the_two_is_the_one_that_counts() {
        let p = Patience::default();
        for _ in 0..3 {
            p.wrong("their-account");
        }
        let by_account = p.wait_for(QUICK, "their-account").unwrap();
        let either = p.wait_for_any(QUICK, &["their-address", "their-account"]).unwrap();
        // Not equality: what is owed counts down between the two calls, and
        // the property worth holding is which name decided the answer, not
        // that a shrinking number stopped shrinking.
        let apart = by_account.abs_diff(either);
        assert!(apart < Duration::from_secs(1), "{by_account:?} vs {either:?}");
        // Three wrong, two of them free, so one past: the first wait, ten
        // seconds. The address has nothing against it and contributed
        // nothing, which is the point.
        assert!(either > Duration::from_secs(19), "the account decided it: {either:?}");
        assert!(p.wait_for(QUICK, "their-address").is_none());
        assert!(p.wait_for_any(QUICK, &["nobody", "no-one"]).is_none());
    }

    #[test]
    fn the_refusal_says_how_long_and_nothing_else() {
        let said = refusal(Duration::from_secs(45));
        assert!(said.contains("45 seconds"), "{said}");
        assert!(said.contains("administrator"), "{said}");
        assert!(!said.contains("tries left"), "never a hint about what is left");
        assert!(refusal(Duration::from_secs(600)).contains("10 minutes"));
        // Never zero, whatever it is handed.
        assert!(refusal(Duration::from_millis(1)).contains("1 second"));
    }
}
