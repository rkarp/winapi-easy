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
    shared::{
        minwindef::HGLOBAL,
        ntdef::HANDLE,
        windef::HWND,
    },
    um::{
        handleapi::{
            CloseHandle,
            INVALID_HANDLE_VALUE,
        },
        winbase::{
            GlobalLock,
            GlobalUnlock,
        },
    },
};

pub(crate) trait PtrLike: Sized + Copy {
    type Target;
}

impl<T> PtrLike for *mut T {
    type Target = T;
}

pub(crate) trait ReturnValue: Sized + Copy {
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

impl ReturnValue for u32 {
    #[inline]
    fn is_null(self) -> bool {
        self == 0
    }
}

impl ReturnValue for i32 {
    #[inline]
    fn is_null(self) -> bool {
        self == 0
    }
}

impl ReturnValue for isize {
    #[inline]
    fn is_null(self) -> bool {
        self == 0
    }
}

impl ReturnValue for HANDLE {
    #[inline]
    fn is_null(self) -> bool {
        self.is_null()
    }
}

pub(crate) trait RawHandle: PtrLike {
    fn to_non_null(self) -> Option<NonNull<Self::Target>> {
        let ptr: *mut Self::Target = unsafe {
            // Safe only as long as `Self: PtrLike`
            *(&self as *const Self as *const *mut Self::Target)
        };
        NonNull::new(ptr)
    }

    fn to_non_null_else_error(
        self,
        error_gen: impl FnOnce() -> io::Error,
    ) -> io::Result<NonNull<Self::Target>> {
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

    #[inline(always)]
    fn is_invalid(self) -> bool {
        false
    }
}

impl RawHandle for HANDLE {
    /// Checks if the handle value is invalid.
    ///
    /// **Caution**: This is not correct for all APIs, for example GetCurrentProcess will also return
    /// `-1` as a special handle representing the current process.
    #[inline]
    fn is_invalid(self) -> bool {
        self == INVALID_HANDLE_VALUE
    }
}

impl RawHandle for HWND {}

pub(crate) trait ManagedHandle {
    type Target;
    fn as_immutable_ptr(&self) -> *mut Self::Target;
    #[inline(always)]
    fn as_mutable_ptr(&mut self) -> *mut Self::Target {
        self.as_immutable_ptr()
    }
}

impl<T> ManagedHandle for NonNull<T> {
    type Target = T;

    #[inline(always)]
    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.as_ptr()
    }
}

impl<T: ManagedHandle + CloseableHandle> ManagedHandle for AutoClose<T> {
    type Target = T::Target;

    #[inline(always)]
    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.entity.as_immutable_ptr()
    }
}

pub(crate) trait CloseableHandle {
    fn close(&mut self);
}

impl CloseableHandle for NonNull<c_void> {
    fn close(&mut self) {
        unsafe {
            CloseHandle(self.as_ptr());
        }
    }
}

pub(crate) struct AutoClose<T: CloseableHandle> {
    entity: T,
}

impl<T: CloseableHandle> From<T> for AutoClose<T> {
    fn from(entity: T) -> Self {
        AutoClose { entity }
    }
}

impl<T: CloseableHandle> Drop for AutoClose<T> {
    fn drop(&mut self) {
        self.entity.close()
    }
}

pub(crate) struct GlobalLockedData {
    handle: HGLOBAL,
    ptr: NonNull<c_void>,
}

impl GlobalLockedData {
    pub(crate) fn lock(handle: HGLOBAL) -> io::Result<Self> {
        unsafe {
            GlobalLock(handle)
                .to_non_null_else_get_last_error()
                .map(|ptr| GlobalLockedData { handle, ptr })
        }
    }
    #[inline(always)]
    pub(crate) fn ptr(&mut self) -> *mut c_void {
        self.ptr.as_ptr()
    }
}

impl Drop for GlobalLockedData {
    fn drop(&mut self) {
        unsafe {
            GlobalUnlock(self.handle);
        }
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
