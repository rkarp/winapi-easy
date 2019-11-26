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
use winapi::ctypes::c_void;
use winapi::shared::basetsd::LONG_PTR;
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
    HBRUSH,
    HCURSOR,
    HWND,
    HWND__,
    POINT,
    RECT,
};
use winapi::shared::winerror::S_OK;
use winapi::um::consoleapi::AllocConsole;
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
    LoadImageW,
    LockWorkStation,
    RegisterClassExW,
    SendMessageW,
    SetWindowLongPtrW,
    SetWindowPlacement,
    UnregisterClassW,
    COLOR_3DDKSHADOW,
    COLOR_3DLIGHT,
    COLOR_ACTIVEBORDER,
    COLOR_ACTIVECAPTION,
    COLOR_APPWORKSPACE,
    COLOR_BACKGROUND,
    COLOR_BTNFACE,
    COLOR_BTNHIGHLIGHT,
    COLOR_BTNSHADOW,
    COLOR_BTNTEXT,
    COLOR_CAPTIONTEXT,
    COLOR_GRADIENTACTIVECAPTION,
    COLOR_GRADIENTINACTIVECAPTION,
    COLOR_GRAYTEXT,
    COLOR_HIGHLIGHT,
    COLOR_HIGHLIGHTTEXT,
    COLOR_HOTLIGHT,
    COLOR_INACTIVEBORDER,
    COLOR_INACTIVECAPTION,
    COLOR_INACTIVECAPTIONTEXT,
    COLOR_INFOBK,
    COLOR_INFOTEXT,
    COLOR_MENU,
    COLOR_MENUBAR,
    COLOR_MENUHILIGHT,
    COLOR_MENUTEXT,
    COLOR_SCROLLBAR,
    COLOR_WINDOW,
    COLOR_WINDOWFRAME,
    COLOR_WINDOWTEXT,
    CW_USEDEFAULT,
    FLASHWINFO,
    FLASHW_ALL,
    FLASHW_CAPTION,
    FLASHW_STOP,
    FLASHW_TIMER,
    FLASHW_TIMERNOFG,
    FLASHW_TRAY,
    GWLP_USERDATA,
    IMAGE_CURSOR,
    LR_DEFAULTSIZE,
    LR_SHARED,
    MAKEINTRESOURCEW,
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
    FromWideString,
    ToWideString,
};
use crate::ui::message::{
    generic_window_proc,
    WindowMessageListener,
};

pub mod message;

const MAX_WINDOW_CLASS_NAME_CHARS: usize = 256;

/// A (non-null) handle to a window.
///
/// **Note**: If the window was not created by this thread, then it is not guaranteed that
/// the handle continues pointing to the same window because the underlying handles
/// can get invalid or even recycled.
///
/// Implements neither `Copy` nor `Clone` to avoid concurrent mutable access to the same handle.
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

    pub fn set_placement(&mut self, placement: &WindowPlacement) -> io::Result<()> {
        unsafe {
            SetWindowPlacement(self.as_mutable_ptr(), &placement.raw_placement)
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

    pub fn perform_action(&mut self, action: WindowAction) -> io::Result<()> {
        let result =
            unsafe { SendMessageW(self.as_mutable_ptr(), WM_SYSCOMMAND, action.into(), 0) };
        result.if_non_null_to_error(|| custom_err_with_code("Cannot perform window action", result))
    }

    #[inline(always)]
    pub fn flash(&mut self) {
        self.flash_custom(Default::default(), Default::default(), Default::default())
    }

    pub fn flash_custom(
        &mut self,
        element: FlashElement,
        duration: FlashDuration,
        frequency: FlashFrequency,
    ) {
        let mut raw_config: FLASHWINFO = Default::default();
        raw_config.cbSize = mem::size_of::<FLASHWINFO>() as UINT;
        raw_config.hwnd = self.as_mutable_ptr();
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

    pub fn flash_stop(&mut self) {
        let mut raw_config: FLASHWINFO = Default::default();
        raw_config.cbSize = mem::size_of::<FLASHWINFO>() as UINT;
        raw_config.hwnd = self.as_mutable_ptr();
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

    pub fn set_monitor_power(&mut self, level: MonitorPower) -> io::Result<()> {
        let result = unsafe {
            SendMessageW(
                self.as_mutable_ptr(),
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

    pub(crate) unsafe fn set_user_data_ptr<T>(&mut self, reference: &mut T) -> io::Result<()> {
        SetWindowLongPtrW(
            self.as_mutable_ptr(),
            GWLP_USERDATA,
            reference as *const T as LONG_PTR,
        );
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

pub struct WindowClass<WML> {
    atom: ATOM,
    // These potentially need to be dropped properly
    _cursor: Cursor,
    _brush: Brush,
    phantom: PhantomData<WML>,
}

impl<WML: WindowMessageListener> WindowClass<WML> {
    pub fn register_new(
        class_name: &str,
        background_brush: Brush,
        cursor: Cursor,
    ) -> io::Result<Self> {
        let class_name_wide = class_name.to_wide_string();

        // No need to reserve extra window memory if we only need a single pointer
        let class_def: WNDCLASSEXW = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as UINT,
            lpfnWndProc: Some(generic_window_proc::<WML>),
            hCursor: cursor.as_handle()?,
            hbrBackground: background_brush.as_handle(),
            lpszClassName: class_name_wide.as_ptr(),
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&class_def).if_null_get_last_error()? };
        Ok(WindowClass {
            atom,
            _cursor: cursor,
            _brush: background_brush,
            phantom: PhantomData,
        })
    }
}

impl<WML> Drop for WindowClass<WML> {
    fn drop(&mut self) {
        unsafe {
            UnregisterClassW(self.atom as *const WCHAR, ptr::null_mut())
                .if_null_get_last_error()
                .unwrap();
        }
    }
}

pub struct Window<'class, 'listener, WML> {
    #[allow(unused)]
    class: &'class WindowClass<WML>,
    handle: WindowHandle,
    phantom: PhantomData<&'listener mut WML>,
}

impl<'class, 'listener, WML: WindowMessageListener> Window<'class, 'listener, WML> {
    pub fn create_new(
        class: &'class WindowClass<WML>,
        listener: &'listener mut WML,
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
}

impl<'a, 'b, WML> Drop for Window<'a, 'b, WML> {
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

impl<'a, 'b, WML> AsRef<WindowHandle> for Window<'a, 'b, WML> {
    fn as_ref(&self) -> &WindowHandle {
        &self.handle
    }
}

impl<'a, 'b, WML> AsMut<WindowHandle> for Window<'a, 'b, WML> {
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

pub type Point = POINT;
pub type Rectangle = RECT;

#[derive(Copy, Clone)]
pub struct WindowPlacement {
    raw_placement: WINDOWPLACEMENT,
}

impl WindowPlacement {
    pub fn get_show_state(&self) -> Option<WindowShowState> {
        (self.raw_placement.showCmd as i32).try_into().ok()
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
#[repr(usize)]
pub enum WindowAction {
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

/// Taskbar progress state animation type.
#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ProgressState {
    /// Stops displaying progress and returns the button to its normal state.
    NoProgress = TBPF_NOPROGRESS,
    /// Shows a "working" animation without indicating a completion percentage.
    ///
    /// Needs animations on the taskbar to be enabled in the OS config,
    /// otherwise it will not show anything to the user.
    Indeterminate = TBPF_INDETERMINATE,
    /// Shows a progress indicator displaying the amount of work being completed.
    Normal = TBPF_NORMAL,
    /// The progress indicator turns red to show that an error has occurred. This is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a red determinate display
    /// of a generic percentage not indicative of actual progress.
    Error = TBPF_ERROR,
    /// The progress indicator turns yellow to show that progress is currently stopped. his is a determinate state.
    /// If the progress indicator is in the indeterminate state, it switches to a yellow determinate display
    /// of a generic percentage not indicative of actual progress.
    Paused = TBPF_PAUSED,
}

impl Default for ProgressState {
    fn default() -> Self {
        ProgressState::NoProgress
    }
}

#[derive(Clone, Debug)]
pub struct Cursor {
    builtin_type: BuiltinCursor,
}

impl Cursor {
    pub(crate) fn as_handle(&self) -> io::Result<HCURSOR> {
        let default_cursor: NonNull<c_void> = unsafe {
            LoadImageW(
                ptr::null_mut(),
                MAKEINTRESOURCEW(self.builtin_type.into()),
                IMAGE_CURSOR,
                0,
                0,
                LR_SHARED | LR_DEFAULTSIZE,
            )
            .to_non_null_else_get_last_error()?
        };
        Ok(default_cursor.as_ptr() as HCURSOR)
    }
}

impl From<BuiltinCursor> for Cursor {
    fn from(builtin_type: BuiltinCursor) -> Self {
        Self { builtin_type }
    }
}

impl Default for Cursor {
    #[inline]
    fn default() -> Self {
        BuiltinCursor::default().into()
    }
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u16)]
pub enum BuiltinCursor {
    /// Standard arrow
    Normal = Self::OCR_NORMAL,
    /// Standard arrow and small hourglass
    NormalPlusWaiting = Self::OCR_APPSTARTING,
    /// Hourglass
    Waiting = Self::OCR_WAIT,
    /// Arrow and question mark
    NormalPlusQuestionMark = Self::OCR_HELP,
    /// Crosshair
    Crosshair = Self::OCR_CROSS,
    /// Hand
    Hand = Self::OCR_HAND,
    /// I-beam
    IBeam = Self::OCR_IBEAM,
    /// Slashed circle
    SlashedCircle = Self::OCR_NO,
    /// Four-pointed arrow pointing north, south, east, and west
    ArrowAllDirections = Self::OCR_SIZEALL,
    /// Double-pointed arrow pointing northeast and southwest
    ArrowNESW = Self::OCR_SIZENESW,
    /// Double-pointed arrow pointing north and south
    ArrowNS = Self::OCR_SIZENS,
    /// Double-pointed arrow pointing northwest and southeast
    ArrowNWSE = Self::OCR_SIZENWSE,
    /// Double-pointed arrow pointing west and east
    ArrowWE = Self::OCR_SIZEWE,
    /// Vertical arrow
    Up = Self::OCR_UP,
}

impl BuiltinCursor {
    // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setsystemcursor
    const OCR_APPSTARTING: u16 = 32650;
    const OCR_NORMAL: u16 = 32512;
    const OCR_CROSS: u16 = 32515;
    const OCR_HAND: u16 = 32649;
    const OCR_HELP: u16 = 32651;
    const OCR_IBEAM: u16 = 32513;
    const OCR_NO: u16 = 32648;
    const OCR_SIZEALL: u16 = 32646;
    const OCR_SIZENESW: u16 = 32643;
    const OCR_SIZENS: u16 = 32645;
    const OCR_SIZENWSE: u16 = 32642;
    const OCR_SIZEWE: u16 = 32644;
    const OCR_UP: u16 = 32516;
    const OCR_WAIT: u16 = 32514;
}

impl Default for BuiltinCursor {
    #[inline]
    fn default() -> Self {
        BuiltinCursor::Normal
    }
}

#[derive(Clone, Debug)]
pub struct Brush {
    standard_color_brush: BuiltinColor,
}

impl Brush {
    pub(crate) fn as_handle(&self) -> HBRUSH {
        i32::from(self.standard_color_brush) as HBRUSH
    }
}

impl From<BuiltinColor> for Brush {
    fn from(color: BuiltinColor) -> Self {
        Brush {
            standard_color_brush: color,
        }
    }
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(i32)]
pub enum BuiltinColor {
    Scrollbar = COLOR_SCROLLBAR,
    Background = COLOR_BACKGROUND,
    ActiveCaption = COLOR_ACTIVECAPTION,
    InactiveCaption = COLOR_INACTIVECAPTION,
    Menu = COLOR_MENU,
    Window = COLOR_WINDOW,
    WindowFrame = COLOR_WINDOWFRAME,
    MenuText = COLOR_MENUTEXT,
    WindowText = COLOR_WINDOWTEXT,
    CaptionText = COLOR_CAPTIONTEXT,
    ActiveBorder = COLOR_ACTIVEBORDER,
    InactiveBorder = COLOR_INACTIVEBORDER,
    AppWorkspace = COLOR_APPWORKSPACE,
    Highlight = COLOR_HIGHLIGHT,
    HighlightText = COLOR_HIGHLIGHTTEXT,
    ButtonFace = COLOR_BTNFACE,
    ButtonShadow = COLOR_BTNSHADOW,
    GrayText = COLOR_GRAYTEXT,
    ButtonText = COLOR_BTNTEXT,
    InactiveCaptionText = COLOR_INACTIVECAPTIONTEXT,
    ButtonHighlight = COLOR_BTNHIGHLIGHT,
    Shadow3DDark = COLOR_3DDKSHADOW,
    Light3D = COLOR_3DLIGHT,
    InfoText = COLOR_INFOTEXT,
    InfoBlack = COLOR_INFOBK,
    HotLight = COLOR_HOTLIGHT,
    GradientActiveCaption = COLOR_GRADIENTACTIVECAPTION,
    GradientInactiveCaption = COLOR_GRADIENTINACTIVECAPTION,
    MenuHighlight = COLOR_MENUHILIGHT,
    MenuBar = COLOR_MENUBAR,
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
    /// **Warning:** On Windows 7 (and possibly newer versions as well), when changing the progress state too quickly,
    /// the change may be ignored. As a workaround, you can sleep for a short time:
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
    /// thread::sleep(Duration::from_millis(20));
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
    fn get_toplevel_windows() -> io::Result<()> {
        let all_windows = WindowHandle::get_toplevel_windows()?;
        assert_gt!(all_windows.len(), 0);
        Ok(())
    }
}
