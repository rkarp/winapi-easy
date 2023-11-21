/*!
A small collection of various abstractions over the Windows API.
*/

#![cfg(windows)]
#![allow(clippy::uninlined_format_args)]

pub mod audio;
pub mod clipboard;
pub mod com;
pub mod fs;
pub mod keyboard;
pub mod process;
pub mod shell;
pub mod ui;

mod internal;
mod string;
