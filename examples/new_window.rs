use std::cell::Cell;
use std::io;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::messaging::ThreadMessageLoop;
use winapi_easy::ui::Point;
use winapi_easy::ui::window::{
    BalloonNotification,
    NotificationIconOptions,
    Window,
    WindowClass,
    WindowClassAppearance,
    WindowHandle,
    WindowShowState,
};
use winapi_easy::ui::menu::{
    MenuItem,
    PopupMenu,
};
use winapi_easy::ui::message_box::{
    MessageBoxOptions,
    show_message_box,
};
use winapi_easy::ui::messaging::WindowMessageListener;
use winapi_easy::ui::resource::{
    BuiltinColor,
    BuiltinIcon,
};

#[derive(Copy, Clone, Debug)]
enum MyMessage {
    IconLeftClicked(Point),
    IconRightClicked(Point),
    MenuItem(MenuID),
}

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
enum MenuID {
    HideWindow,
    ShowWindow,
    ShowBalloonNotification,
    ShowMessageBox,
    #[num_enum(catch_all)]
    Other(u32),
}

struct MyListener {
    message: Cell<Option<MyMessage>>,
}

impl WindowMessageListener for MyListener {
    fn handle_menu_command(&self, _window: &WindowHandle, selected_item_id: u32) {
        self.message
            .replace(Some(MyMessage::MenuItem(selected_item_id.into())));
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
    let icon: BuiltinIcon = Default::default();
    let appearance = WindowClassAppearance {
        background_brush: Some(BuiltinColor::AppWorkspace),
        icon: Some(icon),
        ..Default::default()
    };
    let class: WindowClass<MyListener> = WindowClass::register_new("myclass1", appearance)?;
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
        MenuItem::Text("Show window"),
        MenuID::ShowWindow.into(),
        None,
    )?;
    popup.insert_menu_item(
        MenuItem::Text("Hide window"),
        MenuID::HideWindow.into(),
        None,
    )?;
    popup.insert_menu_item(
        MenuItem::Text("Show balloon notification"),
        MenuID::ShowBalloonNotification.into(),
        None,
    )?;
    popup.insert_menu_item(
        MenuItem::Text("Show message box"),
        MenuID::ShowMessageBox.into(),
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
                MyMessage::MenuItem(MenuID::ShowWindow) => {
                    window_handle.set_show_state(WindowShowState::Show)?;
                }
                MyMessage::MenuItem(MenuID::HideWindow) => {
                    window_handle.set_show_state(WindowShowState::Hide)?;
                }
                MyMessage::MenuItem(MenuID::ShowBalloonNotification) => {
                    let notification = BalloonNotification {
                        title: "A notification",
                        body: "Lorem ipsum",
                        ..Default::default()
                    };
                    notification_icon.set_balloon_notification(Some(notification))?;
                }
                MyMessage::MenuItem(MenuID::ShowMessageBox) => {
                    show_message_box(
                        window_handle,
                        MessageBoxOptions {
                            message: Some("Message"),
                            caption: Some("Caption"),
                            buttons: Default::default(),
                            icon: Some(Default::default()),
                            ..Default::default()
                        },
                    )?;
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
