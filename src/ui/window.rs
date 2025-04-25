//! UI components related to windows.

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{
    Display,
    Formatter,
};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::{
    io,
    mem,
    ptr,
    vec,
};

use derive_more::BitOr;
use num_enum::{
    IntoPrimitive,
    TryFromPrimitive,
};
use windows::Win32::Foundation::{
    ERROR_SUCCESS,
    GetLastError,
    HWND,
    LPARAM,
    NO_ERROR,
    SetLastError,
    WPARAM,
};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetActiveWindow;
use windows::Win32::UI::Shell::{
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
    NOTIFY_ICON_INFOTIP_FLAGS,
    NOTIFY_ICON_STATE,
    NOTIFYICON_VERSION_4,
    NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT,
    CreateWindowExW,
    DestroyWindow,
    EnumWindows,
    FLASHW_ALL,
    FLASHW_CAPTION,
    FLASHW_STOP,
    FLASHW_TIMER,
    FLASHW_TIMERNOFG,
    FLASHW_TRAY,
    FLASHWINFO,
    FLASHWINFO_FLAGS,
    FlashWindowEx,
    GWLP_USERDATA,
    GetClassNameW,
    GetClientRect,
    GetDesktopWindow,
    GetForegroundWindow,
    GetWindowLongPtrW,
    GetWindowPlacement,
    GetWindowTextLengthW,
    GetWindowTextW,
    HICON,
    IsWindow,
    IsWindowVisible,
    RegisterClassExW,
    SC_CLOSE,
    SC_MAXIMIZE,
    SC_MINIMIZE,
    SC_MONITORPOWER,
    SC_RESTORE,
    SHOW_WINDOW_CMD,
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
    SendMessageW,
    SetForegroundWindow,
    SetWindowLongPtrW,
    SetWindowPlacement,
    SetWindowTextW,
    ShowWindow,
    UnregisterClassW,
    WINDOW_EX_STYLE,
    WINDOW_STYLE,
    WINDOWPLACEMENT,
    WM_SYSCOMMAND,
    WNDCLASSEXW,
    WPF_SETMINPOSITION,
    WS_CHILD,
    WS_CLIPCHILDREN,
    WS_EX_LAYERED,
    WS_EX_LEFT,
    WS_EX_TOPMOST,
    WS_EX_TRANSPARENT,
    WS_OVERLAPPED,
    WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};
use windows::core::{
    BOOL,
    GUID,
    PCWSTR,
};

use super::{
    Point,
    RectTransform,
    Rectangle,
};
use crate::internal::{
    OpaqueClosure,
    OpaqueRawBox,
    ReturnValue,
    custom_err_with_code,
    with_sync_closure_to_callback2,
};
#[cfg(feature = "process")]
use crate::process::{
    ProcessId,
    ThreadId,
};
use crate::string::{
    FromWideString,
    ZeroTerminatedWideString,
    to_wide_chars_iter,
};
use crate::ui::messaging::{
    ListenerAnswer,
    ListenerMessage,
    generic_window_proc,
};
use crate::ui::resource::{
    Brush,
    BuiltinColor,
    BuiltinCursor,
    BuiltinIcon,
    Cursor,
    Icon,
};

/// A (non-null) handle to a window.
///
/// # Multithreading
///
/// This handle is not [`Send`] and [`Sync`] because if the window was not created by this thread,
/// then it is not guaranteed that the handle continues pointing to the same window because the underlying handles
/// can get invalid or even recycled.
///
/// # Mutability
///
/// Even though various functions on this type are mutating, they all take non-mut references since
/// it would be too hard to guarantee exclusive references when window messages are involved. The problem
/// in that case is that the windows API will call back into Rust code and that code would then need
/// exclusive references, which would at least make the API rather cumbersome. If an elegant solution
/// to this problem is found, this API may change.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct WindowHandle {
    raw_handle: HWND,
    _marker: PhantomData<*mut ()>,
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(WindowHandle: Send, Sync);

impl WindowHandle {
    /// Returns the console window associated with the current process, if there is one.
    pub fn get_console_window() -> Option<Self> {
        let handle = unsafe { GetConsoleWindow() };
        Self::from_maybe_null(handle)
    }

    /// Returns the current foreground window, if any.
    pub fn get_foreground_window() -> Option<Self> {
        let handle = unsafe { GetForegroundWindow() };
        Self::from_maybe_null(handle)
    }

    /// Returns the 'desktop' window.
    pub fn get_desktop_window() -> io::Result<Self> {
        let handle = unsafe { GetDesktopWindow() };
        handle
            .if_null_to_error(|| io::ErrorKind::Other.into())
            .map(Self::from_non_null)
    }

    /// Returns all top-level windows of desktop apps.
    pub fn get_toplevel_windows() -> io::Result<Vec<Self>> {
        let mut result: Vec<WindowHandle> = Vec::new();
        let callback = |handle: HWND, _app_value: LPARAM| -> BOOL {
            let window_handle = Self::from_maybe_null(handle).unwrap_or_else(|| {
                unreachable!("Window handle passed to callback should never be null")
            });
            result.push(window_handle);
            true.into()
        };
        let acceptor = |raw_callback| unsafe { EnumWindows(Some(raw_callback), LPARAM::default()) };
        with_sync_closure_to_callback2(callback, acceptor)?;
        Ok(result)
    }

    pub(crate) fn from_non_null(handle: HWND) -> Self {
        Self {
            raw_handle: handle,
            _marker: PhantomData,
        }
    }

    pub(crate) fn from_maybe_null(handle: HWND) -> Option<Self> {
        if handle.is_null() {
            None
        } else {
            Some(Self {
                raw_handle: handle,
                _marker: PhantomData,
            })
        }
    }

    /// Checks if the handle points to an existing window.
    pub fn is_window(&self) -> bool {
        let result = unsafe { IsWindow(Some(self.raw_handle)) };
        result.as_bool()
    }

    pub fn is_visible(&self) -> bool {
        let result = unsafe { IsWindowVisible(self.raw_handle) };
        result.as_bool()
    }

    /// Returns the window caption text, converted to UTF-8 in a potentially lossy way.
    pub fn get_caption_text(&self) -> String {
        let required_length: usize = unsafe { GetWindowTextLengthW(self.raw_handle) }
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        let required_length = if required_length == 0 {
            return String::new();
        } else {
            1 + required_length
        };

        let mut buffer: Vec<u16> = vec![0; required_length as usize];
        let copied_chars = unsafe { GetWindowTextW(self.raw_handle, buffer.as_mut()) }
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        if copied_chars == 0 {
            String::new()
        } else {
            // Normally unnecessary, but the text length can theoretically change between the 2 API calls
            buffer.truncate(copied_chars);
            buffer.to_string_lossy()
        }
    }

    /// Sets the window caption text.
    pub fn set_caption_text(&self, text: &str) -> io::Result<()> {
        let ret_val = unsafe {
            SetWindowTextW(
                self.raw_handle,
                ZeroTerminatedWideString::from_os_str(text).as_raw_pcwstr(),
            )
        };
        ret_val?;
        Ok(())
    }

    /// Brings the window to the foreground.
    pub fn set_as_foreground(&self) -> io::Result<()> {
        unsafe {
            SetForegroundWindow(self.raw_handle).if_null_to_error_else_drop(|| {
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Cannot bring window to foreground",
                )
            })?;
        }
        Ok(())
    }

    /// Sets the window as the currently active (selected) window.
    pub fn set_as_active(&self) -> io::Result<()> {
        unsafe {
            SetActiveWindow(self.raw_handle)?;
        }
        Ok(())
    }

    /// Changes the window show state.
    pub fn set_show_state(&self, state: WindowShowState) -> io::Result<()> {
        if self.is_window() {
            unsafe {
                let _ = ShowWindow(self.raw_handle, state.into());
            }
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot set show state because window does not exist",
            ))
        }
    }

    /// Returns the window's show state and positions.
    pub fn get_placement(&self) -> io::Result<WindowPlacement> {
        let mut raw_placement: WINDOWPLACEMENT = WINDOWPLACEMENT {
            length: mem::size_of::<WINDOWPLACEMENT>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            ..Default::default()
        };
        unsafe { GetWindowPlacement(self.raw_handle, &mut raw_placement)? };
        Ok(WindowPlacement { raw_placement })
    }

    /// Sets the window's show state and positions.
    pub fn set_placement(&self, placement: &WindowPlacement) -> io::Result<()> {
        unsafe { SetWindowPlacement(self.raw_handle, &placement.raw_placement)? };
        Ok(())
    }

    /// Returns the window's client area rectangle relative to the screen.
    pub fn get_client_area_coords(&self) -> io::Result<Rectangle> {
        let mut result_rect: Rectangle = Default::default();
        unsafe { GetClientRect(self.raw_handle, &mut result_rect) }?;
        self.map_points(None, result_rect.as_point_array_mut())?;
        Ok(result_rect)
    }

    pub(crate) fn map_points(
        &self,
        other_window: Option<Self>,
        points: &mut [Point],
    ) -> io::Result<()> {
        unsafe { SetLastError(ERROR_SUCCESS) };
        let map_result = unsafe {
            MapWindowPoints(
                Some(self.raw_handle),
                other_window.map(|x| x.raw_handle),
                points,
            )
        };
        if map_result == 0 {
            let last_error = unsafe { GetLastError() };
            if last_error != ERROR_SUCCESS {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Returns the class name of the window's associated [`WindowClass`].
    pub fn get_class_name(&self) -> io::Result<String> {
        const BUFFER_SIZE: usize = WindowClass::MAX_WINDOW_CLASS_NAME_CHARS + 1;
        let mut buffer: Vec<u16> = vec![0; BUFFER_SIZE];
        let chars_copied: usize = unsafe { GetClassNameW(self.raw_handle, buffer.as_mut()) }
            .if_null_get_last_error()?
            .try_into()
            .unwrap_or_else(|_| unreachable!());
        buffer.truncate(chars_copied);
        Ok(buffer.to_string_lossy())
    }

    /// Sends a command to the window, same as if one of the symbols in its top right were clicked.
    pub fn send_command(&self, action: WindowCommand) -> io::Result<()> {
        let result = unsafe {
            SendMessageW(
                self.raw_handle,
                WM_SYSCOMMAND,
                Some(WPARAM(action.to_usize())),
                None,
            )
        };
        result
            .if_non_null_to_error(|| custom_err_with_code("Cannot perform window action", result.0))
    }

    /// Flashes the window using default flash settings.
    ///
    /// Same as [`Self::flash_custom`] using [`Default::default`] for all parameters.
    pub fn flash(&self) {
        self.flash_custom(Default::default(), Default::default(), Default::default());
    }

    /// Flashes the window, allowing various customization parameters.
    pub fn flash_custom(
        &self,
        element: FlashElement,
        duration: FlashDuration,
        frequency: FlashInterval,
    ) {
        let (count, flags) = match duration {
            FlashDuration::Count(count) => (count, Default::default()),
            FlashDuration::CountUntilForeground(count) => (count, FLASHW_TIMERNOFG),
            FlashDuration::ContinuousUntilForeground => (0, FLASHW_TIMERNOFG),
            FlashDuration::Continuous => (0, FLASHW_TIMER),
        };
        let flags = flags | element.to_flashwinfo_flags();
        let raw_config = FLASHWINFO {
            cbSize: mem::size_of::<FLASHWINFO>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            hwnd: self.into(),
            dwFlags: flags,
            uCount: count,
            dwTimeout: match frequency {
                FlashInterval::DefaultCursorBlinkInterval => 0,
                FlashInterval::Milliseconds(ms) => ms,
            },
        };
        unsafe {
            let _ = FlashWindowEx(&raw_config);
        };
    }

    /// Stops the window from flashing.
    pub fn flash_stop(&self) {
        let raw_config = FLASHWINFO {
            cbSize: mem::size_of::<FLASHWINFO>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            hwnd: self.into(),
            dwFlags: FLASHW_STOP,
            ..Default::default()
        };
        unsafe {
            let _ = FlashWindowEx(&raw_config);
        };
    }

    /// Returns the thread ID that created this window.
    #[cfg(feature = "process")]
    pub fn get_creator_thread_id(&self) -> ThreadId {
        self.get_creator_thread_process_ids().0
    }

    /// Returns the process ID that created this window.
    #[cfg(feature = "process")]
    pub fn get_creator_process_id(&self) -> ProcessId {
        self.get_creator_thread_process_ids().1
    }

    #[cfg(feature = "process")]
    fn get_creator_thread_process_ids(&self) -> (ThreadId, ProcessId) {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut process_id: u32 = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(self.raw_handle, Some(&mut process_id)) };
        (ThreadId(thread_id), ProcessId(process_id))
    }

    /// Returns all top-level (non-child) windows created by the thread.
    #[cfg(feature = "process")]
    pub fn get_nonchild_windows(thread_id: ThreadId) -> Vec<Self> {
        use windows::Win32::UI::WindowsAndMessaging::EnumThreadWindows;
        let mut result: Vec<WindowHandle> = Vec::new();
        let callback = |handle: HWND, _app_value: LPARAM| -> BOOL {
            let window_handle = WindowHandle::from_maybe_null(handle).unwrap_or_else(|| {
                unreachable!("Window handle passed to callback should never be null")
            });
            result.push(window_handle);
            true.into()
        };
        let acceptor = |raw_callback| {
            let _ =
                unsafe { EnumThreadWindows(thread_id.0, Some(raw_callback), LPARAM::default()) };
        };
        with_sync_closure_to_callback2(callback, acceptor);
        result
    }

    /// Turns the monitor on or off.
    ///
    /// Windows requires this command to be sent through a window, e.g. using
    /// [`WindowHandle::get_foreground_window`].
    pub fn set_monitor_power(&self, level: MonitorPower) -> io::Result<()> {
        let result = unsafe {
            SendMessageW(
                self.raw_handle,
                WM_SYSCOMMAND,
                Some(WPARAM(
                    SC_MONITORPOWER
                        .try_into()
                        .unwrap_or_else(|_| unreachable!()),
                )),
                Some(LPARAM(level.into())),
            )
        };
        result.if_non_null_to_error(|| {
            custom_err_with_code("Cannot set monitor power using window", result.0)
        })
    }

    pub(crate) unsafe fn get_user_data_ptr<T>(&self) -> Option<NonNull<T>> {
        let ptr_value = unsafe { GetWindowLongPtrW(self.raw_handle, GWLP_USERDATA) };
        NonNull::new(ptr::with_exposed_provenance_mut(ptr_value.cast_unsigned()))
    }

    pub(crate) unsafe fn set_user_data_ptr<T>(&self, ptr: *const T) -> io::Result<()> {
        unsafe { SetLastError(NO_ERROR) };
        let ret_val = unsafe {
            SetWindowLongPtrW(
                self.raw_handle,
                GWLP_USERDATA,
                ptr.expose_provenance().cast_signed(),
            )
        };
        if ret_val == 0 {
            let err_val = unsafe { GetLastError() };
            if err_val != NO_ERROR {
                return Err(custom_err_with_code(
                    "Cannot set window procedure",
                    err_val.0,
                ));
            }
        }
        Ok(())
    }
}

impl From<WindowHandle> for HWND {
    /// Returns the underlying raw window handle used by [`windows`].
    fn from(value: WindowHandle) -> Self {
        value.raw_handle
    }
}

impl From<&WindowHandle> for HWND {
    /// Returns the underlying raw window handle used by [`windows`].
    fn from(value: &WindowHandle) -> Self {
        value.raw_handle
    }
}

impl TryFrom<HWND> for WindowHandle {
    type Error = TryFromHWNDError;

    /// Returns a new window handle from a raw handle if it is non-null.
    fn try_from(value: HWND) -> Result<Self, Self::Error> {
        WindowHandle::from_maybe_null(value).ok_or(TryFromHWNDError(()))
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TryFromHWNDError(pub(crate) ());

impl Display for TryFromHWNDError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "HWND value must not be null")
    }
}

impl Error for TryFromHWNDError {}

/// Window class serving as a base for [`Window`].
#[derive(Debug)]
pub struct WindowClass<'res> {
    atom: u16,
    #[allow(dead_code)]
    icon_handle: HICON,
    phantom: PhantomData<&'res ()>,
}

impl WindowClass<'_> {
    const MAX_WINDOW_CLASS_NAME_CHARS: usize = 256;

    fn raw_class_identifier(&self) -> PCWSTR {
        PCWSTR(self.atom as *const u16)
    }
}

impl<'res> WindowClass<'res> {
    /// Registers a new class.
    ///
    /// This class can then be used to create instances of [`Window`].
    ///
    /// The class name will be generated from the given prefix by adding a random base64 encoded UUID
    /// to ensure uniqueness. This means that the maximum length of the class name prefix is a little less
    /// than the standard 256 characters for class names.
    pub fn register_new<B, I, C>(
        class_name_prefix: &str,
        appearance: WindowClassAppearance<B, I, C>,
    ) -> io::Result<Self>
    where
        B: Brush + 'res,
        I: Icon + 'res,
        C: Cursor + 'res,
    {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;

        let base64_uuid = URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes());
        let class_name = class_name_prefix.to_string() + "_" + &base64_uuid;

        let icon_handle = appearance
            .icon
            .map_or_else(|| Ok(Default::default()), |x| x.as_handle())?;
        // No need to reserve extra window memory if we only need a single pointer
        let class_def = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            lpfnWndProc: Some(generic_window_proc),
            hIcon: icon_handle,
            hCursor: appearance
                .cursor
                .map_or_else(|| Ok(Default::default()), |x| x.as_handle())?,
            hbrBackground: appearance
                .background_brush
                .map_or_else(|| Ok(Default::default()), |x| x.as_handle())?,
            lpszClassName: ZeroTerminatedWideString::from_os_str(class_name).as_raw_pcwstr(),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&class_def).if_null_get_last_error()? };
        Ok(WindowClass {
            atom,
            icon_handle,
            phantom: PhantomData,
        })
    }
}

impl Drop for WindowClass<'_> {
    /// Unregisters the class on drop.
    fn drop(&mut self) {
        unsafe {
            UnregisterClassW(self.raw_class_identifier(), None).unwrap();
        }
    }
}

#[derive(Clone, Debug)]
pub struct WindowClassAppearance<B, I, C> {
    pub background_brush: Option<B>,
    pub icon: Option<I>,
    pub cursor: Option<C>,
}

impl WindowClassAppearance<BuiltinColor, BuiltinIcon, BuiltinCursor> {
    pub fn empty() -> Self {
        Self {
            background_brush: None,
            icon: None,
            cursor: None,
        }
    }
}

impl Default for WindowClassAppearance<BuiltinColor, BuiltinIcon, BuiltinCursor> {
    fn default() -> Self {
        Self {
            background_brush: Some(Default::default()),
            icon: Some(Default::default()),
            cursor: Some(Default::default()),
        }
    }
}

pub trait WindowKind {
    fn handle(&self) -> WindowHandle;
}

/// A window based on a [`WindowClass`].
#[derive(Debug)]
pub struct Window<'class, 'listener> {
    handle: WindowHandle,
    #[allow(dead_code)]
    opaque_listener: OpaqueRawBox<'listener>,
    phantom: PhantomData<&'class ()>,
}

impl<'class, 'listener> Window<'class, 'listener> {
    /// Creates a new window.
    ///
    /// User interaction with the window will result in messages sent to the [`WindowMessageListener`] provided here.
    pub fn create_new<WML>(
        class: &'class WindowClass,
        listener: WML,
        window_name: &str,
        appearance: WindowAppearance,
        parent: Option<&WindowHandle>,
    ) -> io::Result<Self>
    where
        WML: FnMut(ListenerMessage) -> ListenerAnswer + 'listener,
    {
        let h_wnd: HWND = unsafe {
            CreateWindowExW(
                appearance.extended_style.into(),
                class.raw_class_identifier(),
                ZeroTerminatedWideString::from_os_str(window_name).as_raw_pcwstr(),
                appearance.style.into(),
                CW_USEDEFAULT,
                0,
                CW_USEDEFAULT,
                0,
                parent.map(|x| x.raw_handle),
                None,
                None,
                None,
            )?
        };
        let mut opaque_listener = OpaqueRawBox::new(OpaqueClosure::new(listener));
        let handle = WindowHandle::from_non_null(h_wnd);
        unsafe {
            handle.set_user_data_ptr(opaque_listener.as_mut_ptr::<()>())?;
        }
        Ok(Window {
            handle,
            opaque_listener,
            phantom: PhantomData,
        })
    }

    /// Changes the [`WindowMessageListener`].
    pub fn set_listener<WML>(&mut self, listener: WML) -> io::Result<()>
    where
        WML: FnMut(ListenerMessage) -> ListenerAnswer + 'listener,
    {
        let mut opaque_listener = OpaqueRawBox::new(OpaqueClosure::new(listener));
        unsafe {
            self.handle
                .set_user_data_ptr(opaque_listener.as_mut_ptr::<()>())?;
        }
        Ok(())
    }
}

impl WindowKind for Window<'_, '_> {
    fn handle(&self) -> WindowHandle {
        self.handle
    }
}

impl Drop for Window<'_, '_> {
    fn drop(&mut self) {
        unsafe {
            if self.handle.is_window() {
                DestroyWindow(self.handle.raw_handle).unwrap();
            }
        }
    }
}

impl AsRef<WindowHandle> for Window<'_, '_> {
    fn as_ref(&self) -> &WindowHandle {
        &self.handle
    }
}

impl AsMut<WindowHandle> for Window<'_, '_> {
    fn as_mut(&mut self) -> &mut WindowHandle {
        &mut self.handle
    }
}

/// Window style.
///
/// Using combinations is possible with [`std::ops::BitOr`].
///
/// See also: [Microsoft docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-styles)
#[derive(IntoPrimitive, TryFromPrimitive, BitOr, Copy, Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
#[repr(u32)]
pub enum WindowStyle {
    Child = WS_CHILD.0,
    ClipChildren = WS_CLIPCHILDREN.0,
    Overlapped = WS_OVERLAPPED.0,
    OverlappedWindow = WS_OVERLAPPEDWINDOW.0,
    Visible = WS_VISIBLE.0,
    #[num_enum(catch_all)]
    Other(u32),
}

impl Default for WindowStyle {
    fn default() -> Self {
        Self::Overlapped
    }
}

impl From<WindowStyle> for WINDOW_STYLE {
    fn from(value: WindowStyle) -> Self {
        WINDOW_STYLE(value.into())
    }
}

/// Extended window style.
///
/// Using combinations is possible with [`std::ops::BitOr`].
///
/// See also: [Microsoft docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
#[derive(IntoPrimitive, TryFromPrimitive, BitOr, Copy, Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
#[repr(u32)]
pub enum WindowExtendedStyle {
    Layered = WS_EX_LAYERED.0,
    Left = WS_EX_LEFT.0,
    Topmost = WS_EX_TOPMOST.0,
    Transparent = WS_EX_TRANSPARENT.0,
    #[num_enum(catch_all)]
    Other(u32),
}

impl Default for WindowExtendedStyle {
    fn default() -> Self {
        Self::Left
    }
}

impl From<WindowExtendedStyle> for WINDOW_EX_STYLE {
    fn from(value: WindowExtendedStyle) -> Self {
        WINDOW_EX_STYLE(value.into())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Default, Debug)]
pub struct WindowAppearance {
    pub style: WindowStyle,
    pub extended_style: WindowExtendedStyle,
}

/// Window show state such as 'minimized' or 'hidden'.
///
/// Changing this state for a window can be done with [`WindowHandle::set_show_state`].
///
/// [`WindowHandle::get_placement`] and [`WindowPlacement::get_show_state`] can be used to read the state.
#[derive(IntoPrimitive, TryFromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(i32)]
pub enum WindowShowState {
    Hide = SW_HIDE.0,
    Maximize = SW_MAXIMIZE.0,
    Minimize = SW_MINIMIZE.0,
    Restore = SW_RESTORE.0,
    Show = SW_SHOW.0,
    ShowMinimized = SW_SHOWMINIMIZED.0,
    ShowMinNoActivate = SW_SHOWMINNOACTIVE.0,
    ShowNoActivate = SW_SHOWNA.0,
    ShowNormalNoActivate = SW_SHOWNOACTIVATE.0,
    ShowNormal = SW_SHOWNORMAL.0,
}

impl From<WindowShowState> for SHOW_WINDOW_CMD {
    fn from(value: WindowShowState) -> Self {
        SHOW_WINDOW_CMD(value.into())
    }
}

/// Window show state plus positions.
#[derive(Copy, Clone, Debug)]
pub struct WindowPlacement {
    raw_placement: WINDOWPLACEMENT,
}

impl WindowPlacement {
    pub fn get_show_state(&self) -> Option<WindowShowState> {
        i32::try_from(self.raw_placement.showCmd)
            .ok()?
            .try_into()
            .ok()
    }

    pub fn set_show_state(&mut self, state: WindowShowState) {
        self.raw_placement.showCmd = i32::from(state)
            .try_into()
            .unwrap_or_else(|_| unreachable!());
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

/// Window command corresponding to its buttons in the top right corner.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
#[repr(u32)]
pub enum WindowCommand {
    Close = SC_CLOSE,
    Maximize = SC_MAXIMIZE,
    Minimize = SC_MINIMIZE,
    Restore = SC_RESTORE,
}

impl WindowCommand {
    fn to_usize(self) -> usize {
        usize::try_from(u32::from(self)).unwrap()
    }
}

/// The target of the flash animation.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(u32)]
pub enum FlashElement {
    Caption = FLASHW_CAPTION.0,
    Taskbar = FLASHW_TRAY.0,
    #[default]
    CaptionPlusTaskbar = FLASHW_ALL.0,
}

impl FlashElement {
    fn to_flashwinfo_flags(self) -> FLASHWINFO_FLAGS {
        FLASHWINFO_FLAGS(u32::from(self))
    }
}

/// The amount of times the window should be flashed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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

/// The interval between flashes.
#[derive(Copy, Clone, Eq, PartialEq, Default, Debug)]
pub enum FlashInterval {
    #[default]
    DefaultCursorBlinkInterval,
    Milliseconds(u32),
}

/// Monitor power state.
///
/// Can be set using [`WindowHandle::set_monitor_power`].
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(isize)]
pub enum MonitorPower {
    #[default]
    On = -1,
    Low = 1,
    Off = 2,
}

/// An icon in the Windows notification area.
///
/// This icon is always associated with a window and can be used in conjunction with [`crate::ui::menu::PopupMenu`].
#[derive(Debug)]
pub struct NotificationIcon<'res, 'wnd, W: WindowKind + 'wnd> {
    id: NotificationIconId,
    window: &'wnd RefCell<W>,
    _phantom: PhantomData<&'res dyn Icon>,
}

impl<'res, 'wnd, W: WindowKind> NotificationIcon<'res, 'wnd, W> {
    /// Adds a notification icon.
    ///
    /// The window's [`WindowMessageListener`] will receive messages when user interactions with the icon occur.
    pub fn new<NI: Icon + 'res>(
        window: &'wnd RefCell<W>,
        options: NotificationIconOptions<NI>,
    ) -> io::Result<NotificationIcon<'res, 'wnd, W>> {
        // For GUID handling maybe look at generating it from the executable path:
        // https://stackoverflow.com/questions/7432319/notifyicondata-guid-problem
        let chosen_icon_handle = if let Some(icon) = options.icon {
            icon.as_handle()?
        } else {
            BuiltinIcon::default().as_handle()?
        };
        let call_data = get_notification_call_data(
            window.borrow().handle(),
            options.icon_id,
            true,
            Some(chosen_icon_handle),
            options.tooltip_text.as_deref(),
            Some(!options.visible),
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &call_data)
                .if_null_to_error_else_drop(|| io::Error::other("Cannot add notification icon"))?;
            Shell_NotifyIconW(NIM_SETVERSION, &call_data).if_null_to_error_else_drop(|| {
                io::Error::other("Cannot set notification version")
            })?;
        };
        Ok(NotificationIcon {
            id: options.icon_id,
            window,
            _phantom: PhantomData,
        })
    }

    /// Sets the icon graphics.
    pub fn set_icon(&mut self, icon: &'res impl Icon) -> io::Result<()> {
        let call_data = get_notification_call_data(
            self.window.borrow().handle(),
            self.id,
            false,
            Some(icon.as_handle()?),
            None,
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &call_data)
                .if_null_to_error_else_drop(|| io::Error::other("Cannot set notification icon"))?;
        };
        Ok(())
    }

    /// Allows showing or hiding the icon in the notification area.
    pub fn set_icon_hidden_state(&mut self, hidden: bool) -> io::Result<()> {
        let call_data = get_notification_call_data(
            self.window.borrow().handle(),
            self.id,
            false,
            None,
            None,
            Some(hidden),
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &call_data).if_null_to_error_else_drop(|| {
                io::Error::other("Cannot set notification icon hidden state")
            })?;
        };
        Ok(())
    }

    /// Sets the tooltip text when hovering over the icon with the mouse.
    pub fn set_tooltip_text(&mut self, text: &str) -> io::Result<()> {
        let call_data = get_notification_call_data(
            self.window.borrow().handle(),
            self.id,
            false,
            None,
            Some(text),
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &call_data).if_null_to_error_else_drop(|| {
                io::Error::other("Cannot set notification icon tooltip text")
            })?;
        };
        Ok(())
    }

    /// Triggers a balloon notification above the notification icon.
    pub fn set_balloon_notification(
        &mut self,
        notification: Option<BalloonNotification>,
    ) -> io::Result<()> {
        let call_data = get_notification_call_data(
            self.window.borrow().handle(),
            self.id,
            false,
            None,
            None,
            None,
            Some(notification),
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &call_data).if_null_to_error_else_drop(|| {
                io::Error::other("Cannot set notification icon balloon text")
            })?;
        };
        Ok(())
    }
}

impl<W: WindowKind> Drop for NotificationIcon<'_, '_, W> {
    fn drop(&mut self) {
        let call_data = get_notification_call_data(
            self.window.borrow().handle(),
            self.id,
            false,
            None,
            None,
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &call_data)
                .if_null_to_error_else_drop(|| io::Error::other("Cannot remove notification icon"))
                .unwrap();
        }
    }
}

#[allow(clippy::option_option)]
fn get_notification_call_data(
    window_handle: WindowHandle,
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
        hWnd: window_handle.into(),
        ..Default::default()
    };
    icon_data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    match icon_id {
        NotificationIconId::GUID(id) => {
            icon_data.guidItem = id;
            icon_data.uFlags |= NIF_GUID;
        }
        NotificationIconId::Simple(simple_id) => icon_data.uID = simple_id.into(),
    }
    if set_callback_message {
        icon_data.uCallbackMessage = super::messaging::RawMessage::ID_NOTIFICATION_ICON_MSG;
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
            icon_data.dwState = NOTIFY_ICON_STATE(icon_data.dwState.0 | NIS_HIDDEN.0);
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
            icon_data.dwInfoFlags =
                NOTIFY_ICON_INFOTIP_FLAGS(icon_data.dwInfoFlags.0 | u32::from(balloon.icon));
        }
        icon_data.uFlags |= NIF_INFO;
    }
    icon_data
}

/// Notification icon ID given to the Windows API.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum NotificationIconId {
    /// A simple ID.
    Simple(u16),
    /// A GUID that allows Windows to track the icon between applidation restarts.
    ///
    /// This way the user can set preferences for icon visibility and position.
    GUID(GUID),
}

impl Default for NotificationIconId {
    fn default() -> Self {
        NotificationIconId::Simple(0)
    }
}

/// Options for a new notification icon used by [`Window::add_notification_icon`].
#[derive(Eq, PartialEq, Default, Debug)]
pub struct NotificationIconOptions<I> {
    pub icon_id: NotificationIconId,
    pub icon: Option<I>,
    pub tooltip_text: Option<String>,
    pub visible: bool,
}

/// A Balloon notification above the Windows notification area.
///
/// Used with [`NotificationIcon::set_balloon_notification`].
#[derive(Copy, Clone, Default, Debug)]
pub struct BalloonNotification<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub icon: BalloonNotificationStandardIcon,
}

/// Built-in Windows standard icons for balloon notifications.
#[derive(IntoPrimitive, Copy, Clone, Default, Debug)]
#[repr(u32)]
pub enum BalloonNotificationStandardIcon {
    #[default]
    None = NIIF_NONE.0,
    Info = NIIF_INFO.0,
    Warning = NIIF_WARNING.0,
    Error = NIIF_ERROR.0,
}

#[cfg(test)]
mod tests {
    use more_asserts::*;

    use super::*;

    #[test]
    fn run_window_tests_without_parallelism() -> io::Result<()> {
        check_toplevel_windows()?;
        new_window_with_class()?;
        Ok(())
    }

    fn check_toplevel_windows() -> io::Result<()> {
        let all_windows = WindowHandle::get_toplevel_windows()?;
        assert_gt!(all_windows.len(), 0);
        for window in all_windows {
            assert!(window.is_window());
            assert!(window.get_placement().is_ok());
            assert!(window.get_class_name().is_ok());
            std::hint::black_box(&window.get_caption_text());
            #[cfg(feature = "process")]
            std::hint::black_box(&window.get_creator_thread_process_ids());
        }
        Ok(())
    }

    fn new_window_with_class() -> io::Result<()> {
        const CLASS_NAME_PREFIX: &str = "myclass1";
        const WINDOW_NAME: &str = "mywindow1";
        const CAPTION_TEXT: &str = "Testwindow";

        let listener = |_| Default::default();
        let icon: BuiltinIcon = Default::default();
        let class: WindowClass = WindowClass::register_new(
            CLASS_NAME_PREFIX,
            WindowClassAppearance {
                icon: Some(icon),
                ..Default::default()
            },
        )?;
        let window: RefCell<_> = Window::create_new(
            &class,
            listener,
            WINDOW_NAME,
            WindowAppearance::default(),
            None,
        )?
        .into();
        let notification_icon_options = NotificationIconOptions {
            icon: Some(icon),
            tooltip_text: Some("A tooltip!".to_string()),
            visible: false,
            ..Default::default()
        };
        let mut notification_icon = NotificationIcon::new(&window, notification_icon_options)?;
        let balloon_notification = BalloonNotification::default();
        notification_icon.set_balloon_notification(Some(balloon_notification))?;

        let window_handle = *window.borrow().as_ref();
        assert_eq!(window_handle.get_caption_text(), WINDOW_NAME);
        window_handle.set_caption_text(CAPTION_TEXT)?;
        assert_eq!(window_handle.get_caption_text(), CAPTION_TEXT);
        assert!(dbg!(window_handle.get_class_name()?).starts_with(CLASS_NAME_PREFIX));
        assert!(window_handle.get_client_area_coords()?.left >= 0);

        Ok(())
    }
}
