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
use windows::Win32::System::Console::{
    AllocConsole,
    FreeConsole,
};
use windows::Win32::System::Shutdown::LockWorkStation;

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
