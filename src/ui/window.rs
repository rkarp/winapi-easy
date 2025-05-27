//! UI components related to windows.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{
    Display,
    Formatter,
};
use std::marker::PhantomData;
use std::ops::{
    BitOr,
    Deref,
};
use std::ptr::NonNull;
use std::rc::Rc;
use std::{
    io,
    mem,
    ptr,
    vec,
};

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
use windows::Win32::Graphics::Gdi::{
    GetWindowRgn,
    InvalidateRect,
    MapWindowPoints,
    RGN_ERROR,
    SetWindowRgn,
};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetActiveWindow;
use windows::Win32::UI::Magnification::{
    MAGTRANSFORM,
    MS_SHOWMAGNIFIEDCURSOR,
    MagSetWindowSource,
    MagSetWindowTransform,
    WC_MAGNIFIER,
};
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
    HWND_BOTTOM,
    HWND_NOTOPMOST,
    HWND_TOP,
    HWND_TOPMOST,
    IsWindow,
    IsWindowVisible,
    KillTimer,
    LWA_ALPHA,
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
    SWP_NOSIZE,
    SendMessageW,
    SetForegroundWindow,
    SetLayeredWindowAttributes,
    SetTimer,
    SetWindowLongPtrW,
    SetWindowPlacement,
    SetWindowPos,
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
    WS_EX_COMPOSITED,
    WS_EX_LAYERED,
    WS_EX_LEFT,
    WS_EX_NOACTIVATE,
    WS_EX_TOPMOST,
    WS_EX_TRANSPARENT,
    WS_OVERLAPPED,
    WS_OVERLAPPEDWINDOW,
    WS_POPUP,
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
    Region,
    init_magnifier,
};
use crate::internal::{
    RawBox,
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
    CustomUserMessage,
    ListenerAnswer,
    ListenerMessage,
    RawMessage,
    WmlOpaqueClosure,
    generic_window_proc,
};
use crate::ui::resource::{
    Brush,
    Cursor,
    Icon,
};

/// A (non-null) handle to a window.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct WindowHandle {
    raw_handle: HWND,
}

// See reasoning: https://docs.rs/hwnd0/0.0.0-2024-01-10/hwnd0/struct.HWND.html
unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

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
        Self { raw_handle: handle }
    }

    pub(crate) fn from_maybe_null(handle: HWND) -> Option<Self> {
        if handle.is_null() {
            None
        } else {
            Some(Self { raw_handle: handle })
        }
    }

    /// Checks if the handle points to an existing window.
    pub fn is_window(self) -> bool {
        let result = unsafe { IsWindow(Some(self.raw_handle)) };
        result.as_bool()
    }

    pub fn is_visible(self) -> bool {
        let result = unsafe { IsWindowVisible(self.raw_handle) };
        result.as_bool()
    }

    /// Returns the window caption text, converted to UTF-8 in a potentially lossy way.
    pub fn get_caption_text(self) -> String {
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
    pub fn set_caption_text(self, text: &str) -> io::Result<()> {
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
    ///
    /// May interfere with the Z-position of other windows created by this process.
    pub fn set_as_foreground(self) -> io::Result<()> {
        unsafe {
            SetForegroundWindow(self.raw_handle).if_null_to_error_else_drop(|| {
                io::Error::other("Cannot bring window to foreground")
            })?;
        }
        Ok(())
    }

    /// Sets the window as the currently active (selected) window.
    pub fn set_as_active(self) -> io::Result<()> {
        unsafe {
            SetActiveWindow(self.raw_handle)?;
        }
        Ok(())
    }

    /// Changes the window show state.
    pub fn set_show_state(self, state: WindowShowState) -> io::Result<()> {
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
    pub fn get_placement(self) -> io::Result<WindowPlacement> {
        let mut raw_placement: WINDOWPLACEMENT = WINDOWPLACEMENT {
            length: mem::size_of::<WINDOWPLACEMENT>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            ..Default::default()
        };
        unsafe { GetWindowPlacement(self.raw_handle, &raw mut raw_placement)? };
        Ok(WindowPlacement { raw_placement })
    }

    /// Sets the window's show state and positions.
    pub fn set_placement(self, placement: &WindowPlacement) -> io::Result<()> {
        unsafe { SetWindowPlacement(self.raw_handle, &raw const placement.raw_placement)? };
        Ok(())
    }

    pub fn modify_placement_with<F>(self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut WindowPlacement) -> io::Result<()>,
    {
        let mut placement = self.get_placement()?;
        f(&mut placement)?;
        self.set_placement(&placement)?;
        Ok(())
    }

    pub fn set_z_position(self, z_position: WindowZPosition) -> io::Result<()> {
        unsafe {
            SetWindowPos(
                self.raw_handle,
                Some(z_position.to_raw_hwnd()),
                0,
                0,
                0,
                0,
                SWP_NOSIZE,
            )?;
        }
        Ok(())
    }

    /// Returns the window's client area rectangle relative to the screen.
    pub fn get_client_area_coords(self) -> io::Result<Rectangle> {
        let mut result_rect: Rectangle = Default::default();
        unsafe { GetClientRect(self.raw_handle, &raw mut result_rect) }?;
        self.map_points(None, result_rect.as_point_array_mut())?;
        Ok(result_rect)
    }

    pub(crate) fn map_points(
        self,
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

    pub fn get_region(self) -> Option<Region> {
        let region = Region::from_rect(Default::default());
        let result = unsafe { GetWindowRgn(self.raw_handle, region.raw_handle) };
        if result == RGN_ERROR {
            None
        } else {
            Some(region)
        }
    }

    /// Sets the window's interaction region.
    ///
    /// Will potentially remove visual styles from the window.
    pub fn set_region(self, region: Region) -> io::Result<()> {
        unsafe {
            SetWindowRgn(self.raw_handle, Some(region.into()), true)
                .if_null_get_last_error_else_drop()
        }
    }

    pub fn redraw(self) -> io::Result<()> {
        unsafe {
            InvalidateRect(Some(self.raw_handle), None, true).if_null_get_last_error_else_drop()
        }
    }

    /// Returns the class name of the window's associated [`WindowClass`].
    pub fn get_class_name(self) -> io::Result<String> {
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
    pub fn send_command(self, action: WindowCommand) -> io::Result<()> {
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
    pub fn flash(self) {
        self.flash_custom(Default::default(), Default::default(), Default::default());
    }

    /// Flashes the window, allowing various customization parameters.
    pub fn flash_custom(
        self,
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
            let _ = FlashWindowEx(&raw const raw_config);
        };
    }

    /// Stops the window from flashing.
    pub fn flash_stop(self) {
        let raw_config = FLASHWINFO {
            cbSize: mem::size_of::<FLASHWINFO>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            hwnd: self.into(),
            dwFlags: FLASHW_STOP,
            ..Default::default()
        };
        unsafe {
            let _ = FlashWindowEx(&raw const raw_config);
        };
    }

    fn internal_set_layered_opacity_alpha(self, alpha: u8) -> io::Result<()> {
        unsafe {
            SetLayeredWindowAttributes(self.raw_handle, Default::default(), alpha, LWA_ALPHA)?;
        }
        Ok(())
    }

    pub fn set_timer(self, timer_id: usize, interval_ms: u32) -> io::Result<()> {
        unsafe {
            SetTimer(Some(self.raw_handle), timer_id, interval_ms, None)
                .if_null_get_last_error_else_drop()
        }
    }

    pub fn kill_timer(self, timer_id: usize) -> io::Result<()> {
        unsafe { KillTimer(Some(self.raw_handle), timer_id)? }
        Ok(())
    }

    pub fn send_user_message(self, message: CustomUserMessage) -> io::Result<()> {
        RawMessage::from(message).post_to_queue(Some(self))
    }

    /// Returns the thread ID that created this window.
    #[cfg(feature = "process")]
    pub fn get_creator_thread_id(self) -> ThreadId {
        self.get_creator_thread_process_ids().0
    }

    /// Returns the process ID that created this window.
    #[cfg(feature = "process")]
    pub fn get_creator_process_id(self) -> ProcessId {
        self.get_creator_thread_process_ids().1
    }

    #[cfg(feature = "process")]
    fn get_creator_thread_process_ids(self) -> (ThreadId, ProcessId) {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
        let mut process_id: u32 = 0;
        let thread_id =
            unsafe { GetWindowThreadProcessId(self.raw_handle, Some(&raw mut process_id)) };
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
    pub fn set_monitor_power(self, level: MonitorPower) -> io::Result<()> {
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

    pub(crate) unsafe fn get_user_data_ptr<T>(self) -> Option<NonNull<T>> {
        let ptr_value = unsafe { GetWindowLongPtrW(self.raw_handle, GWLP_USERDATA) };
        NonNull::new(ptr::with_exposed_provenance_mut(ptr_value.cast_unsigned()))
    }

    pub(crate) unsafe fn set_user_data_ptr<T>(self, ptr: *const T) -> io::Result<()> {
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

#[derive(Debug)]
enum WindowClassVariant {
    Builtin(PCWSTR),
    Custom(Rc<WindowClass>),
}

impl WindowClassVariant {
    fn raw_class_identifier(&self) -> PCWSTR {
        match self {
            WindowClassVariant::Builtin(pcwstr) => *pcwstr,
            WindowClassVariant::Custom(window_class) => window_class.raw_class_identifier(),
        }
    }
}

/// Window class serving as a base for [`Window`].
#[derive(Debug)]
pub struct WindowClass {
    atom: u16,
    #[expect(dead_code)]
    appearance: WindowClassAppearance,
}

impl WindowClass {
    const MAX_WINDOW_CLASS_NAME_CHARS: usize = 256;

    fn raw_class_identifier(&self) -> PCWSTR {
        PCWSTR(self.atom as *const u16)
    }
}

impl WindowClass {
    /// Registers a new class.
    ///
    /// This class can then be used to create instances of [`Window`].
    ///
    /// The class name will be generated from the given prefix by adding a random base64 encoded UUID
    /// to ensure uniqueness. This means that the maximum length of the class name prefix is a little less
    /// than the standard 256 characters for class names.
    pub fn register_new(
        class_name_prefix: &str,
        appearance: WindowClassAppearance,
    ) -> io::Result<Self> {
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;

        let base64_uuid = URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes());
        let class_name = class_name_prefix.to_string() + "_" + &base64_uuid;

        let icon_handle = appearance
            .icon
            .as_deref()
            .map_or_else(Default::default, Icon::as_handle);
        // No need to reserve extra window memory if we only need a single pointer
        let class_def = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            lpfnWndProc: Some(generic_window_proc),
            hIcon: icon_handle,
            hCursor: appearance
                .cursor
                .as_deref()
                .map_or_else(Default::default, Cursor::as_handle),
            hbrBackground: appearance
                .background_brush
                .as_deref()
                .map_or_else(Default::default, Brush::as_handle),
            lpszClassName: ZeroTerminatedWideString::from_os_str(class_name).as_raw_pcwstr(),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&raw const class_def).if_null_get_last_error()? };
        Ok(WindowClass { atom, appearance })
    }
}

impl Drop for WindowClass {
    /// Unregisters the class on drop.
    fn drop(&mut self) {
        unsafe {
            UnregisterClassW(self.raw_class_identifier(), None).unwrap();
        }
    }
}

#[derive(Clone, Debug)]
pub struct WindowClassAppearance {
    pub background_brush: Option<Rc<Brush>>,
    pub icon: Option<Rc<Icon>>,
    pub cursor: Option<Rc<Cursor>>,
}

impl WindowClassAppearance {
    pub fn empty() -> Self {
        Self {
            background_brush: None,
            icon: None,
            cursor: None,
        }
    }
}

impl Default for WindowClassAppearance {
    fn default() -> Self {
        Self {
            background_brush: Some(Default::default()),
            icon: Some(Default::default()),
            cursor: Some(Default::default()),
        }
    }
}

pub type DefaultWmlType = fn(&ListenerMessage) -> ListenerAnswer;

pub trait WindowSubtype: 'static {}

impl WindowSubtype for () {}

pub enum Layered {}

impl WindowSubtype for Layered {}

pub enum Magnifier {}

impl WindowSubtype for Magnifier {}

/// A window based on a [`WindowClass`].
///
/// # Multithreading
///
/// `Window` is not [`Send`] because the window procedure and window destruction calls
/// must only be called from the creating thread.
pub struct Window<WST = ()> {
    handle: WindowHandle,
    #[expect(dead_code)]
    class: WindowClassVariant,
    #[expect(dead_code)]
    opaque_listener: Option<RawBox<WmlOpaqueClosure<'static>>>,
    #[expect(dead_code)]
    parent: Option<Rc<dyn Any>>,
    notification_icons: HashMap<NotificationIconId, NotificationIcon>,
    phantom: PhantomData<WST>,
}

#[cfg(test)]
static_assertions::assert_not_impl_any!(Window: Send);

impl<WST: WindowSubtype> Window<WST> {
    fn internal_new<WML, PST>(
        class: WindowClassVariant,
        listener: Option<WML>,
        caption_text: &str,
        appearance: WindowAppearance,
        parent: Option<Rc<RefCell<Window<PST>>>>,
    ) -> io::Result<Self>
    where
        WML: FnMut(&ListenerMessage) -> ListenerAnswer + 'static,
        PST: WindowSubtype,
    {
        let h_wnd: HWND = unsafe {
            CreateWindowExW(
                appearance.extended_style.into(),
                class.raw_class_identifier(),
                ZeroTerminatedWideString::from_os_str(caption_text).as_raw_pcwstr(),
                appearance.style.into(),
                CW_USEDEFAULT,
                0,
                CW_USEDEFAULT,
                0,
                parent.as_deref().map(|x| x.borrow().raw_handle),
                None,
                None,
                None,
            )?
        };
        let handle = WindowHandle::from_non_null(h_wnd);

        let opaque_listener = if let Some(listener) = listener {
            let opaque_listener = unsafe { Self::set_listener_internal(handle, listener) }?;
            Some(opaque_listener)
        } else {
            None
        };
        Ok(Window {
            handle,
            class,
            opaque_listener,
            parent: parent.map(|x| x as Rc<dyn Any>),
            notification_icons: HashMap::new(),
            phantom: PhantomData,
        })
    }

    pub fn as_handle(&self) -> WindowHandle {
        self.handle
    }

    /// Changes the [`WindowMessageListener`].
    pub fn set_listener<WML>(&mut self, listener: WML) -> io::Result<()>
    where
        WML: FnMut(&ListenerMessage) -> ListenerAnswer + 'static,
    {
        unsafe { Self::set_listener_internal(self.handle, listener) }?;
        Ok(())
    }

    /// Internally sets the listener
    ///
    /// # Safety
    ///
    /// The returned value must not be dropped while the window callback may still be active.
    unsafe fn set_listener_internal<WML>(
        window_handle: WindowHandle,
        listener: WML,
    ) -> io::Result<RawBox<WmlOpaqueClosure<'static>>>
    where
        WML: FnMut(&ListenerMessage) -> ListenerAnswer + 'static,
    {
        let mut opaque_listener = RawBox::new(Box::new(listener) as WmlOpaqueClosure);
        unsafe {
            window_handle.set_user_data_ptr::<WmlOpaqueClosure>(opaque_listener.as_mut_ptr())?;
        }
        Ok(opaque_listener)
    }

    /// Adds a new notification icon.
    ///
    /// # Panics
    ///
    /// Will panic if the notification icon ID already exists.
    pub fn add_notification_icon(
        &mut self,
        options: NotificationIconOptions,
    ) -> io::Result<&mut NotificationIcon> {
        let id = options.icon_id;
        assert!(
            !self.notification_icons.contains_key(&id),
            "Notification icon ID already exists"
        );
        self.notification_icons
            .insert(id, NotificationIcon::new(self.handle, options)?);
        Ok(self.get_notification_icon(id))
    }

    /// Returns a reference to a previously added notification icon.
    ///
    /// # Panics
    ///
    /// Will panic if the ID doesn't exist.
    pub fn get_notification_icon(&mut self, id: NotificationIconId) -> &mut NotificationIcon {
        self.notification_icons
            .get_mut(&id)
            .expect("Notification icon ID doesn't exist")
    }

    /// Removes a notification icon.
    ///
    /// # Panics
    ///
    /// Will panic if the ID doesn't exist.
    pub fn remove_notification_icon(&mut self, id: NotificationIconId) {
        let _ = self
            .notification_icons
            .remove(&id)
            .expect("Notification icon ID doesn't exist");
    }
}

impl Window<()> {
    /// Creates a new window.
    ///
    /// User interaction with the window will result in messages sent to the window message listener provided here.
    ///
    /// # Generics
    ///
    /// Note that you can use [`DefaultWmlType`] for the `WML` type parameter when not providing a listener.
    pub fn new<WML, PST>(
        class: Rc<WindowClass>,
        listener: Option<WML>,
        caption_text: &str,
        appearance: WindowAppearance,
        parent: Option<Rc<RefCell<Window<PST>>>>,
    ) -> io::Result<Self>
    where
        WML: FnMut(&ListenerMessage) -> ListenerAnswer + 'static,
        PST: WindowSubtype,
    {
        let class = WindowClassVariant::Custom(class);
        Self::internal_new(class, listener, caption_text, appearance, parent)
    }
}

impl Window<Layered> {
    /// Creates a new layered window.
    ///
    /// This is analogous to [`Window::new`].
    pub fn new_layered<WML, PST>(
        class: Rc<WindowClass>,
        listener: Option<WML>,
        caption_text: &str,
        mut appearance: WindowAppearance,
        parent: Option<Rc<RefCell<Window<PST>>>>,
    ) -> io::Result<Self>
    where
        WML: FnMut(&ListenerMessage) -> ListenerAnswer + 'static,
        PST: WindowSubtype,
    {
        appearance.extended_style =
            appearance.extended_style | WindowExtendedStyle::Other(WS_EX_LAYERED.0);
        let class = WindowClassVariant::Custom(class);
        Self::internal_new(class, listener, caption_text, appearance, parent)
    }

    /// Sets the opacity value for a layered window.
    pub fn set_layered_opacity_alpha(&self, alpha: u8) -> io::Result<()> {
        self.handle.internal_set_layered_opacity_alpha(alpha)
    }
}

impl Window<Magnifier> {
    pub fn new_magnifier(
        caption_text: &str,
        mut appearance: WindowAppearance,
        parent: Rc<RefCell<Window<Layered>>>,
    ) -> io::Result<Self> {
        init_magnifier()?;
        appearance.style =
            appearance.style | WindowStyle::Other(MS_SHOWMAGNIFIEDCURSOR.cast_unsigned());
        let class = WindowClassVariant::Builtin(WC_MAGNIFIER);
        Self::internal_new(
            class,
            None::<DefaultWmlType>,
            caption_text,
            appearance,
            Some(parent),
        )
    }

    pub fn set_magnification_factor(&self, mag_factor: f32) -> io::Result<()> {
        const NUM_COLS: usize = 3;
        fn multi_index(matrix: &mut [f32], row: usize, col: usize) -> &mut f32 {
            &mut matrix[row * NUM_COLS + col]
        }
        let mut matrix: MAGTRANSFORM = Default::default();
        *multi_index(&mut matrix.v, 0, 0) = mag_factor;
        *multi_index(&mut matrix.v, 1, 1) = mag_factor;
        *multi_index(&mut matrix.v, 2, 2) = 1.0;
        unsafe {
            MagSetWindowTransform(self.raw_handle, &raw mut matrix)
                .if_null_get_last_error_else_drop()
        }
    }

    pub fn set_magnification_source(&self, source: Rectangle) -> io::Result<()> {
        let _ = unsafe { MagSetWindowSource(self.raw_handle, source).if_null_get_last_error()? };
        Ok(())
    }

    pub fn set_lens_use_bitmap_smoothing(&self, use_smoothing: bool) -> io::Result<()> {
        #[link(
            name = "magnification.dll",
            kind = "raw-dylib",
            modifiers = "+verbatim"
        )]
        unsafe extern "system" {
            fn MagSetLensUseBitmapSmoothing(h_wnd: HWND, use_smoothing: BOOL) -> BOOL;
        }
        unsafe {
            MagSetLensUseBitmapSmoothing(self.raw_handle, use_smoothing.into())
                .if_null_get_last_error_else_drop()
        }
    }
}

impl<WST> Deref for Window<WST> {
    type Target = WindowHandle;

    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl<WST> Drop for Window<WST> {
    fn drop(&mut self) {
        unsafe {
            if self.handle.is_window() {
                DestroyWindow(self.handle.raw_handle).unwrap();
            }
        }
    }
}

/// Window style.
///
/// Using combinations is possible with [`std::ops::BitOr`].
///
/// See also: [Microsoft docs](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-styles)
#[derive(IntoPrimitive, TryFromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
#[repr(u32)]
pub enum WindowStyle {
    Child = WS_CHILD.0,
    ClipChildren = WS_CLIPCHILDREN.0,
    Overlapped = WS_OVERLAPPED.0,
    OverlappedWindow = WS_OVERLAPPEDWINDOW.0,
    Popup = WS_POPUP.0,
    Visible = WS_VISIBLE.0,
    #[num_enum(catch_all)]
    Other(u32),
}

impl Default for WindowStyle {
    fn default() -> Self {
        Self::Overlapped
    }
}

impl BitOr for WindowStyle {
    type Output = WindowStyle;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::Other(u32::from(self) | u32::from(rhs))
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
#[derive(IntoPrimitive, TryFromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
#[repr(u32)]
pub enum WindowExtendedStyle {
    Composited = WS_EX_COMPOSITED.0,
    Left = WS_EX_LEFT.0,
    NoActivate = WS_EX_NOACTIVATE.0,
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

impl BitOr for WindowExtendedStyle {
    type Output = WindowExtendedStyle;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::Other(u32::from(self) | u32::from(rhs))
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

    pub fn get_normal_position(&self) -> Rectangle {
        self.raw_placement.rcNormalPosition
    }

    pub fn set_normal_position(&mut self, rectangle: Rectangle) {
        self.raw_placement.rcNormalPosition = rectangle;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowZPosition {
    Bottom,
    NoTopMost,
    Top,
    TopMost,
}

impl WindowZPosition {
    fn to_raw_hwnd(self) -> HWND {
        match self {
            WindowZPosition::Bottom => HWND_BOTTOM,
            WindowZPosition::NoTopMost => HWND_NOTOPMOST,
            WindowZPosition::Top => HWND_TOP,
            WindowZPosition::TopMost => HWND_TOPMOST,
        }
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
pub struct NotificationIcon {
    id: NotificationIconId,
    window: WindowHandle,
    icon: Rc<Icon>,
}

impl NotificationIcon {
    /// Adds a notification icon.
    ///
    /// The window's [`WindowMessageListener`] will receive messages when user interactions with the icon occur.
    fn new(window: WindowHandle, options: NotificationIconOptions) -> io::Result<Self> {
        // For GUID handling maybe look at generating it from the executable path:
        // https://stackoverflow.com/questions/7432319/notifyicondata-guid-problem
        let call_data = get_notification_call_data(
            window,
            options.icon_id,
            true,
            Some(options.icon.as_handle()),
            options.tooltip_text.as_deref(),
            Some(!options.visible),
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &raw const call_data)
                .if_null_to_error_else_drop(|| io::Error::other("Cannot add notification icon"))?;
            Shell_NotifyIconW(NIM_SETVERSION, &raw const call_data).if_null_to_error_else_drop(
                || io::Error::other("Cannot set notification version"),
            )?;
        };
        Ok(NotificationIcon {
            id: options.icon_id,
            window,
            icon: options.icon,
        })
    }

    /// Sets the icon graphics.
    pub fn set_icon(&mut self, icon: Rc<Icon>) -> io::Result<()> {
        let call_data = get_notification_call_data(
            self.window,
            self.id,
            false,
            Some(icon.as_handle()),
            None,
            None,
            None,
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &raw const call_data)
                .if_null_to_error_else_drop(|| io::Error::other("Cannot set notification icon"))?;
        };
        self.icon = icon;
        Ok(())
    }

    /// Allows showing or hiding the icon in the notification area.
    pub fn set_icon_hidden_state(&mut self, hidden: bool) -> io::Result<()> {
        let call_data =
            get_notification_call_data(self.window, self.id, false, None, None, Some(hidden), None);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &raw const call_data).if_null_to_error_else_drop(
                || io::Error::other("Cannot set notification icon hidden state"),
            )?;
        };
        Ok(())
    }

    /// Sets the tooltip text when hovering over the icon with the mouse.
    pub fn set_tooltip_text(&mut self, text: &str) -> io::Result<()> {
        let call_data =
            get_notification_call_data(self.window, self.id, false, None, Some(text), None, None);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &raw const call_data).if_null_to_error_else_drop(
                || io::Error::other("Cannot set notification icon tooltip text"),
            )?;
        };
        Ok(())
    }

    /// Triggers a balloon notification above the notification icon.
    pub fn set_balloon_notification(
        &mut self,
        notification: Option<BalloonNotification>,
    ) -> io::Result<()> {
        let call_data = get_notification_call_data(
            self.window,
            self.id,
            false,
            None,
            None,
            None,
            Some(notification),
        );
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &raw const call_data).if_null_to_error_else_drop(
                || io::Error::other("Cannot set notification icon balloon text"),
            )?;
        };
        Ok(())
    }
}

impl Drop for NotificationIcon {
    fn drop(&mut self) {
        let call_data =
            get_notification_call_data(self.window, self.id, false, None, None, None, None);
        // Ignore seemingly unavoidable random errors here
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &raw const call_data) };
    }
}

#[expect(clippy::option_option)]
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
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
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
pub struct NotificationIconOptions {
    pub icon_id: NotificationIconId,
    pub icon: Rc<Icon>,
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

        let icon: Rc<Icon> = Default::default();
        let class: WindowClass = WindowClass::register_new(
            CLASS_NAME_PREFIX,
            WindowClassAppearance {
                icon: Some(Rc::clone(&icon)),
                ..Default::default()
            },
        )?;
        let mut window = Window::new::<DefaultWmlType, ()>(
            class.into(),
            None,
            WINDOW_NAME,
            WindowAppearance::default(),
            None,
        )?;
        let notification_icon_options = NotificationIconOptions {
            icon,
            tooltip_text: Some("A tooltip!".to_string()),
            visible: false,
            ..Default::default()
        };
        let notification_icon = window.add_notification_icon(notification_icon_options)?;
        let balloon_notification = BalloonNotification::default();
        notification_icon.set_balloon_notification(Some(balloon_notification))?;

        let window_handle = window.as_handle();
        assert_eq!(window_handle.get_caption_text(), WINDOW_NAME);
        window_handle.set_caption_text(CAPTION_TEXT)?;
        assert_eq!(window_handle.get_caption_text(), CAPTION_TEXT);
        assert!(dbg!(window_handle.get_class_name()?).starts_with(CLASS_NAME_PREFIX));
        assert!(window_handle.get_client_area_coords()?.left >= 0);

        Ok(())
    }
}
