#![allow(dead_code)]

use std::cell::Cell;
use std::ffi::c_void;
use std::fmt::Display;
use std::io;
use std::io::ErrorKind;
use std::panic::{
    catch_unwind,
    AssertUnwindSafe,
};
use std::ptr;
use std::ptr::NonNull;

use windows::Win32::Foundation::{
    CloseHandle,
    BOOL,
    HANDLE,
    HGLOBAL,
    HMODULE,
    HWND,
    INVALID_HANDLE_VALUE,
    LPARAM,
    LRESULT,
};
use windows::Win32::System::Memory::{
    GlobalLock,
    GlobalUnlock,
};
use windows::Win32::UI::WindowsAndMessaging::HMENU;

pub(crate) trait ReturnValue: PartialEq + Sized + Copy {
    const NULL_VALUE: Self;

    fn if_null_to_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<Self> {
        if !self.is_null() {
            Ok(self)
        } else {
            Err(error_gen())
        }
    }

    #[inline]
    fn if_null_get_last_error(self) -> io::Result<Self> {
        self.if_null_to_error(io::Error::last_os_error)
    }

    #[inline]
    fn if_null_panic(self, msg: &'static str) -> Self {
        if !self.is_null() {
            self
        } else {
            panic!("{}", msg)
        }
    }

    #[inline]
    fn if_non_null_to_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<()> {
        if self.is_null() {
            Ok(())
        } else {
            Err(error_gen())
        }
    }

    #[inline]
    fn if_eq_to_error<T>(
        self,
        should_not_equal: T,
        error_gen: impl FnOnce() -> io::Error,
    ) -> io::Result<()>
    where
        T: PartialEq<Self>,
    {
        if should_not_equal != self {
            Ok(())
        } else {
            Err(error_gen())
        }
    }

    #[inline]
    fn if_not_eq_to_error<T>(
        self,
        must_equal: T,
        error_gen: impl FnOnce() -> io::Error,
    ) -> io::Result<()>
    where
        T: PartialEq<Self>,
    {
        if must_equal == self {
            Ok(())
        } else {
            Err(error_gen())
        }
    }

    fn is_null(self) -> bool {
        self == Self::NULL_VALUE
    }
}

impl ReturnValue for u16 {
    const NULL_VALUE: Self = 0;
}

impl ReturnValue for i16 {
    const NULL_VALUE: Self = 0;
}

impl ReturnValue for u32 {
    const NULL_VALUE: Self = 0;
}

impl ReturnValue for i32 {
    const NULL_VALUE: Self = 0;
}

impl ReturnValue for isize {
    const NULL_VALUE: Self = 0;
}

impl<T> ReturnValue for *mut T {
    const NULL_VALUE: Self = ptr::null_mut();

    #[inline]
    fn is_null(self) -> bool {
        self.is_null()
    }
}

impl ReturnValue for BOOL {
    const NULL_VALUE: Self = BOOL(0);
}

impl ReturnValue for HANDLE {
    const NULL_VALUE: Self = HANDLE(0);
}

impl ReturnValue for HWND {
    const NULL_VALUE: Self = HWND(0);
}

impl ReturnValue for HMENU {
    const NULL_VALUE: Self = HMENU(0);
}

impl ReturnValue for HMODULE {
    const NULL_VALUE: Self = HMODULE(0);
}

impl ReturnValue for LRESULT {
    const NULL_VALUE: Self = LRESULT(0);
}

pub(crate) trait PtrLike: Sized + Copy {
    type Target;
}

impl<T> PtrLike for *mut T {
    type Target = T;
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
        self.to_non_null_else_error(io::Error::last_os_error)
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

impl RawHandle for *mut c_void {
    /// Checks if the handle value is invalid.
    ///
    /// **Caution**: This is not correct for all APIs, for example GetCurrentProcess will also return
    /// `-1` as a special handle representing the current process.
    #[inline]
    fn is_invalid(self) -> bool {
        HANDLE(self as isize) == INVALID_HANDLE_VALUE
    }
}

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
    fn close(&self);
}

impl CloseableHandle for HANDLE {
    fn close(&self) {
        unsafe {
            CloseHandle(*self).unwrap();
        }
    }
}

pub(crate) struct AutoClose<T: CloseableHandle> {
    pub(crate) entity: T,
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
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn handle(&self) -> HGLOBAL {
        self.handle
    }
}

impl Drop for GlobalLockedData {
    fn drop(&mut self) {
        unsafe {
            GlobalUnlock(self.handle).unwrap();
        }
    }
}

#[derive(Debug)]
pub(crate) struct CustomAutoDrop<T> {
    pub value: T,
    // Intentionally only a fn to make capturing variables compile errors
    pub drop_fn: fn(&mut T),
}

impl<T> Drop for CustomAutoDrop<T> {
    fn drop(&mut self) {
        (self.drop_fn)(&mut self.value)
    }
}

#[allow(dead_code)]
pub(crate) fn unpack_closure<F, IN, OUT>(
    closure: &mut F,
) -> (LPARAM, unsafe extern "system" fn(IN, LPARAM) -> OUT)
where
    F: FnMut(IN) -> OUT,
{
    unsafe extern "system" fn trampoline<F, IN, OUT>(input: IN, raw_closure: LPARAM) -> OUT
    where
        F: FnMut(IN) -> OUT,
    {
        let raw_closure = raw_closure.0 as *mut F;
        let closure: &mut F = unsafe { &mut *raw_closure };
        let call = || closure(input);
        catch_unwind_and_abort(call)
    }

    (LPARAM(closure as *mut F as isize), trampoline::<F, IN, OUT>)
}

#[allow(dead_code)]
pub(crate) fn sync_closure_to_callback2<F, IN1, IN2, OUT>(
    closure: &mut F,
) -> unsafe extern "system" fn(IN1, IN2) -> OUT
where
    F: FnMut(IN1, IN2) -> OUT,
{
    thread_local! {
        static RAW_CLOSURE: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    }

    unsafe extern "system" fn trampoline<F, IN1, IN2, OUT>(input1: IN1, input2: IN2) -> OUT
    where
        F: FnMut(IN1, IN2) -> OUT,
    {
        let call = move || {
            let unwrapped_closure: *mut c_void = RAW_CLOSURE.with(|raw_closure| raw_closure.get());
            let closure: &mut F = &mut *(unwrapped_closure as *mut F);
            closure(input1, input2)
        };
        catch_unwind_and_abort(call)
    }
    RAW_CLOSURE.with(|cell| cell.set(closure as *mut F as *mut c_void));
    trampoline::<F, IN1, IN2, OUT>
}

/// Converts a 2 parameter closure to a Windows callback function and feeds it to the acceptor.
///
/// # Safety
///
/// This function ensures that the unsafe callback does not outlive the closure. Still, the acceptor must not
/// use the unsafe callback in a way that would cause Windows to call it after this function has returned.
pub(crate) fn with_sync_closure_to_callback2<F, A, O, IN1, IN2, OUT>(
    closure: &mut F,
    acceptor: A,
) -> O
where
    F: FnMut(IN1, IN2) -> OUT,
    A: FnOnce(unsafe extern "system" fn(IN1, IN2) -> OUT) -> O,
{
    thread_local! {
        static RAW_CLOSURE: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    }

    unsafe extern "system" fn trampoline<F, IN1, IN2, OUT>(input1: IN1, input2: IN2) -> OUT
    where
        F: FnMut(IN1, IN2) -> OUT,
    {
        let call = move || {
            let unwrapped_closure: *mut c_void = RAW_CLOSURE.with(|raw_closure| raw_closure.get());
            let closure: &mut F = &mut *(unwrapped_closure as *mut F);
            closure(input1, input2)
        };
        catch_unwind_and_abort(call)
    }
    RAW_CLOSURE.with(|cell| cell.set(closure as *mut F as *mut c_void));
    acceptor(trampoline::<F, IN1, IN2, OUT>)
}

pub(crate) fn catch_unwind_and_abort<F: FnOnce() -> R, R>(f: F) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(_) => {
            // Don't print anything when catching unwinds because the default panic hook already does.
            //
            // Abort is safe because it doesn't unwind.
            std::process::abort();
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

#[allow(dead_code)]
pub(crate) mod windows_missing {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::Shell::{
        NINF_KEY,
        NIN_SELECT,
    };

    pub const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;

    #[allow(non_snake_case)]
    #[inline]
    pub fn LOWORD(l: u32) -> u16 {
        (l << u16::BITS >> u16::BITS).try_into().unwrap()
    }

    #[allow(non_snake_case)]
    #[inline]
    pub fn HIWORD(l: u32) -> u16 {
        (l >> u16::BITS).try_into().unwrap()
    }

    #[allow(non_snake_case)]
    #[inline]
    pub fn GET_X_LPARAM(lp: LPARAM) -> i32 {
        LOWORD(lp.0 as u32) as i16 as i32
    }

    #[allow(non_snake_case)]
    #[inline]
    pub fn GET_Y_LPARAM(lp: LPARAM) -> i32 {
        HIWORD(lp.0 as u32) as i16 as i32
    }
}
