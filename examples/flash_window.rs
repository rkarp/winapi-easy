use std::io;
use winapi_easy::ui::WindowHandle;

fn main() -> io::Result<()> {
    let maybe_window = WindowHandle::get_console_window();
    if let Some(mut window) = maybe_window {
        window.flash();
    } else {
        eprintln!("No console window found!");
    }
    Ok(())
}
