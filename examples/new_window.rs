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
    Window,
    WindowAction,
    WindowClass,
    WindowHandle,
};

struct MyListener {}

impl WindowMessageListener for MyListener {
    fn handle_window_destroy(&self, _: &WindowHandle) {
        ThreadMessageLoop::post_quit_message();
    }
    fn handle_notification_icon_select(&self, icon_id: u16) {
        println!("Selected notification icon id: {}", icon_id);
    }
    fn handle_notification_icon_context_select(&self, icon_id: u16) {
        println!("Context-selected notification icon id: {}", icon_id);
    }
}

fn main() -> io::Result<()> {
    let listener = MyListener {};
    let background: BuiltinColor = BuiltinColor::AppWorkspace;
    let icon: BuiltinIcon = Default::default();
    let cursor: BuiltinCursor = Default::default();
    let class: WindowClass<MyListener, _> =
        WindowClass::register_new("myclass1", &background, &icon, &cursor)?;
    let window = Window::create_new(&class, &listener, "mywindow1")?;
    let _notification_icon =
        window.add_notification_icon(Default::default(), Some(&icon), Some("A tooltip!"));
    let handle = window.as_ref();
    handle.perform_action(WindowAction::Restore)?;
    ThreadMessageLoop::run_thread_message_loop()?;
    //std::thread::sleep_ms(10000);
    Ok(())
}
