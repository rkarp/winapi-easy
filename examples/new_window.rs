use std::io;

use std::cell::Cell;
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
    WindowClass,
    WindowHandle,
    WindowShowState,
};

#[derive(Copy, Clone)]
enum MyMessage {
    IconLeftClicked,
    IconRightClicked,
}

struct MyListener {
    message: Cell<Option<MyMessage>>,
}

impl WindowMessageListener for MyListener {
    fn handle_window_destroy(&self, _: &WindowHandle) {
        ThreadMessageLoop::post_quit_message();
    }
    fn handle_notification_icon_select(&self, icon_id: u16) {
        println!("Selected notification icon id: {}", icon_id);
        self.message.replace(Some(MyMessage::IconLeftClicked));
    }
    fn handle_notification_icon_context_select(&self, icon_id: u16) {
        println!("Context-selected notification icon id: {}", icon_id);
        self.message.replace(Some(MyMessage::IconRightClicked));
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
        WindowClass::register_new("myclass1", &background, &icon, &cursor)?;
    let window = Window::create_new(&class, &listener, "mywindow1")?;
    let _notification_icon =
        window.add_notification_icon(Default::default(), Some(&icon), Some("A tooltip!"));
    let handle = window.as_ref();
    handle.set_show_state(WindowShowState::Show)?;
    let loop_callback = || {
        if let Some(message) = listener.message.take() {
            let window_handle = window.as_ref();
            match message {
                MyMessage::IconLeftClicked => {
                    window_handle.set_show_state(WindowShowState::Show)?;
                }
                MyMessage::IconRightClicked => {
                    window_handle.set_show_state(WindowShowState::Hide)?;
                }
            }
        }
        Ok(())
    };
    ThreadMessageLoop::run_thread_message_loop(loop_callback)?;
    //std::thread::sleep_ms(10000);
    Ok(())
}
