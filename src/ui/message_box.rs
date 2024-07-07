use crate::internal::ReturnValue;
use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use std::io;
use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxExW,
    IDABORT,
    IDCANCEL,
    IDCONTINUE,
    IDIGNORE,
    IDNO,
    IDOK,
    IDRETRY,
    IDTRYAGAIN,
    IDYES,
    MB_ABORTRETRYIGNORE,
    MB_CANCELTRYCONTINUE,
    MB_ICONERROR,
    MB_ICONINFORMATION,
    MB_ICONQUESTION,
    MB_ICONWARNING,
    MB_OK,
    MB_OKCANCEL,
    MB_RETRYCANCEL,
    MB_YESNO,
    MB_YESNOCANCEL,
    MESSAGEBOX_STYLE,
};

use crate::string::ToWideString;
use crate::ui::WindowHandle;

#[derive(Copy, Clone, Default, Debug)]
pub struct MessageBoxOptions<'a> {
    pub message: Option<&'a str>,
    pub caption: Option<&'a str>,
    pub buttons: MessageBoxButtons,
    pub icon: Option<MessageBoxIcon>,
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(u32)]
pub enum MessageBoxButtons {
    #[default]
    Ok = MB_OK.0,
    OkCancel = MB_OKCANCEL.0,
    RetryCancel = MB_RETRYCANCEL.0,
    YesNo = MB_YESNO.0,
    YesNoCancel = MB_YESNOCANCEL.0,
    AbortRetryIgnore = MB_ABORTRETRYIGNORE.0,
    CancelTryContinue = MB_CANCELTRYCONTINUE.0,
}

impl From<MessageBoxButtons> for MESSAGEBOX_STYLE {
    fn from(value: MessageBoxButtons) -> Self {
        MESSAGEBOX_STYLE(value.into())
    }
}

#[derive(IntoPrimitive, Copy, Clone, Eq, PartialEq, Default, Debug)]
#[repr(u32)]
pub enum MessageBoxIcon {
    #[default]
    Information = MB_ICONINFORMATION.0,
    QuestionMark = MB_ICONQUESTION.0,
    Warning = MB_ICONWARNING.0,
    Error = MB_ICONERROR.0,
}

impl From<MessageBoxIcon> for MESSAGEBOX_STYLE {
    fn from(value: MessageBoxIcon) -> Self {
        MESSAGEBOX_STYLE(value.into())
    }
}

#[derive(FromPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(i32)]
pub enum PressedMessageBoxButton {
    Ok = IDOK.0,
    Cancel = IDCANCEL.0,
    Abort = IDABORT.0,
    Retry = IDRETRY.0,
    Ignore = IDIGNORE.0,
    Yes = IDYES.0,
    No = IDNO.0,
    TryAgain = IDTRYAGAIN.0,
    Continue = IDCONTINUE.0,
    #[num_enum(catch_all)]
    Other(i32),
}

pub fn show_message_box(
    window_handle: &WindowHandle,
    options: MessageBoxOptions,
) -> io::Result<PressedMessageBoxButton> {
    let result = unsafe {
        MessageBoxExW(
            window_handle.raw_handle,
            options
                .message
                .map(|x| PCWSTR::from_raw(x.to_wide_string().as_ptr()))
                .as_ref(),
            options
                .caption
                .map(|x| PCWSTR::from_raw(x.to_wide_string().as_ptr()))
                .as_ref(),
            MESSAGEBOX_STYLE::from(options.buttons)
                | options.icon.map(MESSAGEBOX_STYLE::from).unwrap_or_default(),
            0,
        )
    };
    let _ = result.0.if_null_get_last_error()?;
    Ok(result.0.into())
}
