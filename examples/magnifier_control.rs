//! An example magnifier app that will automatically magnify the foreground window on hotkey Ctrl + Alt + Shift + F.
//!
//! Exit via notification icon command.
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::hooking::{
    WinEventHook,
    WinEventKind,
    WinEventMessage,
};
use winapi_easy::input::KeyboardKey;
use winapi_easy::input::hotkey::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::messaging::{
    ThreadMessage,
    ThreadMessageLoop,
};
use winapi_easy::ui::desktop::MonitorHandle;
use winapi_easy::ui::menu::{
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
    set_dpi_awareness_context,
};

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
enum MenuID {
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

#[derive(Default, Debug)]
struct MagnifierOptions {
    target_window_setting: Option<WindowHandle>,
}

#[derive(Default, Debug)]
struct MagnifierState {
    magnifier_active: bool,
    cursor_hider: Option<CursorConcealment>,
    cursor_confinement: Option<CursorConfinement>,
}

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
enum HotkeyId {
    SetTargetWindow,
    #[num_enum(catch_all)]
    Other(u8),
}

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
            "MainClass",
            WindowClassAppearance {
                icon: Some(Rc::clone(&icon)),
                ..Default::default()
            },
        )?
        .into(),
        Some(listener),
        "MainWindow",
        Default::default(),
        None,
    )?;

    let _ = main_window.add_notification_icon(NotificationIconOptions {
        icon_id: NotificationIconId::Simple(0),
        icon: Rc::clone(&icon),
        tooltip_text: Some("Magnifier".to_string()),
        visible: true,
    })?;

    let host_control_window = {
        let host_class: WindowClass = WindowClass::register_new(
            "MagnifierHostclass",
            WindowClassAppearance {
                background_brush: Some(Brush::from(BuiltinColor::InfoBlack).into()),
                icon: Some(Rc::clone(&icon)),
                ..Default::default()
            },
        )?;
        Window::new_layered::<DefaultWmlType, ()>(
            host_class.into(),
            None,
            "MagnifierHost",
            WindowAppearance {
                style: WindowStyle::Popup,
                extended_style: WindowExtendedStyle::Transparent,
            },
            None,
        )?
    };
    let host_control_window_handle = *host_control_window;

    host_control_window_handle.set_caption_text("Magnifier")?;
    host_control_window.set_layered_opacity_alpha(u8::MAX)?;

    let magnifier_window = Window::new_magnifier(
        "MagnifierView",
        WindowAppearance {
            style: WindowStyle::Child | WindowStyle::Visible,
            extended_style: Default::default(),
        },
        Rc::new(RefCell::new(host_control_window)),
    )?;
    magnifier_window.set_lens_use_bitmap_smoothing(true)?;
    magnifier_window.set_show_state(WindowShowState::Show)?;

    let mut popup = SubMenu::new()?;
    popup.insert_menu_item(
        SubMenuItem::Text(TextMenuItem::default_with_text(MenuID::Exit.into(), "Exit")),
        None,
    )?;

    let mut magnifier_options = MagnifierOptions::default();
    let mut magnifier_state = MagnifierState::default();

    let _hotkeys = setup_hotkeys()?;

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
        WinEventHook::new::<0>(win_event_listener)
    };

    let loop_callback = |thread_message| match thread_message {
        ThreadMessage::WindowProc(window_message)
            if window_message.window_handle == *main_window =>
        {
            let set_magnifier_control = |magnifier_state: &mut MagnifierState,
                                         enable: bool|
             -> io::Result<()> {
                if magnifier_state.magnifier_active != enable {
                    if enable {
                        magnifier_state.cursor_hider = Some(CursorConcealment::new()?);
                        host_control_window_handle.set_show_state(WindowShowState::Show)?;
                        // Possible Windows bug: The topmost setting may stop working if another window of the process
                        // was set as the foreground window. As a workaround we reset it first.
                        host_control_window_handle.set_z_position(WindowZPosition::NoTopMost)?;
                        host_control_window_handle.set_z_position(WindowZPosition::TopMost)?;
                    } else {
                        magnifier_state.cursor_hider = None;
                        magnifier_state.cursor_confinement = None;
                        host_control_window_handle.set_z_position(WindowZPosition::Bottom)?;
                        host_control_window_handle.set_show_state(WindowShowState::Hide)?;
                    }
                    magnifier_state.magnifier_active = enable;
                }
                Ok(())
            };

            match window_message.variant {
                ListenerMessageVariant::MenuCommand { selected_item_id } => {
                    match selected_item_id.into() {
                        MenuID::Exit => main_window.send_command(WindowCommand::Close)?,
                        MenuID::Other(_) => unreachable!(),
                    }
                }
                ListenerMessageVariant::NotificationIconContextSelect { xy_coords, .. } => {
                    let _ = main_window.set_as_foreground();
                    popup.show_menu(*main_window, xy_coords)?;
                }
                ListenerMessageVariant::Timer { timer_id: 0 } => {
                    magnifier_window.redraw()?;
                    if let Some(confinement) = &magnifier_state.cursor_confinement {
                        confinement.reapply()?;
                    }
                }
                ListenerMessageVariant::CustomUserMessage(custom_message) => {
                    let mut reinit_magnifier = || -> io::Result<()> {
                        let maybe_foreground_window = WindowHandle::get_foreground_window();
                        match maybe_foreground_window {
                            Some(foreground_window)
                                if maybe_foreground_window
                                    == magnifier_options.target_window_setting =>
                            {
                                set_magnifier_control(&mut magnifier_state, true)?;

                                let monitor_info =
                                    MonitorHandle::from_window(foreground_window).info()?;
                                {
                                    let mut placement =
                                        host_control_window_handle.get_placement()?;
                                    placement.set_normal_position(monitor_info.monitor_area);
                                    host_control_window_handle.set_placement(&placement)?;
                                }
                                let source_window_rect =
                                    foreground_window.get_client_area_coords()?;
                                let scaling_result = ScalingResult::from_rects(
                                    source_window_rect,
                                    monitor_info.monitor_area,
                                );
                                {
                                    let mut placement = magnifier_window.get_placement()?;
                                    placement.set_normal_position(
                                        scaling_result.max_scaled_rect_centered(),
                                    );
                                    magnifier_window.set_placement(&placement)?;
                                }
                                magnifier_window.set_magnification_factor(
                                    scaling_result.max_scale_factor as f32,
                                )?;
                                magnifier_window.set_magnification_source(source_window_rect)?;
                                magnifier_state.cursor_confinement =
                                    Some(CursorConfinement::new(source_window_rect)?);
                            }
                            _ => set_magnifier_control(&mut magnifier_state, false)?,
                        }
                        Ok(())
                    };
                    match UserMessageId::from(custom_message.message_id) {
                        UserMessageId::ForegroundWindowChanged => {
                            reinit_magnifier()?;
                        }
                        UserMessageId::TargetWindowChanged => {
                            if let Some(_window_handle) = magnifier_options.target_window_setting {
                                reinit_magnifier()?;
                                main_window.set_timer(0, 1000 / 60)?;
                            } else {
                                set_magnifier_control(&mut magnifier_state, false)?;
                                let _ = main_window.kill_timer(0);
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
    max_scale_factor: f64,
    max_scaled_rect: Rectangle,
    max_scaled_rect_centered_offset: Point,
}

impl ScalingResult {
    fn from_rects(source: Rectangle, target: Rectangle) -> Self {
        let source_width = source.right - source.left;
        let source_height = source.bottom - source.top;
        let target_width = target.right - target.left;
        let target_height = target.bottom - target.top;
        assert!(source_width > 0);
        assert!(source_height > 0);
        assert!(target_width > 0);
        assert!(target_height > 0);
        let max_width_scale = f64::from(target_width) / f64::from(source_width);
        let max_height_scale = f64::from(target_height) / f64::from(source_height);
        let max_scale_factor = f64::min(max_width_scale, max_height_scale);
        let max_scaled_rect = Rectangle {
            left: 0,
            top: 0,
            right: (f64::from(source_width) * max_scale_factor).round() as i32,
            bottom: (f64::from(source_height) * max_scale_factor).round() as i32,
        };
        Self {
            max_scale_factor,
            max_scaled_rect,
            max_scaled_rect_centered_offset: Point {
                x: (target_width - max_scaled_rect.right) / 2,
                y: (target_height - max_scaled_rect.bottom) / 2,
            },
        }
    }

    fn max_scaled_rect_centered(&self) -> Rectangle {
        Rectangle {
            left: self.max_scaled_rect.left + self.max_scaled_rect_centered_offset.x,
            top: self.max_scaled_rect.top + self.max_scaled_rect_centered_offset.y,
            right: self.max_scaled_rect.right + self.max_scaled_rect_centered_offset.x,
            bottom: self.max_scaled_rect.bottom + self.max_scaled_rect_centered_offset.y,
        }
    }
}
