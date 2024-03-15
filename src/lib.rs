/*!
A small collection of various abstractions over the Windows API.
*/

#![cfg_attr(all(doc, CHANNEL_NIGHTLY), feature(doc_auto_cfg))]
#![allow(clippy::uninlined_format_args)]

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
use windows::core::CanInto;
