use std::io;

use num_enum::IntoPrimitive;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::{
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
    HBRUSH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    LoadImageW,
    GDI_IMAGE_TYPE,
    HCURSOR,
    HICON,
    IMAGE_CURSOR,
    IMAGE_ICON,
    LR_DEFAULTSIZE,
    LR_SHARED,
    OCR_APPSTARTING,
    OCR_CROSS,
    OCR_HAND,
    OCR_HELP,
    OCR_IBEAM,
    OCR_NO,
    OCR_NORMAL,
    OCR_SIZEALL,
    OCR_SIZENESW,
    OCR_SIZENS,
    OCR_SIZENWSE,
    OCR_SIZEWE,
    OCR_UP,
    OCR_WAIT,
    OIC_ERROR,
    OIC_INFORMATION,
    OIC_QUES,
    OIC_SAMPLE,
    OIC_SHIELD,
    OIC_WARNING,
};

use windows_missing::*;

pub trait Icon: Copy {
    fn as_handle(&self) -> io::Result<HICON>;
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(u32)]
pub enum BuiltinIcon {
    #[default]
    Application = OIC_SAMPLE,
    QuestionMark = OIC_QUES,
    Warning = OIC_WARNING,
    Error = OIC_ERROR,
    Information = OIC_INFORMATION,
    Shield = OIC_SHIELD,
}

impl Icon for BuiltinIcon {
    fn as_handle(&self) -> io::Result<HICON> {
        let handle = get_shared_image_handle((*self).into(), IMAGE_ICON)?;
        Ok(HICON(handle.0))
    }
}

pub trait Cursor {
    fn as_handle(&self) -> io::Result<HCURSOR>;
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(u32)]
pub enum BuiltinCursor {
    /// Standard arrow
    #[default]
    Normal = OCR_NORMAL.0,
    /// Standard arrow and small hourglass
    NormalPlusWaiting = OCR_APPSTARTING.0,
    /// Hourglass
    Waiting = OCR_WAIT.0,
    /// Arrow and question mark
    NormalPlusQuestionMark = OCR_HELP.0,
    /// Crosshair
    Crosshair = OCR_CROSS.0,
    /// Hand
    Hand = OCR_HAND.0,
    /// I-beam
    IBeam = OCR_IBEAM.0,
    /// Slashed circle
    SlashedCircle = OCR_NO.0,
    /// Four-pointed arrow pointing north, south, east, and west
    ArrowAllDirections = OCR_SIZEALL.0,
    /// Double-pointed arrow pointing northeast and southwest
    ArrowNESW = OCR_SIZENESW.0,
    /// Double-pointed arrow pointing north and south
    ArrowNS = OCR_SIZENS.0,
    /// Double-pointed arrow pointing northwest and southeast
    ArrowNWSE = OCR_SIZENWSE.0,
    /// Double-pointed arrow pointing west and east
    ArrowWE = OCR_SIZEWE.0,
    /// Vertical arrow
    Up = OCR_UP.0,
}

impl Cursor for BuiltinCursor {
    fn as_handle(&self) -> io::Result<HCURSOR> {
        let handle = get_shared_image_handle((*self).into(), IMAGE_CURSOR)?;
        Ok(HCURSOR(handle.0))
    }
}

pub trait Brush {
    fn as_handle(&self) -> io::Result<HBRUSH>;
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(i32)]
pub enum BuiltinColor {
    #[default]
    Scrollbar = COLOR_SCROLLBAR.0,
    Background = COLOR_BACKGROUND.0,
    ActiveCaption = COLOR_ACTIVECAPTION.0,
    InactiveCaption = COLOR_INACTIVECAPTION.0,
    Menu = COLOR_MENU.0,
    Window = COLOR_WINDOW.0,
    WindowFrame = COLOR_WINDOWFRAME.0,
    MenuText = COLOR_MENUTEXT.0,
    WindowText = COLOR_WINDOWTEXT.0,
    CaptionText = COLOR_CAPTIONTEXT.0,
    ActiveBorder = COLOR_ACTIVEBORDER.0,
    InactiveBorder = COLOR_INACTIVEBORDER.0,
    AppWorkspace = COLOR_APPWORKSPACE.0,
    Highlight = COLOR_HIGHLIGHT.0,
    HighlightText = COLOR_HIGHLIGHTTEXT.0,
    ButtonFace = COLOR_BTNFACE.0,
    ButtonShadow = COLOR_BTNSHADOW.0,
    GrayText = COLOR_GRAYTEXT.0,
    ButtonText = COLOR_BTNTEXT.0,
    InactiveCaptionText = COLOR_INACTIVECAPTIONTEXT.0,
    ButtonHighlight = COLOR_BTNHIGHLIGHT.0,
    Shadow3DDark = COLOR_3DDKSHADOW.0,
    Light3D = COLOR_3DLIGHT.0,
    InfoText = COLOR_INFOTEXT.0,
    InfoBlack = COLOR_INFOBK.0,
    HotLight = COLOR_HOTLIGHT.0,
    GradientActiveCaption = COLOR_GRADIENTACTIVECAPTION.0,
    GradientInactiveCaption = COLOR_GRADIENTINACTIVECAPTION.0,
    MenuHighlight = COLOR_MENUHILIGHT.0,
    MenuBar = COLOR_MENUBAR.0,
}

impl Brush for BuiltinColor {
    fn as_handle(&self) -> io::Result<HBRUSH> {
        Ok(HBRUSH(i32::from(*self).try_into().expect(
            "Conversion of built in color IDs to isize should never fail",
        )))
    }
}

fn get_shared_image_handle(resource_id: u32, resource_type: GDI_IMAGE_TYPE) -> io::Result<HANDLE> {
    let handle = unsafe {
        LoadImageW(
            None,
            MAKEINTRESOURCEW(resource_id),
            resource_type,
            0,
            0,
            LR_SHARED | LR_DEFAULTSIZE,
        )?
    };
    Ok(handle)
}

mod windows_missing {
    use windows::core::PCWSTR;

    // Temporary function until this gets resolved: https://github.com/microsoft/windows-rs/issues/641
    #[allow(non_snake_case)]
    #[inline]
    pub fn MAKEINTRESOURCEW(i: u32) -> PCWSTR {
        PCWSTR(i as usize as *const u16)
    }
}
