use std::io;

use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW,
    HMONITOR,
    MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO,
    MonitorFromWindow,
};

use super::Rectangle;
use super::window::WindowHandle;
use crate::internal::ReturnValue;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct MonitorHandle {
    raw_handle: HMONITOR,
}

impl MonitorHandle {
    pub fn from_window(window_handle: WindowHandle) -> Self {
        let raw_handle =
            unsafe { MonitorFromWindow(window_handle.into(), MONITOR_DEFAULTTOPRIMARY) };
        Self { raw_handle }
    }

    pub fn info(self) -> io::Result<MonitorInfo> {
        let mut raw_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            ..Default::default()
        };
        unsafe { GetMonitorInfoW(self.raw_handle, &raw mut raw_info) }
            .if_null_get_last_error_else_drop()?;
        Ok(MonitorInfo {
            monitor_area: raw_info.rcMonitor,
            work_area: raw_info.rcWork,
        })
    }
}

impl From<MonitorHandle> for HMONITOR {
    fn from(value: MonitorHandle) -> Self {
        value.raw_handle
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MonitorInfo {
    pub monitor_area: Rectangle,
    pub work_area: Rectangle,
}
