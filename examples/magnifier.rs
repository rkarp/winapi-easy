//! An example magnifier app that will automatically magnify the foreground window on hotkey Ctrl + Alt + Shift + F.
//!
//! Exit via notification icon command.

// Hide console window in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::rc::Rc;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use num_traits::ToPrimitive;
use winapi_easy::hooking::{
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
use winapi_easy::module::ExecutableModule;
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
    ImageKind,
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
    CursorConfinement,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    Point,
    Rectangle,
    Region,
    UnmagnifiedCursorConcealment,
    get_cursor_pos,
    get_virtual_screen_rect,
    remove_fullscreen_magnification,
    set_fullscreen_magnification,
    set_fullscreen_magnification_use_bitmap_smoothing,
    set_process_dpi_awareness_context,
};

#[expect(clippy::too_many_lines)]
fn main() -> anyhow::Result<()> {
    set_process_dpi_awareness_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE)?;

    let listener = move |message: &ListenerMessage| match message.variant {
        ListenerMessageVariant::WindowDestroy => {
            ThreadMessageLoop::post_quit_message();
            ListenerAnswer::CallDefaultHandler
        }
        _ => ListenerAnswer::default(),
    };

    let icon: Rc<Icon> = {
        let icon_module = ExecutableModule::load_module_as_data_file("shell32.dll")?;
        let icon = Icon::from_module_by_ordinal(&icon_module, 23).unwrap_or_default();
        icon.into()
    };

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
        WindowAppearance::default(),
        None,
    )?;

    main_window.add_notification_icon(NotificationIconOptions {
        icon_id: NotificationIconId::Simple(0),
        icon: Rc::clone(&icon),
        tooltip_text: Some("Magnifier".to_string()),
        visible: true,
    })?;

    let magnifier_context = RefCell::new(MagnifierContext::new()?);

    let mut popup = {
        let magnifier_options = &magnifier_context.borrow().options;
        SubMenu::new_from_items([
            SubMenuItem::Text(TextMenuItem {
                id: MenuID::UseIntegerScaling.into(),
                text: "Use integer scaling".to_owned(),
                item_symbol: magnifier_options
                    .use_integer_scaling
                    .then_some(ItemSymbol::CheckMark),
                ..TextMenuItem::default()
            }),
            SubMenuItem::Text(TextMenuItem {
                id: MenuID::UseSmoothing.into(),
                text: "Use smoothing".to_owned(),
                item_symbol: magnifier_options
                    .use_smoothing
                    .then_some(ItemSymbol::CheckMark),
                ..TextMenuItem::default()
            }),
            SubMenuItem::Text(TextMenuItem {
                id: MenuID::UseMouseSpeedMod.into(),
                text: "Auto adjust mouse speed".to_owned(),
                item_symbol: magnifier_context
                    .borrow()
                    .mouse_speed_mod
                    .is_some()
                    .then_some(ItemSymbol::CheckMark),
                ..TextMenuItem::default()
            }),
            SubMenuItem::Separator,
            SubMenuItem::Text(TextMenuItem {
                id: MenuID::UseMagnifierControl.into(),
                text: "Use magnifier control mode".to_owned(),
                item_symbol: magnifier_options
                    .use_magnifier_control
                    .then_some(ItemSymbol::CheckMark),
                ..TextMenuItem::default()
            }),
            SubMenuItem::Separator,
            SubMenuItem::Text(TextMenuItem::default_with_text(MenuID::Exit.into(), "Exit")),
        ])?
    };

    let _hotkeys = setup_hotkeys()?;

    let loop_callback = |thread_message| -> Result<(), anyhow::Error> {
        let mut magnifier_context = magnifier_context.borrow_mut();
        match thread_message {
            ThreadMessage::WindowProc(window_message)
                if window_message.window_handle == *main_window =>
            {
                match window_message.variant {
                    ListenerMessageVariant::MenuCommand { selected_item_id } => {
                        let selected_menu_id: MenuID = selected_item_id.into();
                        match selected_menu_id {
                            MenuID::UseIntegerScaling => {
                                let target_state = !magnifier_context.options.use_integer_scaling;
                                popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                    item.item_symbol =
                                        target_state.then_some(ItemSymbol::CheckMark);
                                    Ok(())
                                })?;
                                magnifier_context.options.use_integer_scaling = target_state;
                            }
                            MenuID::UseSmoothing => {
                                let target_state = !magnifier_context.options.use_smoothing;
                                popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                    item.item_symbol =
                                        target_state.then_some(ItemSymbol::CheckMark);
                                    Ok(())
                                })?;
                                magnifier_context.options.use_smoothing = target_state;
                            }
                            MenuID::UseMouseSpeedMod => {
                                let target_state = magnifier_context.mouse_speed_mod.is_none();
                                if target_state {
                                    magnifier_context.mouse_speed_mod = Some(MouseSpeedMod::new()?);
                                } else {
                                    magnifier_context.mouse_speed_mod = None;
                                }
                                popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                    item.item_symbol =
                                        target_state.then_some(ItemSymbol::CheckMark);
                                    Ok(())
                                })?;
                            }
                            MenuID::UseMagnifierControl => {
                                let target_state = !magnifier_context.options.use_magnifier_control;
                                magnifier_context.set_variant(target_state, &main_window)?;
                                popup.modify_text_menu_items_by_id(selected_item_id, |item| {
                                    item.item_symbol =
                                        target_state.then_some(ItemSymbol::CheckMark);
                                    Ok(())
                                })?;
                                magnifier_context.options.use_magnifier_control = target_state;
                            }
                            MenuID::Exit => main_window.send_command(WindowCommand::Close)?,
                            MenuID::Other(_) => unreachable!(),
                        }
                    }
                    ListenerMessageVariant::NotificationIconContextSelect { .. } => {
                        let _ = main_window.set_as_foreground();
                        popup.show_menu(*main_window, get_cursor_pos()?)?;
                    }
                    ListenerMessageVariant::Timer { timer_id: 0 } => {
                        magnifier_context.apply_timer_tick()?;
                    }
                    ListenerMessageVariant::CustomUserMessage(custom_message) => {
                        match UserMessageId::from(custom_message.message_id) {
                            UserMessageId::WindowChanged => {
                                magnifier_context.set_magnifier_initialized(true, &main_window)?;
                            }
                            UserMessageId::WindowDestroyed => {
                                if let Some(window_lock) = &magnifier_context.window_lock
                                    && !window_lock.target_window.is_window()
                                {
                                    magnifier_context.window_lock = None;
                                    magnifier_context
                                        .set_magnifier_initialized(false, &main_window)?;
                                }
                            }
                            UserMessageId::ReapplyMouseConfinement => {
                                if let Some(confinement) = &magnifier_context.cursor_confinement {
                                    confinement.reapply()?;
                                }
                            }

                            UserMessageId::Other(_) => unreachable!(),
                        }
                    }
                    _ => (),
                }
            }
            ThreadMessage::Hotkey(hotkey_id) => {
                if let HotkeyId::SetTargetWindow = HotkeyId::from(hotkey_id) {
                    let foreground_window = WindowHandle::get_foreground_window();
                    if magnifier_context.window_lock.is_some() {
                        magnifier_context.window_lock = None;
                        magnifier_context.set_magnifier_initialized(false, &main_window)?;
                    } else if let Some(foreground_window) = foreground_window {
                        magnifier_context.window_lock =
                            Some(MagnifierWindowLock::new(*main_window, foreground_window)?);
                        magnifier_context.set_magnifier_initialized(true, &main_window)?;
                    }
                } else {
                    unreachable!()
                }
            }
            _ => (),
        }
        Ok(())
    };
    ThreadMessageLoop::new().run_with::<anyhow::Error, _>(loop_callback)?;
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
    WindowChanged,
    WindowDestroyed,
    ReapplyMouseConfinement,
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

#[derive(Default, Debug)]
struct MagnifierOptions {
    use_integer_scaling: bool,
    use_smoothing: bool,
    use_magnifier_control: bool,
}

struct MagnifierWindowLock {
    #[expect(dead_code)]
    win_event_hook: WinEventHook<Box<dyn Fn(WinEventMessage)>>,
    target_window: WindowHandle,
}

impl MagnifierWindowLock {
    fn new(main_window_handle: WindowHandle, target_window: WindowHandle) -> anyhow::Result<Self> {
        const FILTER_EVENTS: &[WinEventKind] = &[
            WinEventKind::ObjectLocationChanged,
            WinEventKind::ForegroundWindowChanged,
            WinEventKind::WindowUnminimized,
            WinEventKind::WindowMinimized,
            WinEventKind::WindowMoveEnd,
            WinEventKind::ObjectDestroyed,
        ];
        let win_event_listener = move |event: WinEventMessage| match event.event_kind {
            WinEventKind::ObjectLocationChanged
            | WinEventKind::ForegroundWindowChanged
            | WinEventKind::WindowUnminimized
            | WinEventKind::WindowMinimized
            | WinEventKind::WindowMoveEnd
                if event.window_handle.is_some() =>
            {
                main_window_handle
                    .send_user_message(CustomUserMessage {
                        message_id: UserMessageId::WindowChanged.into(),
                        ..Default::default()
                    })
                    .unwrap();
            }
            WinEventKind::ObjectDestroyed if event.window_handle.is_some() => {
                main_window_handle
                    .send_user_message(CustomUserMessage {
                        message_id: UserMessageId::WindowDestroyed.into(),
                        ..Default::default()
                    })
                    .unwrap();
            }
            WinEventKind::ObjectLocationChanged if event.window_handle.is_none() => {
                main_window_handle
                    .send_user_message(CustomUserMessage {
                        message_id: UserMessageId::ReapplyMouseConfinement.into(),
                        ..Default::default()
                    })
                    .unwrap();
            }
            _ => (),
        };
        let win_event_hook = WinEventHook::new::<1>(
            Box::new(win_event_listener) as Box<dyn Fn(_)>,
            Some(FILTER_EVENTS),
        )?;
        Ok(Self {
            win_event_hook,
            target_window,
        })
    }
}

struct MagnifierContext {
    magnifier_active: bool,
    variant: MagnifierVariant,
    options: MagnifierOptions,
    last_scaling: Option<Scaling>,
    window_lock: Option<MagnifierWindowLock>,
    mouse_speed_mod: Option<MouseSpeedMod>,
    cursor_hider: Option<UnmagnifiedCursorConcealment>,
    cursor_confinement: Option<CursorConfinement>,
    overlay_class: Rc<WindowClass>,
}

impl MagnifierContext {
    fn new() -> anyhow::Result<Self> {
        let overlay_class = Self::register_overlay_class()?;
        let variant = Self::create_variant(Default::default(), Rc::clone(&overlay_class))?;
        Ok(Self {
            magnifier_active: false,
            variant,
            options: MagnifierOptions::default(),
            last_scaling: None,
            window_lock: None,
            mouse_speed_mod: None,
            cursor_hider: None,
            cursor_confinement: None,
            overlay_class,
        })
    }

    fn set_variant(
        &mut self,
        use_magnifier_control: bool,
        main_window: &Window,
    ) -> anyhow::Result<()> {
        let is_control = match self.variant {
            MagnifierVariant::Fullscreen(..) => false,
            MagnifierVariant::Control(..) => true,
        };
        if is_control != use_magnifier_control {
            self.set_magnifier_initialized(false, main_window)?;
            self.variant =
                Self::create_variant(use_magnifier_control, Rc::clone(&self.overlay_class))?;
            self.set_magnifier_initialized(true, main_window)?;
        }
        Ok(())
    }

    fn set_magnifier_initialized(
        &mut self,
        enable: bool,
        main_window: &Window,
    ) -> anyhow::Result<()> {
        if enable {
            let maybe_foreground_window = WindowHandle::get_foreground_window();
            match (maybe_foreground_window, &self.window_lock) {
                (Some(foreground_window), Some(window_lock))
                    if foreground_window == window_lock.target_window
                        && has_nonzero_area(foreground_window.get_client_area_coords()?) =>
                {
                    self.set_magnifier_enabled(true, main_window)?;
                    self.adjust_for_target(foreground_window)?;
                }
                _ => self.set_magnifier_enabled(false, main_window)?,
            }
        } else {
            self.set_magnifier_enabled(false, main_window)?;
        }
        Ok(())
    }

    fn set_magnifier_enabled(&mut self, enable: bool, main_window: &Window) -> anyhow::Result<()> {
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
                        self.cursor_hider = Some(UnmagnifiedCursorConcealment::new()?);
                    } else {
                        self.cursor_hider = None;
                    }
                    overlay_window_handle = magnifier_control.overlay_window.borrow().as_handle();
                }
            }
            if enable {
                overlay_window_handle.set_show_state(WindowShowState::Show)?;
                // Possible Windows bug: The topmost setting may stop working if another window of the process
                // was set as the foreground window. As a workaround we reset it first.
                overlay_window_handle.set_z_position(WindowZPosition::NoTopMost)?;
                overlay_window_handle.set_z_position(WindowZPosition::TopMost)?;
            } else {
                self.last_scaling = None;
                if let Some(x) = &self.mouse_speed_mod {
                    x.disable()?;
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

    fn adjust_for_target(&mut self, foreground_window: WindowHandle) -> anyhow::Result<()> {
        let monitor_info = MonitorHandle::from_window(foreground_window).info()?;

        let source_window_rect;
        let scaling_result;
        {
            let initial_source_window_rect = foreground_window.get_client_area_coords()?;
            let initial_scaling_result = Scaling::from_rects(
                initial_source_window_rect,
                monitor_info.monitor_area,
                self.options.use_integer_scaling,
            );

            match &mut self.variant {
                MagnifierVariant::Fullscreen(_)
                    if (initial_scaling_result.scale_factor - 1.0).abs() < 0.1 =>
                {
                    // Scale factor of less than 1.1 disables magnifier, so window won't be centered
                    // by the magnification API and needs to be pre-centered directly instead
                    foreground_window.modify_placement_with(|placement| {
                        let old_position = placement.get_normal_position();
                        let new_position = center_rect(old_position, monitor_info.monitor_area);
                        placement.set_normal_position(new_position);
                        Ok(())
                    })?;
                    source_window_rect = foreground_window.get_client_area_coords()?;
                    scaling_result = Scaling::from_rects(
                        source_window_rect,
                        monitor_info.monitor_area,
                        self.options.use_integer_scaling,
                    );
                }
                MagnifierVariant::Fullscreen(_) | MagnifierVariant::Control(_) => {
                    source_window_rect = initial_source_window_rect;
                    scaling_result = initial_scaling_result;
                }
            }
        }

        if let Some(last_scaling) = &self.last_scaling
            && *last_scaling == scaling_result
        {
            return Ok(());
        }

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
                    Region::from_rect(overlay_window_extent)?
                        .and_not_in(&Region::from_rect(source_window_rect)?)?,
                )?;
                set_fullscreen_magnification_use_bitmap_smoothing(self.options.use_smoothing)?;
                set_fullscreen_magnification(
                    scaling_result.scale_factor.to_f32().unwrap(),
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
                control_window.set_lens_use_bitmap_smoothing(self.options.use_smoothing)?;
                control_window
                    .set_magnification_factor(scaling_result.scale_factor.to_f32().unwrap())?;
                control_window.set_magnification_source(source_window_rect)?;
            }
        }
        self.cursor_confinement = Some(CursorConfinement::new(source_window_rect)?);
        if let Some(x) = &self.mouse_speed_mod {
            x.enable(1.0 / scaling_result.scale_factor)?;
        }
        self.last_scaling = Some(scaling_result);
        Ok(())
    }

    fn apply_timer_tick(&mut self) -> anyhow::Result<()> {
        match &mut self.variant {
            MagnifierVariant::Fullscreen(..) => panic!(),
            MagnifierVariant::Control(magnifier_control) => {
                magnifier_control.control_window.redraw()?;
                Ok(())
            }
        }
    }

    fn create_variant(
        use_magnifier_control: bool,
        overlay_class: Rc<WindowClass>,
    ) -> anyhow::Result<MagnifierVariant> {
        let result = if use_magnifier_control {
            MagnifierVariant::Control(MagnifierControl::new(overlay_class)?)
        } else {
            MagnifierVariant::Fullscreen(FullscreenMagnifier::new(overlay_class)?)
        };
        Ok(result)
    }

    fn register_overlay_class() -> anyhow::Result<Rc<WindowClass>> {
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
    fn set_active(&self, active: bool, main_window: &Window) -> anyhow::Result<()> {
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
    fn new(overlay_class: Rc<WindowClass>) -> anyhow::Result<Self> {
        let overlay_window = create_overlay_window(
            overlay_class,
            "Fullscreen Magnifier Overlay",
            WindowExtendedStyle::default(),
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
    fn new(overlay_class: Rc<WindowClass>) -> anyhow::Result<Self> {
        let overlay_window = Rc::new(RefCell::new(create_overlay_window(
            overlay_class,
            "Magnifier Control Overlay",
            WindowExtendedStyle::Transparent,
        )?));
        let control_window = Window::new_magnifier(
            "Magnifier Control View",
            WindowAppearance {
                style: WindowStyle::Child | WindowStyle::Visible,
                extended_style: WindowExtendedStyle::default(),
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
) -> anyhow::Result<Window<Layered>> {
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

fn setup_hotkeys() -> anyhow::Result<GlobalHotkeySet> {
    let mut hotkeys = GlobalHotkeySet::new();
    hotkeys.add_hotkey(
        HotkeyId::SetTargetWindow.into(),
        Modifier::Ctrl + Modifier::Alt + Modifier::Shift + KeyboardKey::F,
    )?;
    Ok(hotkeys)
}

fn center_rect(source: Rectangle, target: Rectangle) -> Rectangle {
    let source_width = source.right - source.left;
    let source_height = source.bottom - source.top;
    let target_width = target.right - target.left;
    let target_height = target.bottom - target.top;
    let centered_offset_x = (target_width - source_width) / 2;
    let centered_offset_y = (target_height - source_height) / 2;
    Rectangle {
        left: target.left + centered_offset_x,
        top: target.top + centered_offset_y,
        right: target.right - centered_offset_x,
        bottom: target.bottom - centered_offset_y,
    }
}

#[derive(PartialEq, Debug)]
struct Scaling {
    scale_factor: f64,
    scaled_rect: Rectangle,
    scaled_rect_centered_offset: Point,
    unscaled_rect_centered_offset: Point,
}

impl Scaling {
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
            let max_scale_factor = if use_integer_scaling {
                max_scale_factor.trunc()
            } else {
                max_scale_factor
            };
            f64::max(1.0, max_scale_factor)
        };
        let scaled_rect = Rectangle {
            left: 0,
            top: 0,
            right: (f64::from(source_width) * scale_factor)
                .round()
                .to_i32()
                .unwrap(),
            bottom: (f64::from(source_height) * scale_factor)
                .round()
                .to_i32()
                .unwrap(),
        };
        let unscaled_lens_width = (f64::from(target_width) / scale_factor)
            .round()
            .to_i32()
            .unwrap();
        let unscaled_lens_height = (f64::from(target_height) / scale_factor)
            .round()
            .to_i32()
            .unwrap();
        let unscaled_lens_x_offset = (f64::from(target.left) / scale_factor)
            .round()
            .to_i32()
            .unwrap();
        let unscaled_lens_y_offset = (f64::from(target.top) / scale_factor)
            .round()
            .to_i32()
            .unwrap();
        Self {
            scale_factor,
            scaled_rect,
            scaled_rect_centered_offset: Point {
                x: (target_width - scaled_rect.right) / 2,
                y: (target_height - scaled_rect.bottom) / 2,
            },
            unscaled_rect_centered_offset: Point {
                x: source.left - unscaled_lens_x_offset - (unscaled_lens_width - source_width) / 2,
                y: source.top - unscaled_lens_y_offset - (unscaled_lens_height - source_height) / 2,
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

fn has_nonzero_area(source: Rectangle) -> bool {
    let source_width = source.right - source.left;
    let source_height = source.bottom - source.top;
    source_width > 0 && source_height > 0
}

#[derive(Debug)]
struct MouseSpeedMod {
    org_speed: u32,
}

impl MouseSpeedMod {
    fn new() -> anyhow::Result<Self> {
        let org_speed = get_mouse_speed()?;
        Ok(Self { org_speed })
    }

    fn enable(&self, factor: f64) -> anyhow::Result<()> {
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
        set_mouse_speed(target_speed, false)?;
        Ok(())
    }

    fn disable(&self) -> anyhow::Result<()> {
        set_mouse_speed(self.org_speed, false)?;
        Ok(())
    }
}

impl Drop for MouseSpeedMod {
    fn drop(&mut self) {
        self.disable().unwrap();
    }
}
