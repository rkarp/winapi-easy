#![allow(dead_code)]

use std::cell::Cell;
use std::error::Error;
use std::ffi::c_void;
use std::fmt::Display;
use std::marker::PhantomData;
use std::panic::{
    AssertUnwindSafe,
    catch_unwind,
};
use std::ptr::NonNull;
use std::{
    io,
    ptr,
};

use windows::Win32::Foundation::{
    CloseHandle,
    HANDLE,
    HGLOBAL,
    HMODULE,
    HWND,
    INVALID_HANDLE_VALUE,
    LRESULT,
};
use windows::Win32::System::Memory::{
    GlobalLock,
    GlobalUnlock,
};
use windows::Win32::UI::WindowsAndMessaging::HMENU;
use windows::core::BOOL;

pub(crate) trait ReturnValue: PartialEq + Sized + Copy {
    const NULL_VALUE: Self;

    fn if_null_to_error_else_drop(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<()> {
        self.if_null_to_error(error_gen).map(|_| ())
    }

    fn if_null_to_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<Self> {
        if self.is_null() {
            Err(error_gen())
        } else {
            Ok(self)
        }
    }

    fn if_null_get_last_error_else_drop(self) -> io::Result<()> {
        self.if_null_to_error_else_drop(io::Error::last_os_error)
    }

    fn if_null_get_last_error(self) -> io::Result<Self> {
        self.if_null_to_error(io::Error::last_os_error)
    }

    fn if_null_panic_else_drop(self, msg: &'static str) {
        self.if_null_panic(msg);
    }

    fn if_null_panic(self, msg: &'static str) -> Self {
        if self.is_null() {
            panic!("{}", msg)
        } else {
            self
        }
    }

    fn if_non_null_to_error(self, error_gen: impl FnOnce() -> io::Error) -> io::Result<()> {
        if self.is_null() {
            Ok(())
        } else {
            Err(error_gen())
        }
    }

    fn if_eq_to_error<T>(
        self,
        should_not_equal: T,
        error_gen: impl FnOnce() -> io::Error,
    ) -> io::Result<()>
    where
        T: PartialEq<Self>,
    {
        if should_not_equal == self {
            Err(error_gen())
        } else {
            Ok(())
        }
    }

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

impl ReturnValue for usize {
    const NULL_VALUE: Self = 0;
}

impl ReturnValue for isize {
    const NULL_VALUE: Self = 0;
}

impl<T> ReturnValue for *mut T {
    const NULL_VALUE: Self = ptr::null_mut();

    fn is_null(self) -> bool {
        self.is_null()
    }
}

impl ReturnValue for BOOL {
    const NULL_VALUE: Self = BOOL(0);
}

impl ReturnValue for HANDLE {
    const NULL_VALUE: Self = HANDLE(ptr::null_mut());
}

impl ReturnValue for HWND {
    const NULL_VALUE: Self = HWND(ptr::null_mut());
}

impl ReturnValue for HMENU {
    const NULL_VALUE: Self = HMENU(ptr::null_mut());
}

impl ReturnValue for HMODULE {
    const NULL_VALUE: Self = HMODULE(ptr::null_mut());
}

impl ReturnValue for LRESULT {
    const NULL_VALUE: Self = LRESULT(0);
}

pub(crate) trait PtrLike: Sized + Copy {
    type Target;

    fn to_non_null(self) -> Option<NonNull<Self::Target>> {
        let ptr: *mut Self::Target = unsafe { *(&raw const self).cast::<*mut Self::Target>() };
        NonNull::new(ptr)
    }
}

impl<T> PtrLike for *mut T {
    type Target = T;
}

pub(crate) trait RawHandle: PtrLike {
    fn to_non_null_else_error(
        self,
        error_gen: impl FnOnce() -> io::Error,
    ) -> io::Result<NonNull<Self::Target>> {
        match self.to_non_null() {
            Some(result) => Ok(result),
            None => Err(error_gen()),
        }
    }

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

    fn is_invalid(self) -> bool {
        false
    }
}

impl RawHandle for *mut c_void {
    /// Checks if the handle value is invalid.
    ///
    /// **Caution**: This is not correct for all APIs, for example `GetCurrentProcess` will also return
    /// `-1` as a special handle representing the current process.
    fn is_invalid(self) -> bool {
        HANDLE(self) == INVALID_HANDLE_VALUE
    }
}

pub(crate) trait ManagedHandle {
    type Target;
    fn as_immutable_ptr(&self) -> *mut Self::Target;
    fn as_mutable_ptr(&mut self) -> *mut Self::Target {
        self.as_immutable_ptr()
    }
}

impl<T> ManagedHandle for NonNull<T> {
    type Target = T;

    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.as_ptr()
    }
}

impl<T: ManagedHandle + CloseableHandle> ManagedHandle for AutoClose<T> {
    type Target = T::Target;

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
        self.entity.close();
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
    pub(crate) fn ptr(&mut self) -> *mut c_void {
        self.ptr.as_ptr()
    }
    #[expect(dead_code)]
    pub(crate) fn handle(&self) -> HGLOBAL {
        self.handle
    }
}

impl Drop for GlobalLockedData {
    fn drop(&mut self) {
        unsafe {
            GlobalUnlock(self.handle).unwrap_or_default_and_print_error();
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
        (self.drop_fn)(&mut self.value);
    }
}

pub(crate) fn sync_closure_to_callback2<F, IN1, IN2, OUT>(
    closure: &mut F,
) -> unsafe extern "system" fn(IN1, IN2) -> OUT
where
    F: FnMut(IN1, IN2) -> OUT,
{
    thread_local! {
        static RAW_CLOSURE: Cell<*mut ()> = const { Cell::new(ptr::null_mut()) };
    }

    unsafe extern "system" fn trampoline<F, IN1, IN2, OUT>(input1: IN1, input2: IN2) -> OUT
    where
        F: FnMut(IN1, IN2) -> OUT,
    {
        let call = move || {
            let unwrapped_closure: *mut () = RAW_CLOSURE.with(Cell::get);
            let closure: &mut F = unsafe { &mut *(unwrapped_closure.cast::<F>()) };
            closure(input1, input2)
        };
        catch_unwind_and_abort(call)
    }
    RAW_CLOSURE.with(|cell| cell.set(ptr::from_mut::<F>(closure).cast::<()>()));
    trampoline::<F, IN1, IN2, OUT>
}

/// Converts a 2 parameter closure to a Windows callback function and feeds it to the acceptor.
///
/// # Panics
///
/// Nested calls to this function are not allowed and will panic.
///
/// # Safety
///
/// This function ensures that the unsafe callback does not outlive the closure. Still, the acceptor must not
/// use the unsafe callback in a way that would cause Windows to call it after this function has returned.
pub(crate) fn with_sync_closure_to_callback2<F, A, O, IN1, IN2, OUT>(
    mut closure: F,
    acceptor: A,
) -> O
where
    F: FnMut(IN1, IN2) -> OUT,
    A: FnOnce(unsafe extern "system" fn(IN1, IN2) -> OUT) -> O,
{
    thread_local! {
        static RUNNING: Cell<bool> = const { Cell::new(false) };
    }

    if RUNNING.get() {
        panic!("Nested calls to this function are not allowed")
    } else {
        RUNNING.set(true);
    }
    let result = acceptor(sync_closure_to_callback2::<F, IN1, IN2, OUT>(&mut closure));
    RUNNING.set(false);
    result
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

/// A box-like struct that does not invalidate raw pointers to its data when it is moved.
#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct RawBox<T: ?Sized>(*mut T);

impl<T> RawBox<T> {
    pub(crate) fn new(value: T) -> Self {
        Self::from_box(Box::new(value))
    }
}

impl<T: ?Sized> RawBox<T> {
    pub(crate) fn from_box(value: Box<T>) -> Self {
        Self(Box::into_raw(value))
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut T {
        self.0
    }
}

impl<T: ?Sized> Drop for RawBox<T> {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.0) };
    }
}

impl<T: ?Sized> From<Box<T>> for RawBox<T> {
    fn from(value: Box<T>) -> Self {
        Self::from_box(value)
    }
}

/// A box-like struct that hides its concrete type and does not invalidate raw pointers to its data when it is moved.
#[derive(Debug)]
pub(crate) struct OpaqueRawBox<'inner> {
    box_ptr: *mut (),
    destructor: unsafe fn(*mut ()),
    phantom: PhantomData<&'inner ()>,
}

impl<'inner> OpaqueRawBox<'inner> {
    pub(crate) fn new<T: 'inner>(value: T) -> Self {
        unsafe fn destructor<T>(box_ptr: *mut ()) {
            let _ = unsafe { Box::from_raw(box_ptr.cast::<T>()) };
        }
        Self {
            box_ptr: Box::into_raw(Box::new(value)).cast::<()>(),
            destructor: destructor::<T>,
            phantom: PhantomData,
        }
    }

    pub(crate) fn as_mut_ptr<T>(&mut self) -> *mut T {
        self.box_ptr.cast()
    }
}

impl Drop for OpaqueRawBox<'_> {
    fn drop(&mut self) {
        unsafe { (self.destructor)(self.box_ptr) };
    }
}

/// A struct that hides the concrete type of a closure but still allows a lifetime.
///
/// Does not invalidate raw pointers to its closure when moved.
#[derive(Debug)]
pub(crate) struct OpaqueClosure<'inner, I, O> {
    raw_boxed_closure: OpaqueRawBox<'inner>,
    trampoline: unsafe fn(*mut (), I) -> O,
}

impl<'inner, I, O> OpaqueClosure<'inner, I, O> {
    pub(crate) fn new<F>(closure: F) -> Self
    where
        F: FnMut(I) -> O + 'inner,
    {
        unsafe fn trampoline<F, I, O>(raw_closure: *mut (), input: I) -> O
        where
            F: FnMut(I) -> O,
        {
            let closure: &mut F = unsafe { &mut *raw_closure.cast::<F>() };
            closure(input)
        }
        Self {
            raw_boxed_closure: OpaqueRawBox::new(closure),
            trampoline: trampoline::<F, I, O>,
        }
    }

    /// Returns a closure that delegates to the original closure.
    #[expect(clippy::wrong_self_convention)]
    pub(crate) fn to_closure(&mut self) -> impl FnMut(I) -> O {
        |input| unsafe { (self.trampoline)(self.raw_boxed_closure.box_ptr, input) }
    }
}

pub(crate) fn custom_err_with_code<C>(err_text: &str, result_code: C) -> io::Error
where
    C: Display,
{
    io::Error::other(format!("{}. Code: {}", err_text, result_code))
}

pub(crate) trait ResultExt {
    type Output: Default;
    fn unwrap_or_default_and_print_error(self) -> Self::Output;
}

impl<T: Default, E: Error> ResultExt for Result<T, E> {
    type Output = T;

    fn unwrap_or_default_and_print_error(self) -> Self::Output {
        if let Err(err) = &self {
            use std::io::Write;
            let mut stderr = std::io::stderr();
            let _ = writeln!(stderr, "Error: {}", err);
            let _ = writeln!(stderr, "{}", std::backtrace::Backtrace::capture());
        }
        self.unwrap_or_default()
    }
}

/// Transforms a collection of values to ranges covering all given values.
pub(crate) fn values_to_ranges(values: impl Into<Vec<u32>>) -> Vec<(u32, u32)> {
    let mut values: Vec<_> = values.into();
    values.sort_unstable();
    values.dedup();
    values
        .chunk_by(|x1, x2| x1.wrapping_add(1) == *x2)
        .map(|consecutives| {
            (
                *consecutives.first().unwrap(),
                *consecutives.last().unwrap(),
            )
        })
        .collect()
}

pub(crate) mod windows_missing {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::Shell::{
        NIN_SELECT,
        NINF_KEY,
    };

    pub const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;

    #[expect(non_snake_case)]
    pub fn LOWORD(l: u32) -> u16 {
        (l << u16::BITS >> u16::BITS).try_into().unwrap()
    }

    #[expect(non_snake_case)]
    pub fn HIWORD(l: u32) -> u16 {
        (l >> u16::BITS).try_into().unwrap()
    }

    #[expect(non_snake_case)]
    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_possible_wrap)]
    #[expect(clippy::cast_sign_loss)]
    pub fn GET_X_LPARAM(lp: LPARAM) -> i32 {
        (LOWORD(lp.0 as u32) as i16).into()
    }

    #[expect(non_snake_case)]
    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_possible_wrap)]
    #[expect(clippy::cast_sign_loss)]
    pub fn GET_Y_LPARAM(lp: LPARAM) -> i32 {
        (HIWORD(lp.0 as u32) as i16).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_sync_closure() {
        const TEST_VALUE: usize = 42;
        let callback = |x: usize, _: ()| x;
        let acceptor = |raw_fn: unsafe extern "system" fn(usize, ()) -> usize| -> usize {
            unsafe { raw_fn(TEST_VALUE, ()) }
        };
        assert_eq!(
            with_sync_closure_to_callback2(callback, acceptor),
            TEST_VALUE
        );
    }

    #[test]
    fn run_opaque_closure() {
        let test_string = &"foo".to_string();
        let mut x = 0;
        let mut closure = |_| {
            x += 1;
            test_string.to_string()
        };
        let mut op_closure = OpaqueClosure::new(&mut closure);
        let mut re_closure = op_closure.to_closure();
        assert_eq!(&re_closure(()), test_string);
    }
}
