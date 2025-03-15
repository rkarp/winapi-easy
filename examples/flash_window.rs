use std::{
    io,
    thread,
};
use std::time::Duration;

use winapi_easy::ui::window::WindowHandle;
use winapi_easy::ui::taskbar::{
    ProgressState,
    Taskbar,
};

fn main() -> io::Result<()> {
    let maybe_window = WindowHandle::get_console_window();
    if let Some(window) = maybe_window {
        let taskbar = Taskbar::new()?;
        taskbar.set_progress_state(&window, ProgressState::Indeterminate)?;
        thread::sleep(Duration::from_millis(3000));
        taskbar.set_progress_state(&window, ProgressState::NoProgress)?;

        window.flash();
    } else {
        eprintln!("No console window found!");
    }
    Ok(())
}
