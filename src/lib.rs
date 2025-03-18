/*!
A small collection of various abstractions over the Windows API.
*/

#![cfg_attr(all(doc, nightly), feature(doc_auto_cfg))]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_errors_doc)]
// Until compiler version 1.87
#![allow(unstable_name_collisions)]

#[cfg(not(windows))]
compile_error!(
    "This crate only supports Windows. Use `[target.'cfg(windows)'.dependencies]` if necessary."
);

pub use windows;

#[cfg(feature = "clipboard")]
pub mod clipboard;
pub mod com;
#[cfg(feature = "fs")]
pub mod fs;
#[cfg(feature = "hooking")]
pub mod hooking;
#[cfg(feature = "input")]
pub mod input;
#[cfg(feature = "media")]
pub mod media;
pub mod messaging;
#[cfg(feature = "process")]
pub mod process;
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "ui")]
pub mod ui;

mod internal;
mod string;

// Workaround for the `windows::core::imp::interface_hierarchy` macro
#[cfg(feature = "media")]
extern crate self as windows_core;
#[cfg(feature = "media")]
mod imp {
    pub(crate) use windows::core::imp::CanInto;
}
