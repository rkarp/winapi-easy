//! An example magnifier app that will automatically magnify the foreground window on hotkey Ctrl + Alt + Shift + F.
//!
//! Exit via notification icon command.

// Hide console window in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::hooking::{
    HookReturnValue,
    LowLevelInputHookType,
    LowLevelMouseAction,
    LowLevelMouseHook,
    LowLevelMouseMessage,
    WinEventHook,
    WinEventKind,
    WinEventMessage,
};
use winapi_easy::input::hotkey::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::input::{
    KeyboardKey,
    get_mouse_speed,
    set_mouse_speed,
};
use winapi_easy::messaging::{
    ThreadMessage,
    ThreadMessageLoop,
};
use winapi_easy::ui::desktop::MonitorHandle;
use winapi_easy::ui::menu::{
    ItemSymbol,
    SubMenu,
    SubMenuItem,
    TextMenuItem,
};
use winapi_easy::ui::messaging::{
    CustomUserMessage,
    ListenerAnswer,
    ListenerMessage,
    ListenerMessageVariant,
};
use winapi_easy::ui::resource::{
    Brush,
    BuiltinColor,
    Icon,
};
use winapi_easy::ui::window::{
    DefaultWmlType,
    Layered,
    Magnifier,
    NotificationIconId,
    NotificationIconOptions,
    Window,
    WindowAppearance,
    WindowClass,
    WindowClassAppearance,
    WindowCommand,
    WindowExtendedStyle,
    WindowHandle,
    WindowShowState,
    WindowStyle,
    WindowZPosition,
};
use winapi_easy::ui::{
    CursorConcealment,
    CursorConfinement,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    Point,
    Rectangle,
    Region,
    get_virtual_screen_rect,
    remove_fullscreen_magnification,
    set_dpi_awareness_context,
    set_fullscreen_magnification,
    set_fullscreen_magnification_use_bitmap_smoothing,
};

fn main() -> io::Result<()> {
    set_dpi_awareness_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE)?;

    let listener = move |message: &ListenerMessage| {
        let answer;
        match message.variant {
            ListenerMessageVariant::WindowDestroy => {
                ThreadMessageLoop::post_quit_message();
                answer = ListenerAnswer::CallDefaultHandler
            }
            _ => answer = ListenerAnswer::default(),
        }
        answer
    };

    let icon: Rc<Icon> = Default::default();

    let mut main_window = Window::new::<_, ()>(
        WindowClass::register_new(
            "MagnifierMainClass",
            WindowClassAppearance {
                icon: Some(Rc::clone(&icon)),
                ..Default::default()
            },
        )?
        .into(),
        Some(listener),
        "Magnifier Main Window",
        Default::default(),
        None,
    )?;

    main_window.add_notification_icon(NotificationIconOptions {
        icon_id: NotificationIconId::Simple(0),
        icon: Rc::clone(&icon),
        tooltip_text: Some("Magnifier".to_string()),
        visible: true,
    })?;

    let mut magnifier_options = MagnifierOptions::default();

    let magnifier_context = RefCell::new(MagnifierContext::new()?);

    let mut popup = SubMenu::new()?;
    popup.insert_menu_item(
        SubMenuItem::Text(TextMenuItem {
            item_symbol: magnifier_options
                .use_integer_scaling
                .then_some(ItemSymbol::CheckMark),
            ..TextMenuItem::default_with_text(
                MenuID::UseIntegerScaling.into(),
                "Use integer scaling",
            )
        }),
        None,
    )?;
    popup.insert_menu_item(
        SubMenuItem::Text(TextMenuItem {
            item_symbol: magnifier_options
                .use_smoothing
                .then_some(ItemSymbol::CheckMark),
            ..TextMenuItem::default_with_text(MenuID::UseSmoothing.into(), "Use smoothing")
        }),
        None,
    )?;
    popup.insert_menu_item(
        SubMenuItem::Text(TextMenuItem {
            item_symbol: magnifier_context
                .borrow()
                .mouse_speed_mod
                .is_some()
                .then_some(ItemSymbol::CheckMark),
            ..TextMenuItem::default_with_text(
                MenuID::UseMouseSpeedMod.into(),
                "Auto adjust mouse speed",
            )
        }),
        None,
    )?;
    popup.insert_menu_item(SubMenuItem::Separator, None)?;
    popup.insert_menu_item(
        SubMenuItem::Text(TextMenuItem {
            item_symbol: magnifier_options
                .use_magnifier_control
                .then_some(ItemSymbol::CheckMark),
            ..TextMenuItem::default_with_text(
                MenuID::UseMagnifierControl.into(),
                "Use magnifier control",
            )
        }),
        None,
    )?;
    popup.insert_menu_item(SubMenuItem::Separator, None)?;
    popup.insert_menu_item(
        SubMenuItem::Text(TextMenuItem::default_with_text(MenuID::Exit.into(), "Exit")),
        None,
    )?;

    let _hotkeys = setup_hotkeys()?;

    let _mouse_hook = {
        let mouse_callback = |message: LowLevelMouseMessage| {
            match message.action {
                LowLevelMouseAction::Move => {
                    if let Some(confinement) = &magnifier_context.borrow().cursor_confinement {
                        confinement.reapply().unwrap();
                    }
                }
                _ => (),
            }
            HookReturnValue::CallNextHook
        };
        LowLevelMouseHook::add_hook::<0, _>(mouse_callback)?
    };

    let _win_event_hook = {
        let win_event_listener = |event: WinEventMessage| match event.event_kind {
            WinEventKind::ForegroundWindowChanged
            | WinEventKind::WindowUnminimized
            | WinEventKind::WindowMinimized
            | WinEventKind::WindowMoveEnd => {
                main_window
                    .send_user_message(CustomUserMessage {
                        message_id: UserMessageId::ForegroundWindowChanged.into(),
                        ..Default::default()
                    })
                    .unwrap();
            }
            _ => (),
        };
        WinEventHook::new::<1>(win_event_listener)
    };

    let loop_callback = |thread_message| match thread_message {
        ThreadMessage::WindowProc(window_message)
            if window_message.window_handle == *main_window =>
        {
            match window_message.variant {
                ListenerMessageVariant::MenuCommand { selected_item_id } => {
                    let selected_menu_id: MenuID = selected_item_id.into();
                    match selected_menu_id {
                        MenuID::UseIntegerScaling => {
                            let target_state = !magnifier_options.use_integer_scaling;
                            popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                item.item_symbol = target_state.then_some(ItemSymbol::CheckMark);
                                Ok(())
                            })?;
                            magnifier_options.use_integer_scaling = target_state;
                        }
                        MenuID::UseSmoothing => {
                            let target_state = !magnifier_options.use_smoothing;
                            popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                item.item_symbol = target_state.then_some(ItemSymbol::CheckMark);
                                Ok(())
                            })?;
                            magnifier_options.use_smoothing = target_state;
                        }
                        MenuID::UseMouseSpeedMod => {
                            let mut magnifier_context = magnifier_context.borrow_mut();
                            let target_state = !magnifier_context.mouse_speed_mod.is_some();
                            if target_state {
                                magnifier_context.mouse_speed_mod = Some(MouseSpeedMod::new()?);
                            } else {
                                magnifier_context.mouse_speed_mod = None;
                            }
                            popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                item.item_symbol = target_state.then_some(ItemSymbol::CheckMark);
                                Ok(())
                            })?;
                        }
                        MenuID::UseMagnifierControl => {
                            let target_state = !magnifier_options.use_magnifier_control;
                            magnifier_context.borrow_mut().set_variant(
                                target_state,
                                &magnifier_options,
                                &main_window,
                            )?;
                            popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                item.item_symbol = target_state.then_some(ItemSymbol::CheckMark);
                                Ok(())
                            })?;
                            magnifier_options.use_magnifier_control = target_state;
                        }
                        MenuID::Exit => main_window.send_command(WindowCommand::Close)?,
                        MenuID::Other(_) => unreachable!(),
                    }
                }
                ListenerMessageVariant::NotificationIconContextSelect { xy_coords, .. } => {
                    let _ = main_window.set_as_foreground();
                    popup.show_menu(*main_window, xy_coords)?;
                }
                ListenerMessageVariant::Timer { timer_id: 0 } => {
                    magnifier_context.borrow_mut().apply_timer_tick()?;
                }
                ListenerMessageVariant::CustomUserMessage(custom_message) => {
                    match UserMessageId::from(custom_message.message_id) {
                        UserMessageId::ForegroundWindowChanged => {
                            magnifier_context.borrow_mut().set_magnifier_initialized(
                                true,
                                &magnifier_options,
                                &main_window,
                            )?;
                        }
                        UserMessageId::TargetWindowChanged => {
                            if let Some(_window_handle) = magnifier_options.target_window_setting {
                                magnifier_context.borrow_mut().set_magnifier_initialized(
                                    true,
                                    &magnifier_options,
                                    &main_window,
                                )?;
                            } else {
                                magnifier_context.borrow_mut().set_magnifier_initialized(
                                    false,
                                    &magnifier_options,
                                    &main_window,
                                )?;
                            }
                        }
                        UserMessageId::Other(_) => unreachable!(),
                    }
                }
                _ => (),
            }
            Ok(())
        }
        ThreadMessage::Hotkey(hotkey_id) => {
            if let HotkeyId::SetTargetWindow = HotkeyId::from(hotkey_id) {
                let foreground_window = WindowHandle::get_foreground_window();
                if magnifier_options.target_window_setting.is_some() {
                    magnifier_options.target_window_setting = None;
                } else {
                    magnifier_options.target_window_setting = foreground_window;
                }
                main_window.send_user_message(CustomUserMessage {
                    message_id: UserMessageId::TargetWindowChanged.into(),
                    ..Default::default()
                })?;
                Ok(())
            } else {
                unreachable!()
            }
        }
        ThreadMessage::Other(_) => Ok(()),
        _ => Ok(()),
    };
    ThreadMessageLoop::new().run_with(loop_callback)?;
    Ok(())
}

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
enum MenuID {
    UseIntegerScaling,
    UseSmoothing,
    UseMouseSpeedMod,
    UseMagnifierControl,
    Exit,
    #[num_enum(catch_all)]
    Other(u32),
}

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
enum UserMessageId {
    ForegroundWindowChanged,
    TargetWindowChanged,
    #[num_enum(catch_all)]
    Other(u8),
}

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
enum HotkeyId {
    SetTargetWindow,
    #[num_enum(catch_all)]
    Other(u8),
}

#[derive(Debug)]
struct MagnifierOptions {
    use_integer_scaling: bool,
    use_smoothing: bool,
    use_magnifier_control: bool,
    target_window_setting: Option<WindowHandle>,
}

impl Default for MagnifierOptions {
    fn default() -> Self {
        Self {
            use_integer_scaling: false,
            use_smoothing: false,
            use_magnifier_control: false,
            target_window_setting: None,
        }
    }
}

struct MagnifierContext {
    magnifier_active: bool,
    variant: MagnifierVariant,
    mouse_speed_mod: Option<MouseSpeedMod>,
    cursor_hider: Option<CursorConcealment>,
    cursor_confinement: Option<CursorConfinement>,
    overlay_class: Rc<WindowClass>,
}

impl MagnifierContext {
    fn new() -> io::Result<Self> {
        let overlay_class = Self::register_overlay_class()?;
        let variant = Self::create_variant(Default::default(), Rc::clone(&overlay_class))?;
        Ok(Self {
            magnifier_active: false,
            variant,
            mouse_speed_mod: None,
            cursor_hider: None,
            cursor_confinement: None,
            overlay_class,
        })
    }

    fn set_variant(
        &mut self,
        use_magnifier_control: bool,
        magnifier_options: &MagnifierOptions,
        main_window: &Window,
    ) -> io::Result<()> {
        let is_control = match self.variant {
            MagnifierVariant::Fullscreen(..) => false,
            MagnifierVariant::Control(..) => true,
        };
        if is_control != use_magnifier_control {
            self.set_magnifier_initialized(false, magnifier_options, main_window)?;
            self.variant =
                Self::create_variant(use_magnifier_control, Rc::clone(&self.overlay_class))?;
            self.set_magnifier_initialized(true, magnifier_options, main_window)?;
        }
        Ok(())
    }

    fn set_magnifier_initialized(
        &mut self,
        enable: bool,
        magnifier_options: &MagnifierOptions,
        main_window: &Window,
    ) -> io::Result<()> {
        if enable {
            let maybe_foreground_window = WindowHandle::get_foreground_window();
            match maybe_foreground_window {
                Some(foreground_window)
                    if maybe_foreground_window == magnifier_options.target_window_setting =>
                {
                    self.set_magnifier_enabled(true, main_window)?;
                    self.adjust_for_target(foreground_window, magnifier_options)?;
                }
                _ => self.set_magnifier_enabled(false, main_window)?,
            }
        } else {
            self.set_magnifier_enabled(false, main_window)?;
        }
        Ok(())
    }

    fn set_magnifier_enabled(&mut self, enable: bool, main_window: &Window) -> io::Result<()> {
        if self.magnifier_active != enable {
            let overlay_window_handle;
            match &mut self.variant {
                MagnifierVariant::Fullscreen(fullscreen_magnifier) => {
                    if !enable {
                        remove_fullscreen_magnification()?;
                    }
                    overlay_window_handle = fullscreen_magnifier.overlay_window.as_handle();
                }
                MagnifierVariant::Control(magnifier_control) => {
                    if enable {
                        self.cursor_hider = Some(CursorConcealment::new()?);
                    } else {
                        self.cursor_hider = None;
                    }
                    overlay_window_handle = magnifier_control.overlay_window.borrow().as_handle();
                }
            };
            if enable {
                overlay_window_handle.set_show_state(WindowShowState::Show)?;
                // Possible Windows bug: The topmost setting may stop working if another window of the process
                // was set as the foreground window. As a workaround we reset it first.
                overlay_window_handle.set_z_position(WindowZPosition::NoTopMost)?;
                overlay_window_handle.set_z_position(WindowZPosition::TopMost)?;
            } else {
                if let Some(x) = &self.mouse_speed_mod {
                    x.disable()?
                }
                self.cursor_confinement = None;
                overlay_window_handle.set_z_position(WindowZPosition::Bottom)?;
                overlay_window_handle.set_show_state(WindowShowState::Hide)?;
            }
            self.variant.set_active(enable, main_window)?;
            self.magnifier_active = enable;
        }
        Ok(())
    }

    fn adjust_for_target(
        &mut self,
        foreground_window: WindowHandle,
        magnifier_options: &MagnifierOptions,
    ) -> io::Result<()> {
        let monitor_info = MonitorHandle::from_window(foreground_window).info()?;
        let source_window_rect = foreground_window.get_client_area_coords()?;

        let scaling_result = ScalingResult::from_rects(
            source_window_rect,
            monitor_info.monitor_area,
            magnifier_options.use_integer_scaling,
        );

        match &mut self.variant {
            MagnifierVariant::Fullscreen(fullscreen_magnifier) => {
                let overlay_window_extent = get_virtual_screen_rect();
                fullscreen_magnifier
                    .overlay_window
                    .modify_placement_with(|placement| {
                        placement.set_normal_position(overlay_window_extent);
                        Ok(())
                    })?;
                fullscreen_magnifier.overlay_window.set_region(
                    Region::from_rect(overlay_window_extent)
                        .and_not_in(&Region::from_rect(source_window_rect))?,
                )?;
                set_fullscreen_magnification_use_bitmap_smoothing(magnifier_options.use_smoothing)?;
                set_fullscreen_magnification(
                    scaling_result.scale_factor as f32,
                    scaling_result.unscaled_rect_centered_offset,
                )?;
            }
            MagnifierVariant::Control(magnifier_control) => {
                magnifier_control
                    .overlay_window
                    .borrow()
                    .as_handle()
                    .modify_placement_with(|placement| {
                        placement.set_normal_position(monitor_info.monitor_area);
                        Ok(())
                    })?;
                let control_window = &mut magnifier_control.control_window;
                control_window.modify_placement_with(|placement| {
                    placement.set_normal_position(scaling_result.max_scaled_rect_centered());
                    Ok(())
                })?;
                control_window.set_lens_use_bitmap_smoothing(magnifier_options.use_smoothing)?;
                control_window.set_magnification_factor(scaling_result.scale_factor as f32)?;
                control_window.set_magnification_source(source_window_rect)?;
            }
        }
        self.cursor_confinement = Some(CursorConfinement::new(source_window_rect)?);
        if let Some(x) = &self.mouse_speed_mod {
            x.enable(1.0 / scaling_result.scale_factor)?
        }
        Ok(())
    }

    fn apply_timer_tick(&mut self) -> io::Result<()> {
        match &mut self.variant {
            MagnifierVariant::Fullscreen(..) => panic!(),
            MagnifierVariant::Control(magnifier_control) => {
                magnifier_control.control_window.redraw()
            }
        }
    }

    fn create_variant(
        use_magnifier_control: bool,
        overlay_class: Rc<WindowClass>,
    ) -> io::Result<MagnifierVariant> {
        let result = if use_magnifier_control {
            MagnifierVariant::Control(MagnifierControl::new(overlay_class)?)
        } else {
            MagnifierVariant::Fullscreen(FullscreenMagnifier::new(overlay_class)?)
        };
        Ok(result)
    }

    fn register_overlay_class() -> io::Result<Rc<WindowClass>> {
        Ok(WindowClass::register_new(
            "MagnifierOverlayClass",
            WindowClassAppearance {
                background_brush: Some(Brush::from(BuiltinColor::InfoBlack).into()),
                ..Default::default()
            },
        )?
        .into())
    }
}

enum MagnifierVariant {
    Fullscreen(FullscreenMagnifier),
    Control(MagnifierControl),
}

impl MagnifierVariant {
    fn set_active(&self, active: bool, main_window: &Window) -> io::Result<()> {
        match self {
            MagnifierVariant::Fullscreen(..) => Ok(()),
            MagnifierVariant::Control(..) => {
                if active {
                    main_window.set_timer(0, 1000 / 60)?;
                } else {
                    let _ = main_window.kill_timer(0);
                }
                Ok(())
            }
        }
    }
}

struct FullscreenMagnifier {
    overlay_window: Window<Layered>,
}

impl FullscreenMagnifier {
    fn new(overlay_class: Rc<WindowClass>) -> io::Result<Self> {
        let overlay_window = create_overlay_window(
            overlay_class,
            "Fullscreen Magnifier Overlay",
            Default::default(),
        )?;
        Ok(Self { overlay_window })
    }
}

impl Drop for FullscreenMagnifier {
    fn drop(&mut self) {
        set_fullscreen_magnification_use_bitmap_smoothing(false).unwrap();
    }
}

struct MagnifierControl {
    control_window: Window<Magnifier>,
    overlay_window: Rc<RefCell<Window<Layered>>>,
}

impl MagnifierControl {
    fn new(overlay_class: Rc<WindowClass>) -> io::Result<Self> {
        let overlay_window = Rc::new(RefCell::new(create_overlay_window(
            overlay_class,
            "Magnifier Control Overlay",
            WindowExtendedStyle::Transparent,
        )?));
        let control_window = Window::new_magnifier(
            "Magnifier Control View",
            WindowAppearance {
                style: WindowStyle::Child | WindowStyle::Visible,
                extended_style: Default::default(),
            },
            Rc::clone(&overlay_window),
        )?;
        control_window.set_show_state(WindowShowState::Show)?;
        Ok(Self {
            control_window,
            overlay_window,
        })
    }
}

fn create_overlay_window(
    overlay_class: Rc<WindowClass>,
    caption_text: &str,
    extra_extended_style: WindowExtendedStyle,
) -> io::Result<Window<Layered>> {
    let overlay_window = Window::new_layered::<DefaultWmlType, ()>(
        overlay_class,
        None,
        caption_text,
        WindowAppearance {
            style: WindowStyle::Popup,
            extended_style: WindowExtendedStyle::NoActivate | extra_extended_style,
        },
        None,
    )?;
    overlay_window.set_layered_opacity_alpha(u8::MAX)?;
    Ok(overlay_window)
}

fn setup_hotkeys() -> io::Result<GlobalHotkeySet> {
    let mut hotkeys = GlobalHotkeySet::new();
    hotkeys.add_hotkey(
        HotkeyId::SetTargetWindow.into(),
        Modifier::Ctrl + Modifier::Alt + Modifier::Shift + KeyboardKey::F,
    )?;
    Ok(hotkeys)
}

#[derive(Debug)]
struct ScalingResult {
    scale_factor: f64,
    scaled_rect: Rectangle,
    scaled_rect_centered_offset: Point,
    unscaled_rect_centered_offset: Point,
}

impl ScalingResult {
    fn from_rects(source: Rectangle, target: Rectangle, use_integer_scaling: bool) -> Self {
        let source_width = source.right - source.left;
        let source_height = source.bottom - source.top;
        let target_width = target.right - target.left;
        let target_height = target.bottom - target.top;
        assert!(source_width > 0);
        assert!(source_height > 0);
        assert!(target_width > 0);
        assert!(target_height > 0);
        let scale_factor = {
            let max_width_scale = f64::from(target_width) / f64::from(source_width);
            let max_height_scale = f64::from(target_height) / f64::from(source_height);
            let max_scale_factor = f64::min(max_width_scale, max_height_scale);
            if use_integer_scaling {
                f64::max(1.0, max_scale_factor.trunc())
            } else {
                max_scale_factor
            }
        };
        let scaled_rect = Rectangle {
            left: 0,
            top: 0,
            right: (f64::from(source_width) * scale_factor).round() as i32,
            bottom: (f64::from(source_height) * scale_factor).round() as i32,
        };
        let unscaled_lens_width = (f64::from(target_width) / scale_factor).round() as i32;
        let unscaled_lens_height = (f64::from(target_height) / scale_factor).round() as i32;
        Self {
            scale_factor,
            scaled_rect,
            scaled_rect_centered_offset: Point {
                x: (target_width - scaled_rect.right) / 2,
                y: (target_height - scaled_rect.bottom) / 2,
            },
            unscaled_rect_centered_offset: Point {
                x: source.left - (unscaled_lens_width - source_width) / 2,
                y: source.top - (unscaled_lens_height - source_height) / 2,
            },
        }
    }

    fn max_scaled_rect_centered(&self) -> Rectangle {
        Rectangle {
            left: self.scaled_rect.left + self.scaled_rect_centered_offset.x,
            top: self.scaled_rect.top + self.scaled_rect_centered_offset.y,
            right: self.scaled_rect.right + self.scaled_rect_centered_offset.x,
            bottom: self.scaled_rect.bottom + self.scaled_rect_centered_offset.y,
        }
    }
}

#[derive(Debug)]
struct MouseSpeedMod {
    org_speed: u32,
}

impl MouseSpeedMod {
    fn new() -> io::Result<Self> {
        let org_speed = get_mouse_speed()?;
        Ok(Self { org_speed })
    }

    fn enable(&self, factor: f64) -> io::Result<()> {
        // See: https://stackoverflow.com/a/53022163
        const WINDOWS_MOUSE_SPEED_MULTS: [f64; 20] = [
            0.03125, 0.0625, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0, 1.25, 1.5, 1.75,
            2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5,
        ];
        assert!(factor > 0.0 && factor <= 1.0);
        let org_speed_mult =
            WINDOWS_MOUSE_SPEED_MULTS[usize::try_from(self.org_speed).unwrap() - 1];
        let target_speed = {
            let target_speed_mult = org_speed_mult * factor;
            assert!(target_speed_mult >= 0.0);
            let target_speed =
                WINDOWS_MOUSE_SPEED_MULTS.partition_point(|x| *x < target_speed_mult) + 1;
            u32::try_from(target_speed).unwrap()
        };
        set_mouse_speed(target_speed, false)
    }

    fn disable(&self) -> io::Result<()> {
        set_mouse_speed(self.org_speed, false)
    }
}

impl Drop for MouseSpeedMod {
    fn drop(&mut self) {
        self.disable().unwrap()
    }
}
