#![recursion_limit = "256"]
//! The self-hosted Hyperview server.
//!
//! A company runs one of these. It holds the drawings, the markups people have
//! put on them, the shared tool chests, and the version of the program the
//! office is meant to be running. Everything it does is reachable over an open,
//! documented API, so an estimating system, an ERP, a spreadsheet or anything
//! else can ask it the same questions the viewer does and get the same answers.
//!
//! What it is deliberately not: the place the drawing lives. The PDF is the
//! file of record. This server keeps a copy of those exact bytes and a stream
//! of the annotation dictionaries that belong on them, and applying the second
//! to the first reproduces the drawing somebody would hand a general
//! contractor. Nothing is translated into a private format on the way in.
//!
//! It is built to be run by somebody who is not a systems administrator. One
//! binary, a SQLite file, a folder of drawings. No message queue, no container
//! orchestration, and nothing that stops working when the machine reboots.

pub mod api;
pub mod audit;
pub mod auth;
pub mod config;
pub mod fleet;
pub mod license;
pub mod mcp;
pub mod openapi;
pub mod patience;
pub mod presence;
pub mod projects;
pub mod quantities;
pub mod run;
pub mod store;
pub mod tunnel;
pub mod updates;
pub mod winsvc;

pub use config::Config;
pub use store::Store;

/// The server's own build, which a client is told so a person can see at a
/// glance whether the two match.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The public keys a release must be signed by before this server will hold it
/// for anybody.
///
/// The same list a seat pins, and for the same reason: a company's server is
/// not a place where a build gets to become trusted. It checks the signature
/// when it takes a release, and every seat checks it again before it runs one.
/// An empty list refuses everything, which is how a build ships until a key is
/// compiled into it.
pub fn trusted() -> hub::update::Trusted {
    hub::update::Trusted::pinned(KEYS)
}

/// The same file the program compiles, read in rather than copied, so the
/// server and the seats can never disagree about whose signature counts. A
/// second list that somebody had to remember to update at release time is how
/// a server ends up refusing the build every seat would have taken.
mod pinned {
    #![allow(dead_code)]
    include!("../../hyperview/src/trust.rs");
}

pub use pinned::{HOME_CHANNEL, HOME_FEED, KEYS, LICENSE_FEED};
