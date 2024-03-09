/*!
Component Object Model (COM) initialization.
*/

#![allow(dead_code)]

use std::cell::Cell;
use std::io;

use windows::core::{
    ComInterface,
    GUID,
};
use windows::Win32::System::Com::{
    CoCreateInstance,
    CoInitializeEx,
    CoTaskMemFree,
    CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};

/// Initializes the COM library for the current thread. Will do nothing on further calls from the same thread.
pub fn initialize_com() -> io::Result<()> {
    thread_local! {
        static COM_INITIALIZED: Cell<bool> = const { Cell::new(false) };
    }
    COM_INITIALIZED.with(|initialized| {
        if !initialized.get() {
            let init_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            match init_result {
                Ok(()) => {
                    initialized.set(true);
                    Ok(())
                }
                Err(err) => Err(err.into()),
            }
        } else {
            Ok(())
        }
    })
}

pub(crate) trait ComInterfaceExt: ComInterface {
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
        unsafe { CoTaskMemFree(Some(self.0 as *mut _)) }
    }
}
