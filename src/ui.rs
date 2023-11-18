/*!
UI components: Windows, taskbar.
*/

use std::convert::TryInto;
use std::io;
use std::io::ErrorKind;
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::ptr::NonNull;

use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use winapi::shared::basetsd::LONG_PTR;
use winapi::shared::guiddef::GUID;
use winapi::shared::minwindef::{
    ATOM,
    BOOL,
    DWORD,
    LPARAM,
    TRUE,
    UINT,
};
use winapi::shared::ntdef::{
    HRESULT,
    WCHAR,
};
use winapi::shared::windef::{
    HICON,
    HWND,
    HWND__,
    POINT,
    RECT,
};
use winapi::shared::winerror::S_OK;
use winapi::um::consoleapi::AllocConsole;
use winapi::um::shellapi::{
    Shell_NotifyIconW,
    NIF_GUID,
    NIF_ICON,
    NIF_INFO,
    NIF_MESSAGE,
    NIF_SHOWTIP,
    NIF_STATE,
    NIF_TIP,
    NIIF_ERROR,
    NIIF_INFO,
    NIIF_NONE,
    NIIF_WARNING,
    NIM_ADD,
    NIM_DELETE,
    NIM_MODIFY,
    NIM_SETVERSION,
    NIS_HIDDEN,
    NOTIFYICONDATAW,
    NOTIFYICON_VERSION_4,
};
use winapi::um::shobjidl_core::{
    ITaskbarList3,
    TBPF_ERROR,
    TBPF_INDETERMINATE,
    TBPF_NOPROGRESS,
    TBPF_NORMAL,
    TBPF_PAUSED,
};
use winapi::um::wincon::GetConsoleWindow;
use winapi::um::winuser::{
    CreateWindowExW,
    DestroyWindow,
    EnumWindows,
    FlashWindowEx,
    GetClassNameW,
    GetDesktopWindow,
    GetForegroundWindow,
    GetWindowLongPtrW,
    GetWindowPlacement,
    GetWindowTextLengthW,
    GetWindowTextW,
    GetWindowThreadProcessId,
    IsWindow,
    IsWindowVisible,
    LockWorkStation,
    RegisterClassExW,
    SendMessageW,
    SetActiveWindow,
    SetForegroundWindow,
    SetWindowLongPtrW,
    SetWindowPlacement,
    ShowWindow,
    UnregisterClassW,
    CW_USEDEFAULT,
    FLASHWINFO,
    FLASHW_ALL,
    FLASHW_CAPTION,
    FLASHW_STOP,
    FLASHW_TIMER,
    FLASHW_TIMERNOFG,
    FLASHW_TRAY,
    GWLP_USERDATA,
    SC_CLOSE,
    SC_MAXIMIZE,
    SC_MINIMIZE,
    SC_MONITORPOWER,
    SC_RESTORE,
    SW_HIDE,
    SW_MAXIMIZE,
    SW_MINIMIZE,
    SW_RESTORE,
    SW_SHOW,
    SW_SHOWMINIMIZED,
    SW_SHOWMINNOACTIVE,
    SW_SHOWNA,
    SW_SHOWNOACTIVATE,
    SW_SHOWNORMAL,
    WINDOWPLACEMENT,
    WM_SYSCOMMAND,
    WNDCLASSEXW,
    WPF_SETMINPOSITION,
    WS_OVERLAPPEDWINDOW,
};
use wio::com::ComPtr;

use crate::com::ComInterface;
use crate::internal::{
    custom_err_with_code,
    sync_closure_to_callback2,
    ManagedHandle,
    RawHandle,
    ReturnValue,
};
use crate::process::{
    ProcessId,
    ThreadId,
};
use crate::string::{
    to_wide_chars_iter,
    FromWideString,
    ToWideString,
};
use crate::ui::message::{
    generic_window_proc,
    WindowMessageListener,
};
use crate::ui::resource::{
    Brush,
    Cursor,
    Icon,
};

pub mod menu;
pub mod message;
pub mod resource;

const MAX_WINDOW_CLASS_NAME_CHARS: usize = 256;

/// A (non-null) handle to a window.
///
/// **Note**: If the window was not created by this thread, then it is not guaranteed that
/// the handle continues pointing to the same window because the underlying handles
/// can get invalid or even recycled.
#[derive(Eq, PartialEq)]
pub struct WindowHandle {
    handle: NonNull<HWND__>,
}

impl WindowHandle {
    /// Returns the console window associated with the current process, if there is one.
    pub fn get_console_window() -> Option<Self> {
        let handle = unsafe { GetConsoleWindow() };
        handle.to_non_null().map(Self::from_non_null)
    }

    pub fn get_foreground_window() -> Option<Self> {
        let handle = unsafe { GetForegroundWindow() };
        handle.to_non_null().map(Self::from_non_null)
    }

    pub fn get_desktop_window() -> io::Result<Self> {
        let handle = unsafe { GetDesktopWindow() };
        let handle = handle.to_non_null_else_error(|| ErrorKind::Other.into())?;
        Ok(Self::from_non_null(handle))
    }

    /// Returns all top-level windows of desktop apps.
    pub fn get_toplevel_windows() -> io::Result<Vec<Self>> {
        let mut result: Vec<WindowHandle> = Vec::new();
        let mut callback = |handle: HWND, _app_value: LPARAM| -> BOOL {
            let window_handle = handle
                .to_non_null()
                .expect("Window handle should not be null");
            result.push(WindowHandle::from_non_null(window_handle));
            TRUE
        };
        let ret_val = unsafe { EnumWindows(Some(sync_closure_to_callback2(&mut callback)), 0) };
        ret_val.if_null_get_last_error()?;
        Ok(result)
    }

    pub(crate) fn from_non_null(handle: NonNull<HWND__>) -> Self {
        Self { handle }
    }

    /// Checks if the handle points to an existing window.
    pub fn is_window(&self) -> bool {
        let result = unsafe { IsWindow(self.as_immutable_ptr()) };
        !result.is_null()
    }

    pub fn is_visible(&self) -> bool {
        let result = unsafe { IsWindowVisible(self.as_immutable_ptr()) };
        !result.is_null()
    }

    pub fn get_caption_text(&self) -> String {
        let required_length = unsafe { GetWindowTextLengthW(self.as_immutable_ptr()) };
        let required_length = if required_length <= 0 {
            return String::new();
        } else {
            1 + required_length
        };

        let mut buffer: Vec<WCHAR> = Vec::with_capacity(required_length as usize);
        let copied_chars = unsafe {
            GetWindowTextW(
                self.as_immutable_ptr(),
                buffer.as_mut_ptr(),
                required_length,
            )
        };
        if copied_chars <= 0 {
            return String::new();
        }
        unsafe {
            buffer.set_len(copied_chars as usize);
        }
        buffer.to_string_lossy()
    }

    pub fn set_as_foreground(&self) -> io::Result<()> {
        unsafe {
            SetForegroundWindow(self.as_immutable_ptr()).if_null_to_error(|| {
                io::Error::new(
                    ErrorKind::PermissionDenied,
                    "Cannot bring window to foreground",
                )
            })?;
        }
        Ok(())
    }

    pub fn set_as_active(&self) -> io::Result<()> {
        unsafe {
            SetActiveWindow(self.as_immutable_ptr()).if_null_get_last_error()?;
        }
        Ok(())
    }

    pub fn set_show_state(&self, state: WindowShowState) -> io::Result<()> {
        if self.is_window() {
            unsafe {
                ShowWindow(self.as_immutable_ptr(), state.into());
            }
            Ok(())
        } else {
            Err(io::Error::new(
                ErrorKind::NotFound,
                "Cannot set show state because window does not exist",
            ))
        }
    }

    pub fn get_placement(&self) -> io::Result<WindowPlacement> {
        let mut raw_placement: WINDOWPLACEMENT = WINDOWPLACEMENT {
            length: mem::size_of::<WINDOWPLACEMENT>() as UINT,
            ..Default::default()
        };
        unsafe {
            GetWindowPlacement(self.as_immutable_ptr(), &mut raw_placement)
                .if_null_get_last_error()?
        };
        Ok(WindowPlacement { raw_placement })
    }

    pub fn set_placement(&self, placement: &WindowPlacement) -> io::Result<()> {
        unsafe {
            SetWindowPlacement(self.as_immutable_ptr(), &placement.raw_placement)
                .if_null_get_last_error()?
        };
        Ok(())
    }

    pub fn get_class_name(&self) -> io::Result<String> {
        const BUFFER_SIZE: usize = MAX_WINDOW_CLASS_NAME_CHARS + 1;
        let mut buffer: Vec<WCHAR> = Vec::with_capacity(BUFFER_SIZE);
        let chars_copied = unsafe {
            GetClassNameW(
                self.as_immutable_ptr(),
                buffer.as_mut_ptr(),
                BUFFER_SIZE as i32,
            )
        };
        chars_copied.if_null_get_last_error()?;
        unsafe {
            buffer.set_len(chars_copied as usize);
        }
        Ok(buffer.to_string_lossy())
    }

    pub fn send_command(&self, action: WindowCommand) -> io::Result<()> {
        let result =
            unsafe { SendMessageW(self.as_immutable_ptr(), WM_SYSCOMMAND, action.into(), 0) };
        result.if_non_null_to_error(|| custom_err_with_code("Cannot perform window action", result))
    }

    #[inline(always)]
    pub fn flash(&self) {
        self.flash_custom(Default::default(), Default::default(), Default::default())
    }

    pub fn flash_custom(
        &self,
        element: FlashElement,
        duration: FlashDuration,
        frequency: FlashFrequency,
    ) {
        let mut raw_config: FLASHWINFO = Default::default();
        raw_config.cbSize = mem::size_of::<FLASHWINFO>() as UINT;
        raw_config.hwnd = self.as_immutable_ptr();
        let (count, mut flags) = match duration {
            FlashDuration::Count(count) => (count, 0),
            FlashDuration::CountUntilForeground(count) => (count, FLASHW_TIMERNOFG),
            FlashDuration::ContinuousUntilForeground => (0, FLASHW_TIMERNOFG),
            FlashDuration::Continuous => (0, FLASHW_TIMER),
        };
        flags |= DWORD::from(element);
        raw_config.dwFlags = flags;
        raw_config.uCount = count;
        raw_config.dwTimeout = match frequency {
            FlashFrequency::DefaultCursorBlinkRate => 0,
            FlashFrequency::Milliseconds(ms) => ms,
        };
        unsafe { FlashWindowEx(&mut raw_config) };
    }

    pub fn flash_stop(&self) {
        let mut raw_config: FLASHWINFO = Default::default();
        raw_config.cbSize = mem::size_of::<FLASHWINFO>() as UINT;
        raw_config.hwnd = self.as_immutable_ptr();
        raw_config.dwFlags = FLASHW_STOP;
        unsafe { FlashWindowEx(&mut raw_config) };
    }

    #[inline(always)]
    pub fn get_creator_thread_id(&self) -> ThreadId {
        self.get_creator_thread_process_ids().0
    }

    #[inline(always)]
    pub fn get_creator_process_id(&self) -> ProcessId {
        self.get_creator_thread_process_ids().1
    }

    fn get_creator_thread_process_ids(&self) -> (ThreadId, ProcessId) {
        let mut process_id: DWORD = 0;
        let thread_id =
            unsafe { GetWindowThreadProcessId(self.as_immutable_ptr(), &mut process_id) };
        (ThreadId(thread_id), ProcessId(process_id))
    }

    pub fn set_monitor_power(&self, level: MonitorPower) -> io::Result<()> {
        let result = unsafe {
            SendMessageW(
                self.as_immutable_ptr(),
                WM_SYSCOMMAND,
                SC_MONITORPOWER,
                level.into(),
            )
        };
        result.if_non_null_to_error(|| {
            custom_err_with_code("Cannot set monitor power using window", result)
        })
    }

    pub(crate) unsafe fn get_user_data_ptr<T>(&self) -> Option<NonNull<T>> {
        let ptr_value = GetWindowLongPtrW(self.as_immutable_ptr(), GWLP_USERDATA);
        NonNull::new(ptr_value as *mut T)
    }

    pub(crate) unsafe fn set_user_data_ptr<T>(&mut self, ptr: *const T) -> io::Result<()> {
        SetWindowLongPtrW(self.as_mutable_ptr(), GWLP_USERDATA, ptr as LONG_PTR);
        // TODO add error checking, distinguishing between old value 0 and an actual error (see MS docs)
        Ok(())
    }

    pub fn into_raw_handle(self) -> NonNull<HWND__> {
        self.handle
    }
}

impl ManagedHandle for WindowHandle {
    type Target = HWND__;

    #[inline(always)]
    fn as_immutable_ptr(&self) -> *mut Self::Target {
        self.handle.as_immutable_ptr()
    }
}

pub struct WindowClass<'res, WML, I> {
    atom: ATOM,
    icon: &'res I,
    phantom: PhantomData<(WML, &'res ())>,
}

impl<'res, WML: WindowMessageListener, I: Icon> WindowClass<'res, WML, I> {
    pub fn register_new(
        class_name: &str,
        background_brush: &'res impl Brush,
        icon: &'res I,
        cursor: &'res impl Cursor,
    ) -> io::Result<Self> {
        let class_name_wide = class_name.to_wide_string();

        // No need to reserve extra window memory if we only need a single pointer
        let class_def = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as UINT,
            lpfnWndProc: Some(generic_window_proc::<WML>),
            hIcon: icon.as_handle()?,
            hCursor: cursor.as_handle()?,
            hbrBackground: background_brush.as_handle()?,
            lpszClassName: class_name_wide.as_ptr(),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&class_def).if_null_get_last_error()? };
        Ok(WindowClass {
            atom,
            icon,
            phantom: PhantomData,
        })
    }
}

impl<WML, I> Drop for WindowClass<'_, WML, I> {
    fn drop(&mut self) {
        unsafe {
            UnregisterClassW(self.atom as *const WCHAR, ptr::null_mut())
                .if_null_get_last_error()
                .unwrap();
        }
    }
}

pub struct Window<'class, 'listener, WML, I> {
    class: &'class WindowClass<'class, WML, I>,
    handle: WindowHandle,
    phantom: PhantomData<&'listener mut WML>,
}

impl<'class, 'listener, WML: WindowMessageListener, I: Icon> Window<'class, 'listener, WML, I> {
    pub fn create_new(
        class: &'class WindowClass<WML, I>,
        listener: &'listener WML,
        window_name: &str,
    ) -> io::Result<Self> {
        let h_wnd: NonNull<HWND__> = unsafe {
            CreateWindowExW(
                0,
                class.atom as *const WCHAR,
                window_name.to_wide_string().as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                0,
                CW_USEDEFAULT,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
            .to_non_null_else_get_last_error()?
        };
        let mut handle = WindowHandle::from_non_null(h_wnd);
        unsafe {
            handle.set_user_data_ptr(listener)?;
        }
        Ok(Window {
            class,
            handle,
            phantom: PhantomData,
        })
    }

    pub fn add_notification_icon<'a>(
        &'a self,
        icon_id: NotificationIconId,
        icon: Option<&'a impl Icon>,
        tooltip_text: Option<&str>,
    ) -> io::Result<NotificationIcon<'a, WML, I>> {
        // For GUID handling maybe look at generating it from the executable path:
        // https://stackoverflow.com/questions/7432319/notifyicondata-guid-problem
        let chosen_icon_handle = if let Some(icon) = icon {
            icon.as_handle()?
        } else {
            self.class.icon.as_handle()?
        };
        let mut call_data = get_notification_call_data(
            &self.handle,
            icon_id,
            true,
            Some(chosen_icon_handle),
            tooltip_text,
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &mut call_data).if_null_to_error(|| {
                io::Error::new(io::ErrorKind::Other, "Cannot add notification icon")
            })?;
            Shell_NotifyIconW(NIM_SETVERSION, &mut call_data).if_null_to_error(|| {
                io::Error::new(io::ErrorKind::Other, "Cannot set notification version")
            })?;
        };
        Ok(NotificationIcon {
            id: icon_id,
            window: self,
        })
    }
}

impl<WML, I> Drop for Window<'_, '_, WML, I> {
    fn drop(&mut self) {
        unsafe {
            if self.handle.is_window() {
                DestroyWindow(self.handle.as_mutable_ptr())
                    .if_null_get_last_error()
                    .unwrap();
            }
        }
    }
}

impl<WML, I> AsRef<WindowHandle> for Window<'_, '_, WML, I> {
    fn as_ref(&self) -> &WindowHandle {
        &self.handle
    }
}

impl<WML, I> AsMut<WindowHandle> for Window<'_, '_, WML, I> {
    fn as_mut(&mut self) -> &mut WindowHandle {
        &mut self.handle
    }
}

#[derive(IntoPrimitive, TryFromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(i32)]
pub enum WindowShowState {
    Hide = SW_HIDE,
    Maximize = SW_MAXIMIZE,
    Minimize = SW_MINIMIZE,
    Restore = SW_RESTORE,
    Show = SW_SHOW,
    ShowMinimized = SW_SHOWMINIMIZED,
    ShowMinNoActivate = SW_SHOWMINNOACTIVE,
    ShowNoActivate = SW_SHOWNA,
    ShowNormalNoActivate = SW_SHOWNOACTIVATE,
    ShowNormal = SW_SHOWNORMAL,
}

/// DPI-scaled virtual coordinates.
pub type Point = POINT;
/// DPI-scaled virtual coordinates of a rectangle.
pub type Rectangle = RECT;

#[derive(Copy, Clone)]
pub struct WindowPlacement {
    raw_placement: WINDOWPLACEMENT,
}

impl WindowPlacement {
    pub fn get_show_state(&self) -> Option<WindowShowState> {
        (i32::try_from(self.raw_placement.showCmd).unwrap())
            .try_into()
            .ok()
    }

    pub fn set_show_state(&mut self, state: WindowShowState) {
        self.raw_placement.showCmd = i32::from(state) as u32;
    }

    pub fn get_minimized_position(&self) -> Point {
        self.raw_placement.ptMinPosition
    }

    pub fn set_minimized_position(&mut self, coords: Point) {
        self.raw_placement.ptMinPosition = coords;
        self.raw_placement.flags |= WPF_SETMINPOSITION;
    }

    pub fn get_maximized_position(&self) -> Point {
        self.raw_placement.ptMaxPosition
    }

    pub fn set_maximized_position(&mut self, coords: Point) {
        self.raw_placement.ptMaxPosition = coords;
    }

    pub fn get_restored_position(&self) -> Rectangle {
        self.raw_placement.rcNormalPosition
    }

    pub fn set_restored_position(&mut self, rectangle: Rectangle) {
        self.raw_placement.rcNormalPosition = rectangle;
    }
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq)]
#[non_exhaustive]
#[repr(usize)]
pub enum WindowCommand {
    Close = SC_CLOSE,
    Maximize = SC_MAXIMIZE,
    Minimize = SC_MINIMIZE,
    Restore = SC_RESTORE,
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum FlashElement {
    Caption = FLASHW_CAPTION,
    Taskbar = FLASHW_TRAY,
    CaptionPlusTaskbar = FLASHW_ALL,
}

impl Default for FlashElement {
    fn default() -> Self {
        FlashElement::CaptionPlusTaskbar
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum FlashDuration {
    Count(u32),
    CountUntilForeground(u32),
    ContinuousUntilForeground,
    Continuous,
}

impl Default for FlashDuration {
    fn default() -> Self {
        FlashDuration::CountUntilForeground(5)
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum FlashFrequency {
    DefaultCursorBlinkRate,
    Milliseconds(u32),
}

impl Default for FlashFrequency {
    fn default() -> Self {
        FlashFrequency::DefaultCursorBlinkRate
    }
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq)]
#[repr(isize)]
pub enum MonitorPower {
    On = -1,
    Low = 1,
    Off = 2,
}

pub struct NotificationIcon<'a, WML, I> {
    id: NotificationIconId,
    window: &'a Window<'a, 'a, WML, I>,
}

impl<'a, WML, I> NotificationIcon<'a, WML, I> {
    pub fn set_icon(&mut self, icon: &'a impl Icon) -> io::Result<()> {
        let mut call_data = get_notification_call_data(
            &self.window.handle,
            self.id,
            false,
            Some(icon.as_handle()?),
            None,
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut call_data).if_null_to_error(|| {
                io::Error::new(io::ErrorKind::Other, "Cannot set notification icon")
            })?;
        };
        Ok(())
    }

    pub fn set_icon_hidden_state(&mut self, hidden: bool) -> io::Result<()> {
        let mut call_data = get_notification_call_data(
            &self.window.handle,
            self.id,
            false,
            None,
            None,
            Some(hidden),
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut call_data).if_null_to_error(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "Cannot set notification icon hidden state",
                )
            })?;
        };
        Ok(())
    }

    pub fn set_tooltip_text(&mut self, text: &str) -> io::Result<()> {
        let mut call_data = get_notification_call_data(
            &self.window.handle,
            self.id,
            false,
            None,
            Some(text),
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut call_data).if_null_to_error(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "Cannot set notification icon tooltip text",
                )
            })?;
        };
        Ok(())
    }

    pub fn set_balloon_notification(
        &mut self,
        notification: Option<BalloonNotification>,
    ) -> io::Result<()> {
        let mut call_data = get_notification_call_data(
            &self.window.handle,
            self.id,
            false,
            None,
            None,
            None,
            Some(notification),
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut call_data).if_null_to_error(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "Cannot set notification icon balloon text",
                )
            })?;
        };
        Ok(())
    }
}

impl<WML, I> Drop for NotificationIcon<'_, WML, I> {
    fn drop(&mut self) {
        let mut call_data =
            get_notification_call_data(&self.window.handle, self.id, false, None, None, None, None);
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &mut call_data)
                .if_null_to_error(|| {
                    io::Error::new(io::ErrorKind::Other, "Cannot remove notification icon")
                })
                .unwrap();
        }
    }
}

fn get_notification_call_data(
    window_handle: &WindowHandle,
    icon_id: NotificationIconId,
    set_callback_message: bool,
    maybe_icon: Option<HICON>,
    maybe_tooltip_str: Option<&str>,
    icon_hidden_state: Option<bool>,
    maybe_balloon_text: Option<Option<BalloonNotification>>,
) -> NOTIFYICONDATAW {
    let mut icon_data = NOTIFYICONDATAW {
        cbSize: mem::size_of::<NOTIFYICONDATAW>()
            .try_into()
            .expect("NOTIFYICONDATAW size conversion failed"),
        hWnd: window_handle.as_immutable_ptr(),
        ..Default::default()
    };
    unsafe {
        *icon_data.u.uVersion_mut() = NOTIFYICON_VERSION_4;
    }
    match icon_id {
        NotificationIconId::GUID(id) => {
            icon_data.guidItem = id;
            icon_data.uFlags |= NIF_GUID;
        }
        NotificationIconId::Simple(simple_id) => icon_data.uID = simple_id.into(),
    };
    if set_callback_message {
        icon_data.uCallbackMessage = message::RawMessage::ID_NOTIFICATION_ICON_MSG;
        icon_data.uFlags |= NIF_MESSAGE;
    }
    if let Some(icon) = maybe_icon {
        icon_data.hIcon = icon;
        icon_data.uFlags |= NIF_ICON;
    }
    if let Some(tooltip_str) = maybe_tooltip_str {
        let chars = to_wide_chars_iter(tooltip_str)
            .take(icon_data.szTip.len() - 1)
            .chain(std::iter::once(0))
            .enumerate();
        for (i, w_char) in chars {
            icon_data.szTip[i] = w_char;
        }
        icon_data.uFlags |= NIF_TIP;
        // Standard tooltip is normally suppressed on NOTIFYICON_VERSION_4
        icon_data.uFlags |= NIF_SHOWTIP;
    }
    if let Some(hidden_state) = icon_hidden_state {
        if hidden_state {
            icon_data.dwState |= NIS_HIDDEN;
            icon_data.dwStateMask |= NIS_HIDDEN;
        }
        icon_data.uFlags |= NIF_STATE;
    }
    if let Some(set_balloon_notification) = maybe_balloon_text {
        if let Some(balloon) = set_balloon_notification {
            let body_chars = to_wide_chars_iter(balloon.body)
                .take(icon_data.szInfo.len() - 1)
                .chain(std::iter::once(0))
                .enumerate();
            for (i, w_char) in body_chars {
                icon_data.szInfo[i] = w_char;
            }
            let title_chars = to_wide_chars_iter(balloon.title)
                .take(icon_data.szInfoTitle.len() - 1)
                .chain(std::iter::once(0))
                .enumerate();
            for (i, w_char) in title_chars {
                icon_data.szInfoTitle[i] = w_char;
            }
            icon_data.dwInfoFlags |= DWORD::from(balloon.icon);
        }
        icon_data.uFlags |= NIF_INFO;
    }
    icon_data
}

#[derive(Copy, Clone)]
pub enum NotificationIconId {
    Simple(u16),
    GUID(GUID),
}

impl Default for NotificationIconId {
    fn default() -> Self {
        NotificationIconId::Simple(0)
    }
}

#[derive(Copy, Clone)]
pub struct BalloonNotification<'a> {
    title: &'a str,
    body: &'a str,
    icon: BalloonNotificationStandardIcon,
}

#[derive(IntoPrimitive, Copy, Clone)]
#[repr(u32)]
pub enum BalloonNotificationStandardIcon {
    None = NIIF_NONE,
    Info = NIIF_INFO,
    Warning = NIIF_WARNING,
    Error = NIIF_ERROR,
}

impl Default for BalloonNotificationStandardIcon {
    fn default() -> Self {
        BalloonNotificationStandardIcon::None
    }
}

/// Taskbar progress state animation type.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ProgressState {
    /// Stops displaying progress and returns the button to its normal state.
    NoProgress = TBPF_NOPROGRESS,
    /// Shows a "working" animation without indicating a completion percentage.
    Indeterminate = TBPF_INDETERMINATE,
    /// Shows a progress indicator displaying the amount of work being completed.
    Normal = TBPF_NORMAL,
    /// The progress indicator turns red to show that an error has occurred. This is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a red determinate display
    /// of a generic percentage not indicative of actual progress.
    Error = TBPF_ERROR,
    /// The progress indicator turns yellow to show that progress is currently stopped. This is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a yellow determinate display
    /// of a generic percentage not indicative of actual progress.
    Paused = TBPF_PAUSED,
}

impl Default for ProgressState {
    fn default() -> Self {
        ProgressState::NoProgress
    }
}

/// Taskbar functionality.
pub struct Taskbar {
    taskbar_list_3: ComPtr<ITaskbarList3>,
}

impl Taskbar {
    pub fn new() -> io::Result<Self> {
        let result = Taskbar {
            taskbar_list_3: ITaskbarList3::new_instance()?,
        };
        Ok(result)
    }

    /// Sets the progress state taskbar animation of a window.
    ///
    /// See also: [Microsoft docs](https://docs.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-itaskbarlist3-setprogressstate)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use winapi_easy::ui::{
    ///     ProgressState,
    ///     Taskbar,
    ///     WindowHandle,
    /// };
    ///
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// let mut window = WindowHandle::get_console_window().expect("Cannot get console window");
    /// let mut taskbar = Taskbar::new()?;
    ///
    /// taskbar.set_progress_state(&mut window, ProgressState::Indeterminate)?;
    /// thread::sleep(Duration::from_millis(3000));
    /// taskbar.set_progress_state(&mut window, ProgressState::NoProgress)?;
    ///
    /// # std::result::Result::<(), std::io::Error>::Ok(())
    /// ```
    pub fn set_progress_state(
        &mut self,
        window: &mut WindowHandle,
        state: ProgressState,
    ) -> io::Result<()> {
        let ret_val: HRESULT = unsafe {
            self.taskbar_list_3
                .SetProgressState(window.as_mutable_ptr(), state.into())
        };
        ret_val.if_not_eq_to_error(S_OK, || {
            custom_err_with_code("Error setting progress state", ret_val)
        })
    }

    /// Sets the completion amount of the taskbar progress state animation.
    pub fn set_progress_value(
        &mut self,
        window: &mut WindowHandle,
        completed: u64,
        total: u64,
    ) -> io::Result<()> {
        let ret_val: HRESULT = unsafe {
            self.taskbar_list_3
                .SetProgressValue(window.as_mutable_ptr(), completed, total)
        };
        ret_val.if_not_eq_to_error(S_OK, || {
            custom_err_with_code("Error setting progress value", ret_val)
        })
    }
}

pub fn allocate_console() -> io::Result<()> {
    unsafe {
        AllocConsole().if_null_get_last_error()?;
    }
    Ok(())
}

pub fn lock_workstation() -> io::Result<()> {
    // Because the function executes asynchronously, a nonzero return value indicates that the operation has been initiated.
    // It does not indicate whether the workstation has been successfully locked.
    let _ = unsafe { LockWorkStation().if_null_get_last_error()? };
    Ok(())
}

#[cfg(test)]
mod tests {
    use more_asserts::*;

    use super::*;

    #[test]
    fn check_toplevel_windows() -> io::Result<()> {
        let all_windows = WindowHandle::get_toplevel_windows()?;
        assert_gt!(all_windows.len(), 0);
        for window in all_windows {
            assert!(window.is_window());
        }
        Ok(())
    }
}
