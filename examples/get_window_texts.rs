use std::io;
use winapi_easy::ui::Window;

fn main() -> io::Result<()> {
    Window::get_toplevel_windows()?
        .into_iter()
        .filter(|window| window.is_visible())
        .map(|window| window.get_caption_text())
        .for_each(|caption| println!("{}", caption));
    Ok(())
}
