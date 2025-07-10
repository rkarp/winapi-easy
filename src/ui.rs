//! UI functionality.

use std::sync::Mutex;
use std::{
    io,
    ptr,
};

use window::WindowHandle;
use windows::Win32::Foundation::{
    POINT,
    RECT,
};
use windows::Win32::Graphics::Gdi::{
    CombineRgn,
    CreateRectRgn,
    GDI_REGION_TYPE,
    HRGN,
    RGN_COMBINE_MODE,
    RGN_COPY,
    RGN_DIFF,
    RGN_ERROR,
};
use windows::Win32::System::Console::{
    AllocConsole,
    FreeConsole,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT,
    SetProcessDpiAwarenessContext,
    SetThreadDpiAwarenessContext,
};
pub use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
    DPI_AWARENESS_CONTEXT_UNAWARE,
    DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED,
};
use windows::Win32::UI::Magnification::{
    MagInitialize,
    MagSetFullscreenTransform,
    MagShowSystemCursor,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor,
    GetCursorPos,
    GetSystemMetrics,
    SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
    SetCursorPos,
};
use windows::core::{
    BOOL,
    Free,
};

use crate::internal::ReturnValue;

pub mod desktop;
pub mod menu;
pub mod message_box;
pub mod messaging;
pub mod resource;
pub mod taskbar;
pub mod window;

/// DPI-scaled virtual coordinates.
pub type Point = POINT;
/// DPI-scaled virtual coordinates of a rectangle.
pub type Rectangle = RECT;

trait RectTransform {
    #[expect(dead_code)]
    fn as_point_array(&self) -> &[POINT];
    fn as_point_array_mut(&mut self) -> &mut [POINT];
}
impl RectTransform for RECT {
    fn as_point_array(&self) -> &[POINT] {
        let data = ptr::from_ref(self).cast::<POINT>();
        unsafe { std::slice::from_raw_parts(data, 2) }
    }

    fn as_point_array_mut(&mut self) -> &mut [POINT] {
        let data = ptr::from_mut(self).cast::<POINT>();
        unsafe { std::slice::from_raw_parts_mut(data, 2) }
    }
}

impl ReturnValue for GDI_REGION_TYPE {
    const NULL_VALUE: Self = RGN_ERROR;
}

impl ReturnValue for HRGN {
    const NULL_VALUE: Self = HRGN(ptr::null_mut());
}

/// A (non-null) handle to a region.
#[derive(Eq, PartialEq, Debug)]
pub struct Region {
    raw_handle: HRGN,
    forget_handle: bool,
}

impl Region {
    pub fn from_rect(rect: Rectangle) -> io::Result<Self> {
        let raw_handle = unsafe { CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom) }
            .if_null_to_error(|| io::ErrorKind::Other.into())?;
        Ok(Self::from_non_null(raw_handle))
    }

    pub(crate) fn from_non_null(handle: HRGN) -> Self {
        Self {
            raw_handle: handle,
            forget_handle: false,
        }
    }

    pub fn duplicate(&self) -> io::Result<Self> {
        self.combine(None, RGN_COPY)
    }

    pub fn and_not_in(&self, other: &Region) -> io::Result<Self> {
        self.combine(Some(other), RGN_DIFF)
    }

    fn combine(&self, other: Option<&Region>, mode: RGN_COMBINE_MODE) -> io::Result<Self> {
        let dest = Self::from_rect(Default::default())?;
        unsafe {
            CombineRgn(
                Some(dest.raw_handle),
                Some(self.raw_handle),
                other.map(|x| x.raw_handle),
                mode,
            )
            .if_null_get_last_error_else_drop()?;
        }
        Ok(dest)
    }

    fn into_raw(mut self) -> HRGN {
        self.forget_handle = true;
        self.raw_handle
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        if !self.forget_handle {
            unsafe {
                self.raw_handle.free();
            }
        }
    }
}

impl From<Region> for HRGN {
    fn from(value: Region) -> Self {
        value.into_raw()
    }
}

impl From<&Region> for HRGN {
    fn from(value: &Region) -> Self {
        value.raw_handle
    }
}

#[derive(Debug)]
#[must_use]
pub struct CursorConfinement(Rectangle);

impl CursorConfinement {
    /// Globally confines the cursor to a rectangular area on the screen.
    ///
    /// The confinement will be automatically released when [`CursorConfinement`] is dropped.
    pub fn new(bounding_area: Rectangle) -> io::Result<Self> {
        Self::apply(bounding_area)?;
        Ok(Self(bounding_area))
    }

    /// Reapply the corsor clipping.
    ///
    /// This can be necessary since some operations automatically unclip the cursor.
    pub fn reapply(&self) -> io::Result<()> {
        Self::apply(self.0)
    }

    fn apply(bounding_area: Rectangle) -> io::Result<()> {
        unsafe {
            ClipCursor(Some(&raw const bounding_area))?;
        }
        Ok(())
    }

    pub fn remove() -> io::Result<()> {
        unsafe {
            ClipCursor(None)?;
        }
        Ok(())
    }
}

impl Drop for CursorConfinement {
    fn drop(&mut self) {
        Self::remove().expect("Releasing cursor clipping should never fail");
    }
}

#[derive(Debug)]
#[must_use]
pub struct UnmagnifiedCursorConcealment(());

impl UnmagnifiedCursorConcealment {
    /// Globally hides the unmagnified system cursor.
    ///
    /// The cursor will be automatically visible again when [`UnmagnifiedCursorConcealment`] is dropped.
    pub fn new() -> io::Result<Self> {
        init_magnifier()?;
        unsafe {
            MagShowSystemCursor(false).if_null_get_last_error_else_drop()?;
        }
        Ok(Self(()))
    }

    pub fn remove() -> io::Result<()> {
        unsafe {
            MagShowSystemCursor(true).if_null_get_last_error_else_drop()?;
        }
        Ok(())
    }
}

impl Drop for UnmagnifiedCursorConcealment {
    fn drop(&mut self) {
        Self::remove().expect("Removing cursor hidden state failed");
    }
}

pub fn get_cursor_pos() -> io::Result<Point> {
    let mut point = Point::default();
    unsafe { GetCursorPos(&raw mut point)? }
    Ok(point)
}

pub fn set_cursor_pos(coords: Point) -> io::Result<()> {
    unsafe { SetCursorPos(coords.x, coords.y)? }
    Ok(())
}

pub fn get_virtual_screen_rect() -> Rectangle {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    Rectangle {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn init_magnifier() -> io::Result<()> {
    static MAGNIFIER_INITIALIZED: Mutex<bool> = const { Mutex::new(false) };

    let mut initialized = MAGNIFIER_INITIALIZED.lock().unwrap();
    if *initialized {
        Ok(())
    } else {
        let result = unsafe { MagInitialize().if_null_get_last_error_else_drop() };
        *initialized = true;
        result
    }
}

pub fn set_fullscreen_magnification(mag_factor: f32, offset: Point) -> io::Result<()> {
    init_magnifier()?;
    unsafe {
        MagSetFullscreenTransform(mag_factor, offset.x, offset.y).if_null_get_last_error_else_drop()
    }
}

pub fn remove_fullscreen_magnification() -> io::Result<()> {
    set_fullscreen_magnification(1.0, Point { x: 0, y: 0 })
}

pub fn set_fullscreen_magnification_use_bitmap_smoothing(use_smoothing: bool) -> io::Result<()> {
    #[link(
        name = "magnification.dll",
        kind = "raw-dylib",
        modifiers = "+verbatim"
    )]
    unsafe extern "system" {
        fn MagSetFullscreenUseBitmapSmoothing(use_smoothing: BOOL) -> BOOL;
    }

    init_magnifier()?;
    unsafe {
        MagSetFullscreenUseBitmapSmoothing(use_smoothing.into()).if_null_get_last_error_else_drop()
    }
}

pub fn set_process_dpi_awareness_context(context: DPI_AWARENESS_CONTEXT) -> io::Result<()> {
    unsafe {
        SetProcessDpiAwarenessContext(context)?;
    }
    Ok(())
}

pub fn set_thread_dpi_awareness_context(context: DPI_AWARENESS_CONTEXT) -> io::Result<()> {
    unsafe {
        SetThreadDpiAwarenessContext(context)
            .0
            .if_null_get_last_error_else_drop()?;
    }
    Ok(())
}

/// Creates a console window for the current process if there is none.
pub fn allocate_console() -> io::Result<()> {
    unsafe {
        AllocConsole()?;
    }
    Ok(())
}

/// Detaches the current process from its console.
///
/// If no other processes use the console, it will be destroyed.
pub fn detach_console() -> io::Result<()> {
    unsafe {
        FreeConsole()?;
    }
    Ok(())
}

/// Locks the computer, same as the user action.
pub fn lock_workstation() -> io::Result<()> {
    // Because the function executes asynchronously, a nonzero return value indicates that the operation has been initiated.
    // It does not indicate whether the workstation has been successfully locked.
    unsafe { LockWorkStation()? };
    Ok(())
}
