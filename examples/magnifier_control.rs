//! An example magnifier app that will automatically magnify the foreground window on hotkey Ctrl + Alt + Shift + F.
//!
//! Exit via notification icon command.
use std::cell::{
    Cell,
    RefCell,
};
use std::rc::Rc;
use std::sync::{
    Mutex,
    OnceLock,
};
use std::{
    io,
    thread,
};

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::input::KeyboardKey;
use winapi_easy::input::hotkeys::{
    GlobalHotkeySet,
    Modifier,
};
use winapi_easy::messaging::ThreadMessageLoop;
use winapi_easy::process::ThreadId;
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
    TargetWindowChanged,
    #[num_enum(catch_all)]
    Other(u8),
}

#[derive(Default, Debug)]
struct MagnifierState {
    magnifier_active: bool,
    cursor_hider: Option<CursorConcealment>,
    cursor_confinement: Option<CursorConfinement>,
}

fn main() -> io::Result<()> {
    set_dpi_awareness_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE)?;

    let target_window_setting: Mutex<Option<WindowHandle>> = Default::default();
    let hotkey_thread_id: OnceLock<ThreadId> = OnceLock::new();

    thread::scope(|scope| {
        let listener_data: Rc<Cell<Option<ListenerMessage>>> = Rc::new(None.into());
        let listener_data_clone = listener_data.clone();
        let listener = move |message: ListenerMessage| {
            let answer;
            match message.variant {
                ListenerMessageVariant::WindowDestroy => {
                    ThreadMessageLoop::post_quit_message();
                    answer = ListenerAnswer::CallDefaultHandler
                }
                _ => answer = ListenerAnswer::default(),
            }
            listener_data.replace(Some(message));
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
            listener,
            "MainWindow",
            Default::default(),
            None,
        )?;

        let hotkey_thread = {
            let main_window_handle = *main_window;
            let hotkey_thread_id = &hotkey_thread_id;
            let target_window_setting = &target_window_setting;
            scope.spawn(move || {
                hotkey_set_target_window(
                    main_window_handle,
                    target_window_setting,
                    hotkey_thread_id,
                )
            })
        };

        let _ = main_window.add_notification_icon(NotificationIconOptions {
            icon_id: NotificationIconId::Simple(0),
            icon: Rc::clone(&icon),
            tooltip_text: Some("Magnifier".to_string()),
            visible: true,
        })?;

        let monitor = MonitorHandle::from_window(WindowHandle::get_desktop_window()?);
        let monitor_info = monitor.info()?;

        let host_class: WindowClass = WindowClass::register_new(
            "MagnifierHostclass",
            WindowClassAppearance {
                background_brush: Some(Brush::from(BuiltinColor::InfoBlack).into()),
                icon: Some(Rc::clone(&icon)),
                ..Default::default()
            },
        )?;

        let host_control_window = Window::new_layered::<_, ()>(
            host_class.into(),
            |_| Default::default(),
            "MagnifierHost",
            WindowAppearance {
                style: WindowStyle::Popup,
                extended_style: WindowExtendedStyle::Transparent,
            },
            None,
        )?;
        let host_control_window_handle = *host_control_window;

        host_control_window_handle.set_caption_text("Magnifier")?;

        {
            let mut placement = host_control_window_handle.get_placement()?;
            placement.set_show_state(WindowShowState::Hide);
            placement.set_normal_position(monitor_info.monitor_area);
            host_control_window_handle.set_placement(&placement)?;
        }
        host_control_window.set_layered_opacity_alpha(u8::MAX)?;

        let host_window = Rc::new(RefCell::new(host_control_window));

        let magnifier_window_appearance = WindowAppearance {
            style: WindowStyle::Child | WindowStyle::Visible,
            extended_style: Default::default(),
        };
        let magnifier_window =
            Window::new_magnifier("MagnifierView", magnifier_window_appearance, host_window)?;
        magnifier_window.set_lens_use_bitmap_smoothing(true)?;
        magnifier_window.set_show_state(WindowShowState::Show)?;

        let mut popup = SubMenu::new()?;
        popup.insert_menu_item(
            SubMenuItem::Text(TextMenuItem::default_with_text(MenuID::Exit.into(), "Exit")),
            None,
        )?;

        let mut magnifier_state = MagnifierState::default();

        let loop_callback = || {
            if let Some(message) = listener_data_clone.take() {
                let set_magnifier_control = |magnifier_state: &mut MagnifierState,
                                             enable: bool|
                 -> io::Result<()> {
                    if magnifier_state.magnifier_active != enable {
                        if enable {
                            magnifier_state.cursor_hider = Some(CursorConcealment::new()?);
                            host_control_window_handle.set_show_state(WindowShowState::Show)?;
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

                match message.variant {
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
                        if let Some(foreground_window) = WindowHandle::get_foreground_window() {
                            if target_window_setting
                                .lock()
                                .unwrap()
                                .is_some_and(|target_window| target_window == foreground_window)
                            {
                                set_magnifier_control(&mut magnifier_state, true)?;
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
                            } else {
                                set_magnifier_control(&mut magnifier_state, false)?;
                            }
                        } else {
                            set_magnifier_control(&mut magnifier_state, false)?;
                        };
                    }
                    ListenerMessageVariant::CustomUserMessage(custom_message) => {
                        if custom_message.message_id == UserMessageId::TargetWindowChanged.into() {
                            let target_window_setting_lock = target_window_setting.lock().unwrap();
                            if let Some(_window_handle) = *target_window_setting_lock {
                                main_window.set_timer(0, 1000 / 60)?;
                            } else {
                                set_magnifier_control(&mut magnifier_state, false)?;
                                let _ = main_window.kill_timer(0);
                            }
                        }
                    }
                    ListenerMessageVariant::Other => (),
                    _ => (),
                }
            }
            Ok(())
        };
        ThreadMessageLoop::run_thread_message_loop(loop_callback)?;

        if let Some(thread_id) = hotkey_thread_id.get() {
            thread_id.post_quit_message()?;
        }
        hotkey_thread.join().unwrap()
    })
}

fn hotkey_set_target_window(
    main_window: WindowHandle,
    target_window_setting: &Mutex<Option<WindowHandle>>,
    hotkey_thread_id: &OnceLock<ThreadId>,
) -> io::Result<()> {
    hotkey_thread_id.get_or_init(|| ThreadId::current());
    let hotkey_def = GlobalHotkeySet::new().add_hotkey(
        0,
        Modifier::Ctrl + Modifier::Alt + Modifier::Shift + KeyboardKey::F,
    );
    for event in hotkey_def.listen_for_hotkeys()? {
        if event? == 0 {
            let foreground_window = WindowHandle::get_foreground_window();
            let mut target_window_setting_lock = target_window_setting.lock().unwrap();
            if target_window_setting_lock.is_some() {
                *target_window_setting_lock = None;
            } else {
                *target_window_setting_lock = foreground_window;
            }
            main_window.send_user_message(CustomUserMessage {
                message_id: UserMessageId::TargetWindowChanged.into(),
                ..Default::default()
            })?
        } else {
            unreachable!()
        }
    }
    Ok(())
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
