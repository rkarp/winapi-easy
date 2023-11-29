use std::io;

use std::cell::Cell;
use winapi_easy::ui::menu::{
    PopupMenu,
    SubMenuItem,
};
use winapi_easy::ui::message::{
    ThreadMessageLoop,
    WindowMessageListener,
};
use winapi_easy::ui::resource::{
    BuiltinColor,
    BuiltinCursor,
    BuiltinIcon,
};
use winapi_easy::ui::{
    BalloonNotification,
    NotificationIconOptions,
    Point,
    Window,
    WindowClass,
    WindowHandle,
    WindowShowState,
};

#[derive(Copy, Clone, Debug)]
enum MyMessage {
    IconLeftClicked(Point),
    IconRightClicked(Point),
    MenuItem(u32),
}

impl MyMessage {
    const HIDE_WINDOW: u32 = 1;
    const SHOW_WINDOW: u32 = 2;
    const SHOW_BALLOON_NOTIFICATION: u32 = 3;
}

struct MyListener {
    message: Cell<Option<MyMessage>>,
}

impl WindowMessageListener for MyListener {
    fn handle_menu_command(&self, _window: &WindowHandle, selected_item_id: u32) {
        self.message
            .replace(Some(MyMessage::MenuItem(selected_item_id)));
    }

    fn handle_window_destroy(&self, _: &WindowHandle) {
        ThreadMessageLoop::post_quit_message();
    }
    fn handle_notification_icon_select(&self, icon_id: u16, xy_coords: Point) {
        println!(
            "Selected notification icon id: {}, coords: ({}, {})",
            icon_id, xy_coords.x, xy_coords.y
        );
        self.message
            .replace(Some(MyMessage::IconLeftClicked(xy_coords)));
    }
    fn handle_notification_icon_context_select(&self, icon_id: u16, xy_coords: Point) {
        println!(
            "Context-selected notification icon id: {}, coords: ({}, {})",
            icon_id, xy_coords.x, xy_coords.y
        );
        self.message
            .replace(Some(MyMessage::IconRightClicked(xy_coords)));
    }
}

fn main() -> io::Result<()> {
    let listener = MyListener {
        message: None.into(),
    };
    let background: BuiltinColor = BuiltinColor::AppWorkspace;
    let icon: BuiltinIcon = Default::default();
    let cursor: BuiltinCursor = Default::default();
    let class: WindowClass<MyListener, _> =
        WindowClass::register_new("myclass1", &background, icon, &cursor)?;
    let window = Window::create_new(&class, &listener, "mywindow1")?;
    let notification_icon_options = NotificationIconOptions {
        icon: Some(icon),
        tooltip_text: Some("A tooltip!"),
        visible: true,
        ..Default::default()
    };
    let mut notification_icon = window.add_notification_icon(notification_icon_options)?;
    let window_handle = window.as_ref();
    window_handle.set_caption_text("My Window")?;
    window_handle.set_show_state(WindowShowState::Show)?;
    let popup = PopupMenu::new()?;
    popup.insert_menu_item(
        SubMenuItem::Text("Show window"),
        MyMessage::SHOW_WINDOW,
        None,
    )?;
    popup.insert_menu_item(
        SubMenuItem::Text("Hide window"),
        MyMessage::HIDE_WINDOW,
        None,
    )?;
    popup.insert_menu_item(
        SubMenuItem::Text("Show balloon notification"),
        MyMessage::SHOW_BALLOON_NOTIFICATION,
        None,
    )?;
    let loop_callback = || {
        if let Some(message) = listener.message.take() {
            let window_handle = window.as_ref();
            match message {
                MyMessage::IconLeftClicked(_coords) => {
                    window_handle.set_show_state(WindowShowState::ShowNormal)?;
                    window_handle.set_as_foreground()?;
                }
                MyMessage::IconRightClicked(coords) => {
                    window_handle.set_as_foreground()?;
                    popup.show_popup_menu(window_handle, coords)?;
                }
                MyMessage::MenuItem(MyMessage::SHOW_WINDOW) => {
                    window_handle.set_show_state(WindowShowState::Show)?;
                }
                MyMessage::MenuItem(MyMessage::HIDE_WINDOW) => {
                    window_handle.set_show_state(WindowShowState::Hide)?;
                }
                MyMessage::MenuItem(MyMessage::SHOW_BALLOON_NOTIFICATION) => {
                    let notification = BalloonNotification {
                        title: "A notification",
                        body: "Lorem ipsum",
                        ..Default::default()
                    };
                    notification_icon.set_balloon_notification(Some(notification))?;
                }
                MyMessage::MenuItem(_) => panic!(),
            }
        }
        Ok(())
    };
    ThreadMessageLoop::run_thread_message_loop(loop_callback)?;
    //std::thread::sleep_ms(10000);
    Ok(())
}
