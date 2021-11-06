/*!
A small collection of various abstractions over the Windows API.
*/

#![cfg(windows)]

pub mod clipboard;
pub mod com;
pub mod keyboard;
pub mod process;
pub mod shell;
pub mod ui;

mod internal;
mod string;
