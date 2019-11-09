use std::{
    fmt::Display,
    io::{
        self,
        ErrorKind,
    },
    ops::{Deref, DerefMut},
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
use winapi::shared::windef::HWND;
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
    #[inline]
    fn is_null(self) -> bool {
        self == 0
    }
}

impl WinErrCheckable for i32 {
    #[inline]
    fn is_null(self) -> bool {
        self == 0
    }
}

impl WinErrCheckable for isize {
    #[inline]
    fn is_null(self) -> bool {
        self == 0
    }
}

impl WinErrCheckable for HANDLE {
    #[inline]
    fn is_null(self) -> bool {
        self.is_null()
    }
}

pub(crate) trait WinErrCheckableHandle: PtrLike {
    fn to_non_null<'ptr>(self) -> Option<&'ptr mut Self::Target> {
        let ptr: *mut Self::Target = unsafe {
            // Safe only as long as `Self: PtrLike`
            *(&self as *const Self as *const *mut Self::Target)
        };
        unsafe {
            ptr.as_mut()
        }
    }

    fn to_non_null_else_error<'ptr>(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<&'ptr mut Self::Target> {
        match self.to_non_null() {
            Some(result) => Ok(result),
            None => Err(error_gen()),
        }
    }

    #[inline]
    fn to_non_null_else_get_last_error<'ptr>(self) -> io::Result<&'ptr mut Self::Target> {
        self.to_non_null_else_error(|| io::Error::last_os_error())
    }

    fn if_invalid_get_last_error(self) -> io::Result<Self> {
        if self.is_invalid() {
            Err(io::Error::last_os_error())
        } else {
            Ok(self)
        }
    }

    #[inline(always)]
    fn is_invalid(self) -> bool {
        false
    }
}

impl WinErrCheckableHandle for HANDLE {
    /// Checks if the handle value is invalid.
    ///
    /// **Caution**: This is not correct for all APIs, for example GetCurrentProcess will also return
    /// `-1` as a special handle representing the current process.
    #[inline]
    fn is_invalid(self) -> bool {
        self == INVALID_HANDLE_VALUE
    }
}

impl WinErrCheckableHandle for HWND {}

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

impl CloseableHandle for c_void {
    fn close(&mut self) {
        unsafe {
            CloseHandle(self);
        }
    }
}

pub(crate) struct AutoClose<'ptr, T: CloseableHandle> {
    entity: &'ptr mut T,
}

impl<'ptr, T: CloseableHandle> From<&'ptr mut T> for AutoClose<'ptr, T> {
    fn from(entity: &'ptr mut T) -> Self {
        AutoClose {
            entity,
        }
    }
}

impl<'ptr, T: CloseableHandle> Drop for AutoClose<'ptr, T> {
    fn drop(&mut self) {
        self.entity.close()
    }
}

impl<'ptr, T: CloseableHandle> Deref for AutoClose<'ptr, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.entity
    }
}

impl<'ptr, T: CloseableHandle> DerefMut for AutoClose<'ptr, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entity
    }
}

impl<'ptr, T: CloseableHandle> AsRef<T> for AutoClose<'ptr, T> {
    fn as_ref(&self) -> &T {
        &self.entity
    }
}

impl<'ptr, T: CloseableHandle> AsMut<T> for AutoClose<'ptr, T> {
    fn as_mut(&mut self) -> &mut T {
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
