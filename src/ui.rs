//! UI functionality.

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
};
pub use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
    DPI_AWARENESS_CONTEXT_UNAWARE,
    DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED,
};
use windows::Win32::UI::Magnification::MagShowSystemCursor;
use windows::Win32::UI::WindowsAndMessaging::ClipCursor;

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
    #[allow(dead_code)]
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

/// A (non-null) handle to a region.
#[derive(Eq, PartialEq, Debug)]
pub struct Region {
    raw_handle: HRGN,
}

impl Region {
    pub fn from_rect(rect: Rectangle) -> Self {
        let raw_handle = unsafe { CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom) };
        Self::from_non_null(raw_handle)
    }

    pub(crate) fn from_non_null(handle: HRGN) -> Self {
        Self { raw_handle: handle }
    }

    pub fn duplicate(&self) -> io::Result<Self> {
        self.combine(None, RGN_COPY)
    }

    pub fn and_not_in(&self, other: &Region) -> io::Result<Self> {
        self.combine(Some(other), RGN_DIFF)
    }

    fn combine(&self, other: Option<&Region>, mode: RGN_COMBINE_MODE) -> io::Result<Self> {
        let dest = Self::from_rect(Default::default());
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
}

impl From<Region> for HRGN {
    fn from(value: Region) -> Self {
        value.raw_handle
    }
}

impl From<&Region> for HRGN {
    fn from(value: &Region) -> Self {
        value.raw_handle
    }
}

#[must_use]
pub struct CursorConfinement(());

impl CursorConfinement {
    /// Globally confines the cursor to a rectangular area on the screen.
    ///
    /// The confinement will be automatically released when [`CursorConfinement`] is dropped.
    pub fn new(bounding_area: Rectangle) -> io::Result<Self> {
        unsafe {
            ClipCursor(Some(&bounding_area))?;
        }
        Ok(Self(()))
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

#[must_use]
pub struct CursorConcealment(());

impl CursorConcealment {
    /// Globally hides the system cursor.
    ///
    /// The cursor will be automatically visible again when [`CursorConcealment`] is dropped.
    pub fn new() -> io::Result<Self> {
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

impl Drop for CursorConcealment {
    fn drop(&mut self) {
        Self::remove().expect("Removing cursor hidden state failed");
    }
}

pub fn set_dpi_awareness_context(context: DPI_AWARENESS_CONTEXT) -> io::Result<()> {
    unsafe {
        SetProcessDpiAwarenessContext(context)?;
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
