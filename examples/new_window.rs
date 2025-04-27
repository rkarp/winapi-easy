use std::cell::Cell;
use std::io;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::messaging::ThreadMessageLoop;
use winapi_easy::ui::Point;
use winapi_easy::ui::menu::{
    MenuItem,
    PopupMenu,
};
use winapi_easy::ui::message_box::{
    MessageBoxOptions,
    show_message_box,
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
    BalloonNotification,
    NotificationIconId,
    NotificationIconOptions,
    Window,
    WindowAppearance,
    WindowClass,
    WindowClassAppearance,
    WindowKind,
    WindowShowState,
    WindowStyle,
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

fn main() -> io::Result<()> {
    let listener_data: Cell<Option<MyMessage>> = None.into();
    let listener = |message: ListenerMessage| match message.variant {
        ListenerMessageVariant::MenuCommand { selected_item_id } => {
            listener_data.replace(Some(MyMessage::MenuItem(selected_item_id.into())));
            ListenerAnswer::MessageProcessed
        }
        ListenerMessageVariant::WindowDestroy => {
            ThreadMessageLoop::post_quit_message();
            ListenerAnswer::CallDefaultHandler
        }
        ListenerMessageVariant::NotificationIconSelect { icon_id, xy_coords } => {
            println!(
                "Selected notification icon id: {}, coords: ({}, {})",
                icon_id, xy_coords.x, xy_coords.y
            );
            listener_data.replace(Some(MyMessage::IconLeftClicked(xy_coords)));
            ListenerAnswer::MessageProcessed
        }
        ListenerMessageVariant::NotificationIconContextSelect { icon_id, xy_coords } => {
            println!(
                "Context-selected notification icon id: {}, coords: ({}, {})",
                icon_id, xy_coords.x, xy_coords.y
            );
            listener_data.replace(Some(MyMessage::IconRightClicked(xy_coords)));
            ListenerAnswer::MessageProcessed
        }
        ListenerMessageVariant::Other => ListenerAnswer::default(),
        _ => ListenerAnswer::default(),
    };

    let icon: Icon = Default::default();
    let class_appearance = WindowClassAppearance {
        background_brush: Some(Brush::BuiltinColor(BuiltinColor::AppWorkspace)),
        icon: Some(icon.clone()),
        ..Default::default()
    };
    let class: WindowClass = WindowClass::register_new("myclass1", class_appearance)?;
    let window_appearance = WindowAppearance {
        style: WindowStyle::OverlappedWindow,
        ..Default::default()
    };
    let mut window = Window::create_new(&class, listener, "mywindow1", window_appearance, None)?;
    let notification_icon_id = NotificationIconId::Simple(0);
    let notification_icon_options = NotificationIconOptions {
        icon_id: notification_icon_id,
        icon: Some(icon),
        tooltip_text: Some("A tooltip!".to_string()),
        visible: true,
    };
    let _ = window.add_notification_icon(notification_icon_options)?;
    let window_handle = window.handle();
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
        if let Some(message) = listener_data.take() {
            let window_handle = window.handle();
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
                    window
                        .get_notification_icon(notification_icon_id)
                        .set_balloon_notification(Some(notification))?;
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
