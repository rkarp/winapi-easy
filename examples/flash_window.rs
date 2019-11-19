use std::io;
use winapi_easy::ui::Window;

fn main() -> io::Result<()> {
    let maybe_window = Window::get_console_window();
    if let Some(mut window) = maybe_window {
        window.flash();
    } else {
        eprintln!("No console window found!");
    }
    Ok(())
}
