// The signing keys this build will accept an update from.
//
// This file is included verbatim by the release tool as well as compiled into
// the program, so the command that says a release is good and the seat that
// decides whether to install it are reading the same list. That is why it holds
// nothing but the list.

/// The signing keys this build will accept an update from, as
/// `(name, public key as 64 hex characters)`.
///
/// Compiled in on purpose. A Hyperview server hands out installers, and nothing
/// about running a server should be enough to decide what software six machines
/// run — a release signed by a key that is not in this list does not install,
/// whoever is serving it.
///
/// Adding one is deliberate: generate it with `hyperview-release keygen`, on the
/// publisher's own machine, and paste the public half here. The private half
/// never appears in this repository.
///
/// An empty list means this build refuses every update and says so, which is the
/// right way round: a program that installs anything when it has been told to
/// trust nothing is worse than one that installs nothing.
pub const KEYS: &[(&str, &str)] = &[
    // Made on Creede's own PC on 2026-09-21. The private half is there and
    // nowhere else.
    ("mesafab-2026", "14d93acf4729859a3a7b01f458e118173a7a1859305193e84c3bd7513eb72790"),
];

/// Where new versions are published: a releases page on GitHub.
///
/// A company's server reads this and passes new versions on to its seats. A
/// seat with no server reads it itself. Two cases, worth telling apart:
///
/// A seat signed in to a company's own server takes its answer from **that
/// server**, always. A shop that has held its six seats on 1.2 while it
/// finishes a bid has made a decision, and a program that went round the back
/// of it to a feed of ours would be overruling the person who owns the
/// machines.
///
/// A seat with no server — a copy on a laptop, one somebody was handed at a
/// demo, the first machine in a shop before anybody has set anything up — has
/// nobody to ask, so it asks here itself.
///
/// A company's server asks here on its seats' behalf, and only its
/// administrator can hold it back.
///
/// What travels over it is an installer and the signed manifest that describes
/// it. Nothing about a company goes the other way: not a drawing, not a markup,
/// not a tool chest, not a name. And what arrives is checked against [`KEYS`]
/// before it is run, so this address is a place to fetch from rather than
/// something the program trusts — a feed that was taken over could serve only
/// things that were already signed by a key its publisher holds.
///
/// Empty means a loose seat simply never looks, which is what a build does
/// until somebody fills this in.
pub const HOME_FEED: &str = "https://api.github.com/repos/cngmoney88/hyperview-releases/releases";

/// The channel a copy follows when nobody has said otherwise.
pub const HOME_CHANNEL: &str = "stable";
