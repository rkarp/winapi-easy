use std::io;
use std::ptr;
use std::ptr::NonNull;

use num_enum::{
    IntoPrimitive,
};
use winapi::ctypes::c_void;
use winapi::shared::minwindef::{
    UINT,
    WORD,
};
use winapi::shared::windef::{
    HBRUSH,
    HCURSOR,
    HICON,
};
use winapi::um::winuser::{
    LoadImageW,
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
    IMAGE_CURSOR,
    IMAGE_ICON,
    LR_DEFAULTSIZE,
    LR_SHARED,
    MAKEINTRESOURCEW,
};

use crate::internal::{
    RawHandle,
};

pub trait Icon {
    fn as_handle(&self) -> io::Result<HICON>;
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u16)]
pub enum BuiltinIcon {
    Application = Self::OIC_SAMPLE,
    QuestionMark = Self::OIC_QUES,
    Warning = Self::OIC_WARNING,
    Error = Self::OIC_ERROR,
    Information = Self::OIC_INFORMATION,
    Shield = Self::OIC_SHIELD,
}

impl BuiltinIcon {
    const OIC_SAMPLE: u16 = 32512;
    const OIC_QUES: u16 = 32514;
    const OIC_WARNING: u16 = 32515;
    const OIC_ERROR: u16 = 32513;
    const OIC_INFORMATION: u16 = 32516;
    const OIC_SHIELD: u16 = 32518;
}

impl Icon for BuiltinIcon {
    fn as_handle(&self) -> io::Result<HICON> {
        let handle = get_shared_image_handle((*self).into(), IMAGE_ICON)?;
        Ok(handle.as_ptr() as HICON)
    }
}

impl Default for BuiltinIcon {
    fn default() -> Self {
        BuiltinIcon::Application
    }
}

pub trait Cursor {
    fn as_handle(&self) -> io::Result<HCURSOR>;
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

impl Cursor for BuiltinCursor {
    fn as_handle(&self) -> io::Result<HCURSOR> {
        let handle = get_shared_image_handle((*self).into(), IMAGE_CURSOR)?;
        Ok(handle.as_ptr() as HCURSOR)
    }
}

impl Default for BuiltinCursor {
    #[inline]
    fn default() -> Self {
        BuiltinCursor::Normal
    }
}

pub trait Brush {
    fn as_handle(&self) -> io::Result<HBRUSH>;
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

impl Brush for BuiltinColor {
    fn as_handle(&self) -> io::Result<HBRUSH> {
        Ok(i32::from(*self) as HBRUSH)
    }
}

fn get_shared_image_handle(resource_id: WORD, resource_type: UINT) -> io::Result<NonNull<c_void>> {
    let handle: NonNull<c_void> = unsafe {
        LoadImageW(
            ptr::null_mut(),
            MAKEINTRESOURCEW(resource_id),
            resource_type,
            0,
            0,
            LR_SHARED | LR_DEFAULTSIZE,
        )
        .to_non_null_else_get_last_error()?
    };
    Ok(handle)
}
