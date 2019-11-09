use std::{
    fmt::Display,
    io::{
        self,
        ErrorKind,
    },
    ptr::NonNull,
};

use winapi::{
    ctypes::c_void,
    shared::minwindef::HGLOBAL,
    um::{
        winbase::{
            GlobalUnlock,
            GlobalLock,
        },
    },
};
use winapi::_core::ops::{Deref, DerefMut};
use winapi::shared::ntdef::HANDLE;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};

pub(crate) trait PtrLike: Sized + Copy {
    type Target;
}

impl<T> PtrLike for *mut T {
    type Target = T;
}

pub(crate) trait WinErrCheckable: Sized + Copy {
    fn if_null_to_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<Self> {
        if self.is_null() {
            Err(error_gen())
        } else {
            Ok(self)
        }
    }

    #[inline]
    fn if_null_get_last_error(self) -> io::Result<Self> {
        self.if_null_to_error(|| io::Error::last_os_error())
    }

    fn if_null_panic(self, msg: &'static str) -> Self {
        if self.is_null() {
            panic!(msg)
        } else {
            self
        }
    }

    fn if_non_null_to_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<()> {
        if !self.is_null() {
            Err(error_gen())
        } else {
            Ok(())
        }
    }

    fn is_null(self) -> bool;
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

impl WinErrCheckable for isize {
    fn is_null(self) -> bool {
        self == 0
    }
}

impl WinErrCheckable for HANDLE {
    fn is_null(self) -> bool {
        self.is_null()
    }
}

pub(crate) trait WinErrCheckableHandle: WinErrCheckable + PtrLike {
    fn to_non_null(self) -> Option<NonNull<Self::Target>> {
        let ptr: *mut Self::Target = unsafe {
            // Safe only as long as `Self: PtrLike`
            *(&self as *const Self as *const *mut Self::Target)
        };
        NonNull::new(ptr)
    }

    fn to_non_null_else_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<NonNull<Self::Target>> {
        match self.to_non_null() {
            Some(result) => Ok(result),
            None => Err(error_gen()),
        }
    }

    #[inline]
    fn to_non_null_else_get_last_error(self) -> io::Result<NonNull<Self::Target>> {
        self.to_non_null_else_error(|| io::Error::last_os_error())
    }

    fn if_invalid_get_last_error(self) -> io::Result<Self> {
        if self.is_invalid() {
            Err(io::Error::last_os_error())
        } else {
            Ok(self)
        }
    }

    fn is_invalid(self) -> bool;
}

impl WinErrCheckableHandle for HANDLE {
    /// Checks if the handle value is invalid.
    ///
    /// **Caution**: This is not correct for all APIs, for example GetCurrentProcess will also return
    /// `-1` as a special handle representing the current process.
    fn is_invalid(self) -> bool {
        self == INVALID_HANDLE_VALUE
    }
}

pub(crate) fn custom_err_with_code<C>(err_text: &str, result_code: C) -> io::Error
    where
        C: Display,
{
    io::Error::new(
        ErrorKind::Other,
        format!("{}. Code: {}", err_text, result_code),
    )
}

pub(crate) trait CloseableHandle {
    fn close(&mut self);
}

impl CloseableHandle for NonNull<c_void> {
    fn close(&mut self) {
        unsafe {
            CloseHandle(self.as_mut());
        }
    }
}

pub(crate) struct AutoClose<T: CloseableHandle> {
    entity: T,
}

impl<T: CloseableHandle> From<T> for AutoClose<T> {
    fn from(entity: T) -> Self {
        AutoClose {
            entity,
        }
    }
}

impl<T: CloseableHandle> Drop for AutoClose<T> {
    fn drop(&mut self) {
        self.entity.close()
    }
}

impl<T: CloseableHandle> Deref for AutoClose<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.entity
    }
}

impl<T: CloseableHandle> DerefMut for AutoClose<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entity
    }
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
