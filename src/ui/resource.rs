//! Application resources (icons, cursors and brushes).

use std::path::Path;
use std::{
    io,
    ptr,
};

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
    DestroyCursor,
    DestroyIcon,
    GDI_IMAGE_TYPE,
    HCURSOR,
    HICON,
    IMAGE_CURSOR,
    IMAGE_ICON,
    LR_DEFAULTSIZE,
    LR_LOADFROMFILE,
    LR_SHARED,
    LoadImageW,
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
use windows_missing::MAKEINTRESOURCEW;

pub(crate) use self::private::*;
use crate::internal::ResultExt;
use crate::module::ExecutableModule;
use crate::string::ZeroTerminatedWideString;

mod private {
    #[expect(clippy::wildcard_imports)]
    use super::*;

    pub trait ImageHandleKind: Copy + Sized {
        type BuiltinType: BuiltinImageKind;
        const RESOURCE_TYPE: GDI_IMAGE_TYPE;

        fn from_untyped_handle(handle: HANDLE) -> Self;

        /// Destroys a non-shared image handle.
        fn destroy(self) -> io::Result<()>;
    }

    pub trait ImageKindInternal {
        type Handle: ImageHandleKind;
        fn new_from_loaded_image(loaded_image: LoadedImage<Self::Handle>) -> Self;
        fn as_handle(&self) -> Self::Handle;
    }

    pub trait BuiltinImageKind: Into<u32> {
        fn into_ordinal(self) -> u32 {
            self.into()
        }
    }

    #[derive(Eq, PartialEq, Debug)]
    pub struct LoadedImage<H: ImageHandleKind> {
        handle: H,
        shared: bool,
    }

    impl<H: ImageHandleKind> LoadedImage<H> {
        pub(crate) fn from_builtin(builtin: H::BuiltinType) -> io::Result<Self> {
            Self::load(LoadImageVariant::BuiltinId(builtin.into_ordinal()))
        }

        pub(crate) fn from_module_by_name(
            module: &ExecutableModule,
            name: String,
        ) -> io::Result<Self> {
            Self::load(LoadImageVariant::FromModule {
                module,
                module_load_variant: LoadImageFromModuleVariant::ByName(name),
                load_as_shared: true,
            })
        }

        pub(crate) fn from_module_by_ordinal(
            module: &ExecutableModule,
            ordinal: u32,
        ) -> io::Result<Self> {
            Self::load(LoadImageVariant::FromModule {
                module,
                module_load_variant: LoadImageFromModuleVariant::ByOrdinal(ordinal),
                load_as_shared: true,
            })
        }

        pub(crate) fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
            Self::load(LoadImageVariant::FromFile(path.as_ref()))
        }

        pub(crate) fn as_handle(&self) -> H {
            self.handle
        }

        fn load(load_params: LoadImageVariant) -> io::Result<Self> {
            let handle_param;
            let base_flags;
            let name_data;
            let name_param;
            let shared;
            match load_params {
                LoadImageVariant::BuiltinId(resource_id) => {
                    handle_param = None;
                    base_flags = Default::default();
                    name_param = MAKEINTRESOURCEW(resource_id);
                    shared = true;
                }
                LoadImageVariant::FromModule {
                    module,
                    module_load_variant,
                    load_as_shared,
                } => {
                    handle_param = Some(module.as_hinstance());
                    base_flags = Default::default();
                    match module_load_variant {
                        LoadImageFromModuleVariant::ByOrdinal(ordinal) => {
                            name_param = MAKEINTRESOURCEW(ordinal);
                        }
                        LoadImageFromModuleVariant::ByName(name) => {
                            name_data = ZeroTerminatedWideString::from_os_str(name);
                            name_param = name_data.as_raw_pcwstr();
                        }
                    }
                    shared = load_as_shared;
                }
                LoadImageVariant::FromFile(file_name) => {
                    handle_param = None;
                    base_flags = LR_LOADFROMFILE;
                    name_data = ZeroTerminatedWideString::from_os_str(file_name);
                    name_param = name_data.as_raw_pcwstr();
                    shared = false;
                }
            }
            let flags = if shared {
                base_flags | LR_SHARED
            } else {
                base_flags
            };
            let handle = unsafe {
                LoadImageW(
                    handle_param,
                    name_param,
                    H::RESOURCE_TYPE,
                    0,
                    0,
                    flags | LR_DEFAULTSIZE,
                )?
            };
            let handle = H::from_untyped_handle(handle);
            Ok(Self { handle, shared })
        }
    }

    impl<H: ImageHandleKind> Drop for LoadedImage<H> {
        fn drop(&mut self) {
            if !self.shared {
                self.handle.destroy().unwrap_or_default_and_print_error();
            }
        }
    }

    impl TryFrom<BuiltinIcon> for LoadedImage<HICON> {
        type Error = io::Error;

        fn try_from(value: BuiltinIcon) -> Result<Self, Self::Error> {
            Self::from_builtin(value)
        }
    }

    impl TryFrom<BuiltinCursor> for LoadedImage<HCURSOR> {
        type Error = io::Error;

        fn try_from(value: BuiltinCursor) -> Result<Self, Self::Error> {
            Self::from_builtin(value)
        }
    }
}

impl ImageHandleKind for HICON {
    type BuiltinType = BuiltinIcon;
    const RESOURCE_TYPE: GDI_IMAGE_TYPE = IMAGE_ICON;

    fn from_untyped_handle(handle: HANDLE) -> Self {
        Self(handle.0)
    }

    fn destroy(self) -> io::Result<()> {
        unsafe {
            DestroyIcon(self)?;
        }
        Ok(())
    }
}

impl ImageHandleKind for HCURSOR {
    type BuiltinType = BuiltinCursor;
    const RESOURCE_TYPE: GDI_IMAGE_TYPE = IMAGE_CURSOR;

    fn from_untyped_handle(handle: HANDLE) -> Self {
        Self(handle.0)
    }

    fn destroy(self) -> io::Result<()> {
        unsafe {
            DestroyCursor(self)?;
        }
        Ok(())
    }
}

pub trait ImageKind: ImageKindInternal + Sized {
    fn from_builtin(builtin: <Self::Handle as ImageHandleKind>::BuiltinType) -> Self {
        Self::new_from_loaded_image(
            LoadedImage::from_builtin(builtin).unwrap_or_else(|_| unreachable!()),
        )
    }

    fn from_module_by_name(module: &ExecutableModule, name: String) -> io::Result<Self> {
        Ok(Self::new_from_loaded_image(
            LoadedImage::from_module_by_name(module, name)?,
        ))
    }

    fn from_module_by_ordinal(module: &ExecutableModule, ordinal: u32) -> io::Result<Self> {
        Ok(Self::new_from_loaded_image(
            LoadedImage::from_module_by_ordinal(module, ordinal)?,
        ))
    }

    fn from_file<A: AsRef<Path>>(path: A) -> io::Result<Self> {
        Ok(Self::new_from_loaded_image(LoadedImage::from_file(path)?))
    }
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

impl BuiltinImageKind for BuiltinIcon {}

#[derive(Eq, PartialEq, Debug)]
pub struct Icon(LoadedImage<HICON>);

impl ImageKindInternal for Icon {
    type Handle = HICON;

    fn new_from_loaded_image(loaded_image: LoadedImage<Self::Handle>) -> Self {
        Self(loaded_image)
    }

    fn as_handle(&self) -> Self::Handle {
        self.0.as_handle()
    }
}

impl ImageKind for Icon {}

impl From<BuiltinIcon> for Icon {
    fn from(value: BuiltinIcon) -> Self {
        Self::from_builtin(value)
    }
}

impl Default for Icon {
    fn default() -> Self {
        Self::from(BuiltinIcon::default())
    }
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

impl BuiltinImageKind for BuiltinCursor {}

#[derive(Eq, PartialEq, Debug)]
pub struct Cursor(LoadedImage<HCURSOR>);

impl ImageKindInternal for Cursor {
    type Handle = HCURSOR;

    fn new_from_loaded_image(loaded_image: LoadedImage<Self::Handle>) -> Self {
        Self(loaded_image)
    }

    fn as_handle(&self) -> Self::Handle {
        self.0.as_handle()
    }
}

impl ImageKind for Cursor {}

impl From<BuiltinCursor> for Cursor {
    fn from(value: BuiltinCursor) -> Self {
        Self::from_builtin(value)
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::from(BuiltinCursor::default())
    }
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

impl BuiltinColor {
    fn as_handle(&self) -> HBRUSH {
        HBRUSH(ptr::with_exposed_provenance_mut(
            i32::from(*self)
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
        ))
    }
}

#[derive(Eq, PartialEq, Debug)]
enum BrushKind {
    BuiltinColor(BuiltinColor),
}

impl BrushKind {
    pub(crate) fn as_handle(&self) -> HBRUSH {
        match self {
            Self::BuiltinColor(builtin_brush) => builtin_brush.as_handle(),
        }
    }
}

impl Default for BrushKind {
    fn default() -> Self {
        Self::BuiltinColor(Default::default())
    }
}

#[derive(Eq, PartialEq, Default, Debug)]
pub struct Brush(BrushKind);

impl Brush {
    pub(crate) fn as_handle(&self) -> HBRUSH {
        self.0.as_handle()
    }
}

impl From<BuiltinColor> for Brush {
    fn from(value: BuiltinColor) -> Self {
        Self(BrushKind::BuiltinColor(value))
    }
}

enum LoadImageVariant<'a> {
    BuiltinId(u32),
    FromModule {
        module: &'a ExecutableModule,
        module_load_variant: LoadImageFromModuleVariant,
        load_as_shared: bool,
    },
    FromFile(&'a Path),
}

enum LoadImageFromModuleVariant {
    ByOrdinal(u32),
    ByName(String),
}

mod windows_missing {
    use windows::core::PCWSTR;

    // Temporary function until this gets resolved: https://github.com/microsoft/windows-rs/issues/641
    #[expect(non_snake_case)]
    pub fn MAKEINTRESOURCEW(i: u32) -> PCWSTR {
        PCWSTR(i as usize as *const u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_builtin_icon() -> io::Result<()> {
        let icon = Icon::from_builtin(BuiltinIcon::default());
        assert!(!icon.as_handle().is_invalid());
        Ok(())
    }

    #[test]
    fn load_shell32_icon() -> io::Result<()> {
        let module = ExecutableModule::load_module_as_data_file("shell32.dll")?;
        let icon = Icon::from_module_by_ordinal(&module, 1)?;
        assert!(!icon.as_handle().is_invalid());
        Ok(())
    }
}
