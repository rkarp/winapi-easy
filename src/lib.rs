use std::{
    io::{
        self,
        ErrorKind
    }
};

use winapi::{
    ctypes::c_void,
    shared::{
        minwindef::{
            HGLOBAL,
        },
        winerror::HRESULT
    },
    um::{
        winbase::{
            GlobalUnlock,
            GlobalLock
        }
    },
};

pub mod clipboard;
pub mod keyboard;
pub mod process;
pub mod ui;

pub trait WinErrCheckable: Sized + Copy {
    fn if_null_get_last_error(self) -> io::Result<Self> {
        if self.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(self)
        }
    }
    fn if_null_to_error(self, error_gen: impl Fn() -> io::Error) -> io::Result<Self> {
        if self.is_null() {
            Err(error_gen())
        } else {
            Ok(self)
        }
    }
    fn if_null_panic(self) -> Self {
        if self.is_null() {
            panic!()
        } else {
            self
        }
    }
    fn is_null(self) -> bool;
}

impl WinErrCheckable for *mut c_void {
    fn is_null(self) -> bool {
        self.is_null()
    }
}

impl WinErrCheckable for u32 {
    fn is_null(self) -> bool {
        self == 0
    }
}

impl WinErrCheckable for i32 {
    fn is_null(self) -> bool {
        self == 0
    }
}

fn custom_hresult_err<T>(err_text: &str, hresult: HRESULT) -> io::Result<T> {
    Err(io::Error::new(
        ErrorKind::Other,
        format!("{}. Code: {}", err_text, hresult),
    ))
}

pub(crate) struct GlobalLockedData<'ptr> {
    handle: HGLOBAL,
    ptr: &'ptr mut c_void,
}

impl GlobalLockedData<'_> {
    pub(crate) fn lock(handle: *mut c_void) -> io::Result<Self> {
        unsafe {
            GlobalLock(handle)
                .if_null_get_last_error()
                .map(|ptr| GlobalLockedData {
                    handle,
                    ptr: ptr.as_mut().expect("Unexpected null from GlobalLock"),
                })
        }
    }
    #[inline(always)]
    pub(crate) fn ptr(&mut self) -> *mut c_void {
        self.ptr
    }
}

impl Drop for GlobalLockedData<'_> {
    fn drop(&mut self) {
        unsafe {
            GlobalUnlock(self.handle);
        }
    }
}
