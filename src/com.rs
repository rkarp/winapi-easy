/*!
Component Object Model (COM) initialization.
*/

#![allow(dead_code)]

use std::cell::Cell;
use std::ffi::c_void;
use std::io;

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
    CoCreateInstance,
    CoInitializeEx,
    CoTaskMemFree,
};
use windows::core::{
    GUID,
    Interface,
};

/// Initializes the COM library for the current thread. Will do nothing on further calls from the same thread.
pub fn initialize_com() -> windows::core::Result<()> {
    thread_local! {
        static COM_INITIALIZED: Cell<bool> = const { Cell::new(false) };
    }
    COM_INITIALIZED.with(|initialized| {
        if initialized.get() {
            Ok(())
        } else {
            let init_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() };
            if let Ok(()) = init_result {
                initialized.set(true);
            }
            init_result
        }
    })
}

pub(crate) trait ComInterfaceExt: Interface {
    const CLASS_GUID: GUID;

    fn new_instance() -> io::Result<Self> {
        initialize_com()?;
        let result = unsafe { CoCreateInstance(&Self::CLASS_GUID, None, CLSCTX_INPROC_SERVER) };
        result.map_err(Into::into)
    }
}

/// COM task memory location to be automatically freed.
#[derive(Debug)]
pub(crate) struct ComTaskMemory<T>(pub *mut T);

impl<T> From<*mut T> for ComTaskMemory<T> {
    fn from(value: *mut T) -> Self {
        ComTaskMemory(value)
    }
}

impl<T> Drop for ComTaskMemory<T> {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast_const().cast::<c_void>())) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_initialize_com() -> io::Result<()> {
        initialize_com()?;
        Ok(())
    }
}
