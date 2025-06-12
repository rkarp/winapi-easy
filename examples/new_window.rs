use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use num_enum::{
    FromPrimitive,
    IntoPrimitive,
};
use winapi_easy::messaging::{
    ThreadMessage,
    ThreadMessageLoop,
};
use winapi_easy::ui::menu::{
    SubMenu,
    SubMenuItem,
    TextMenuItem,
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
    WindowShowState,
    WindowStyle,
};

#[derive(FromPrimitive, IntoPrimitive, Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
enum MenuID {
    None,
    HideWindow,
    ShowWindow,
    ShowBalloonNotification,
    ShowMessageBox,
    #[num_enum(catch_all)]
    Other(u32),
}

fn main() -> io::Result<()> {
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
    let class_appearance = WindowClassAppearance {
        background_brush: Some(Brush::from(BuiltinColor::AppWorkspace).into()),
        icon: Some(Rc::clone(&icon)),
        ..Default::default()
    };
    let class: WindowClass = WindowClass::register_new("myclass1", class_appearance)?;
    let window_appearance = WindowAppearance {
        style: WindowStyle::OverlappedWindow,
        ..Default::default()
    };
    let mut window = Window::new_layered::<_, ()>(
        class.into(),
        Some(listener),
        "mywindow1",
        window_appearance,
        None,
    )?;
    window.set_layered_opacity_alpha(u8::MAX)?;
    let notification_icon_id = NotificationIconId::Simple(0);
    let notification_icon_options = NotificationIconOptions {
        icon_id: notification_icon_id,
        icon,
        tooltip_text: Some("A tooltip!".to_string()),
        visible: true,
    };
    let _ = window.add_notification_icon(notification_icon_options)?;
    let window_handle = window.as_handle();
    window_handle.set_caption_text("My Window")?;
    window_handle.set_show_state(WindowShowState::Show)?;

    let messsage_box_item = SubMenuItem::Text(TextMenuItem::default_with_text(
        MenuID::ShowMessageBox.into(),
        "Show message box",
    ));
    let mut submenu = SubMenu::new()?;
    submenu.insert_menu_item(messsage_box_item.clone(), None)?;
    let submenu = Rc::new(RefCell::new(submenu));
    let popup = SubMenu::new_from_items([
        SubMenuItem::Text(TextMenuItem::default_with_text(
            MenuID::ShowWindow.into(),
            "Show window",
        )),
        SubMenuItem::Text(TextMenuItem::default_with_text(
            MenuID::HideWindow.into(),
            "Hide window",
        )),
        SubMenuItem::Text(TextMenuItem::default_with_text(
            MenuID::ShowBalloonNotification.into(),
            "Show balloon notification",
        )),
        messsage_box_item,
        SubMenuItem::Text(TextMenuItem {
            sub_menu: Some(submenu),
            ..TextMenuItem::default_with_text(MenuID::None.into(), "Submenu")
        }),
    ])?;

    let loop_callback = |thread_message| match thread_message {
        ThreadMessage::WindowProc(window_message)
            if window_message.window_handle == window_handle =>
        {
            match window_message.variant {
                ListenerMessageVariant::MenuCommand { selected_item_id } => {
                    match selected_item_id.into() {
                        MenuID::None => (),
                        MenuID::ShowWindow => {
                            window_handle.set_show_state(WindowShowState::Show)?;
                        }
                        MenuID::HideWindow => {
                            window_handle.set_show_state(WindowShowState::Hide)?;
                        }
                        MenuID::ShowBalloonNotification => {
                            let notification = BalloonNotification {
                                title: "A notification",
                                body: "Lorem ipsum",
                                ..Default::default()
                            };
                            window
                                .get_notification_icon(notification_icon_id)
                                .set_balloon_notification(Some(notification))?;
                        }
                        MenuID::ShowMessageBox => {
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
                        MenuID::Other(_) => unreachable!(),
                    }
                }
                ListenerMessageVariant::NotificationIconSelect { icon_id, xy_coords } => {
                    println!(
                        "Selected notification icon id: {}, coords: ({}, {})",
                        icon_id, xy_coords.x, xy_coords.y
                    );
                    window_handle.set_show_state(WindowShowState::ShowNormal)?;
                    let _ = window_handle.set_as_foreground();
                }
                ListenerMessageVariant::NotificationIconContextSelect { icon_id, xy_coords } => {
                    println!(
                        "Context-selected notification icon id: {}, coords: ({}, {})",
                        icon_id, xy_coords.x, xy_coords.y
                    );
                    let _ = window_handle.set_as_foreground();
                    popup.show_menu(window_handle, xy_coords)?;
                }
                _ => (),
            }
            Ok(())
        }
        ThreadMessage::Other(_) => Ok(()),
        _ => Ok(()),
    };
    ThreadMessageLoop::new().run_with(loop_callback)?;
    Ok(())
}
