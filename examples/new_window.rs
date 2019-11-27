use std::io;

use winapi_easy::ui::message::{
    post_quit_message,
    run_thread_message_loop,
    WindowMessageListener,
};
use winapi_easy::ui::{
    BuiltinColor,
    BuiltinCursor,
    BuiltinIcon,
    Window,
    WindowAction,
    WindowClass,
    WindowHandle,
};

struct MyListener {}

impl WindowMessageListener for MyListener {
    fn handle_window_destroy(&mut self, _: WindowHandle) {
        post_quit_message();
    }
}

fn main() -> io::Result<()> {
    let mut listener = MyListener {};
    let background: BuiltinColor = BuiltinColor::AppWorkspace;
    let icon: BuiltinIcon = Default::default();
    let cursor: BuiltinCursor = Default::default();
    let class: WindowClass<MyListener> =
        WindowClass::register_new("myclass1", &background, &icon, &cursor)?;
    let mut window = Window::create_new(&class, &mut listener, "mywindow1")?;
    let handle = window.as_mut();
    handle.perform_action(WindowAction::Restore)?;
    run_thread_message_loop()?;
    //std::thread::sleep_ms(10000);
    Ok(())
}
