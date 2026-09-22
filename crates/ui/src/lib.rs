//! The program's chrome: commands, icons, panels and the markups grid.

pub mod chrome;
pub mod command;
pub mod icon;
pub mod mark;
pub mod menu;
pub mod tess;

pub use chrome::{Chrome, Theme};
pub use command::{Command, Kind};
