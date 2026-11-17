//! Dialed core: deklarative Tweaks mit Erkennen, Anwenden und Zurücksetzen.
pub mod bench;
pub mod catalog;
pub mod engine;
pub mod hardware;
pub mod model;
#[cfg(windows)]
mod nv;
pub mod parse;
pub mod sys;

pub use bench::BenchRun;
pub use engine::{Engine, ScanResult};
pub use model::{Tweak, TweakView};
pub use sys::{Sys, WinSys};
