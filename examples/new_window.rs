use std::io;

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
    NotificationIconId,
    Window,
    WindowAction,
    WindowClass,
    WindowHandle,
};

struct MyListener {}

impl WindowMessageListener for MyListener {
    fn handle_window_destroy(&mut self, _: &WindowHandle) {
        ThreadMessageLoop::post_quit_message();
    }
    fn handle_notification_icon_select(&mut self, icon_id: u16) {
        println!("Selected notification icon id: {}", icon_id);
    }
    fn handle_notification_icon_context_select(&mut self, icon_id: u16) {
        println!("Context-selected notification icon id: {}", icon_id);
    }
}

fn main() -> io::Result<()> {
    let mut listener = MyListener {};
    let background: BuiltinColor = BuiltinColor::AppWorkspace;
    let icon: BuiltinIcon = Default::default();
    let cursor: BuiltinCursor = Default::default();
    let class: WindowClass<MyListener, _> =
        WindowClass::register_new("myclass1", &background, &icon, &cursor)?;
    let window = Window::create_new(&class, &mut listener, "mywindow1")?;
    let _notification_icon = window.add_notification_icon(
        NotificationIconId::Simple(0),
        Some(&icon),
        Some("A tooltip!"),
    );
    let handle = window.as_ref();
    handle.perform_action(WindowAction::Restore)?;
    ThreadMessageLoop::run_thread_message_loop()?;
    //std::thread::sleep_ms(10000);
    Ok(())
}
