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

fn main() -> io::Result<()> {
    let target_window_setting: Mutex<Option<WindowHandle>> = Default::default();
    let hotkey_thread_id: OnceLock<ThreadId> = OnceLock::new();
    thread::scope(|scope| {
        let hotkey_thread =
            scope.spawn(|| hotkey_set_target_window(&target_window_setting, &hotkey_thread_id));
        main_thread(&target_window_setting)?;
        if let Some(thread_id) = hotkey_thread_id.get() {
            thread_id.post_quit_message()?;
        }
        hotkey_thread.join().unwrap()
    })
}

fn main_thread(target_window_setting: &Mutex<Option<WindowHandle>>) -> io::Result<()> {
    set_dpi_awareness_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE)?;

    let monitor = MonitorHandle::from_window(WindowHandle::get_desktop_window()?);
    let monitor_info = monitor.info()?;

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
    let host_class_appearance = WindowClassAppearance {
        background_brush: Some(Brush::from(BuiltinColor::InfoBlack).into()),
        icon: Some(Rc::clone(&icon)),
        ..Default::default()
    };
    let host_class: WindowClass =
        WindowClass::register_new("MagnifierHostclass", host_class_appearance)?;
    let host_window_appearance = WindowAppearance {
        style: WindowStyle::Popup,
        extended_style: WindowExtendedStyle::Transparent,
    };
    let mut host_window = Window::new_layered::<_, ()>(
        host_class.into(),
        listener,
        "MagnifierHost",
        host_window_appearance,
        None,
    )?;
    let host_window_handle = *host_window;

    let notification_icon_id = NotificationIconId::Simple(0);
    let notification_icon_options = NotificationIconOptions {
        icon_id: notification_icon_id,
        icon,
        tooltip_text: Some("Magnifier".to_string()),
        visible: true,
    };
    let _ = host_window.add_notification_icon(notification_icon_options)?;

    host_window_handle.set_caption_text("Magnifier")?;

    {
        let mut placement = host_window_handle.get_placement()?;
        placement.set_show_state(WindowShowState::Hide);
        placement.set_normal_position(monitor_info.monitor_area);
        host_window_handle.set_placement(&placement)?;
    }
    host_window.set_layered_opacity_alpha(u8::MAX)?;
    host_window_handle.set_timer(0, 1000 / 60)?;

    let host_window = Rc::new(RefCell::new(host_window));

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

    let mut magnifier_active = false;
    let mut cursor_hider: Option<CursorConcealment> = None;
    let mut cursor_confinement: Option<CursorConfinement> = None;

    let loop_callback = || {
        if let Some(message) = listener_data_clone.take() {
            match message.variant {
                ListenerMessageVariant::MenuCommand { selected_item_id } => {
                    match selected_item_id.into() {
                        MenuID::Exit => host_window_handle.send_command(WindowCommand::Close)?,
                        MenuID::Other(_) => unreachable!(),
                    }
                }
                ListenerMessageVariant::NotificationIconContextSelect { xy_coords, .. } => {
                    let _ = host_window_handle.set_as_foreground();
                    popup.show_menu(host_window_handle, xy_coords)?;
                }
                ListenerMessageVariant::Timer { timer_id: 0 } => {
                    let disable;
                    if let Some(foreground_window) = WindowHandle::get_foreground_window() {
                        if target_window_setting
                            .lock()
                            .unwrap()
                            .is_some_and(|target_window| target_window == foreground_window)
                        {
                            if !magnifier_active {
                                cursor_hider = Some(CursorConcealment::new()?);
                                host_window_handle.set_show_state(WindowShowState::Show)?;
                                host_window_handle.set_z_position(WindowZPosition::TopMost)?;
                                magnifier_active = true;
                            }
                            let source_window_rect = foreground_window.get_client_area_coords()?;
                            let scaling_result = ScalingResult::from_rects(
                                source_window_rect,
                                monitor_info.monitor_area,
                            );
                            {
                                let mut placement = magnifier_window.get_placement()?;
                                placement
                                    .set_normal_position(scaling_result.max_scaled_rect_centered());
                                magnifier_window.set_placement(&placement)?;
                            }
                            magnifier_window
                                .set_magnification_factor(scaling_result.max_scale_factor as f32)?;
                            magnifier_window.set_magnification_source(source_window_rect)?;
                            cursor_confinement = Some(CursorConfinement::new(source_window_rect)?);
                            disable = false;
                        } else {
                            disable = true;
                        }
                    } else {
                        disable = true;
                    };
                    if disable && magnifier_active {
                        cursor_hider = None;
                        cursor_confinement = None;
                        host_window_handle.set_z_position(WindowZPosition::Bottom)?;
                        host_window_handle.set_show_state(WindowShowState::Hide)?;
                        magnifier_active = false;
                    }
                }
                ListenerMessageVariant::Other => (),
                _ => (),
            }
        }
        Ok(())
    };
    ThreadMessageLoop::run_thread_message_loop(loop_callback)?;
    Ok(())
}

fn hotkey_set_target_window(
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
