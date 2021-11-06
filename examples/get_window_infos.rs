use std::io;
use winapi_easy::ui::{
    Rectangle,
    WindowHandle,
    WindowShowState,
};

#[derive(Debug)]
#[allow(dead_code)]
struct WindowInfo {
    caption: String,
    class_name: String,
    show_state: WindowShowState,
    restored_position: Rectangle,
}

fn main() -> io::Result<()> {
    WindowHandle::get_toplevel_windows()?
        .into_iter()
        .filter(|window| window.is_visible())
        .map(|window| {
            let placement = window.get_placement()?;
            Ok(WindowInfo {
                caption: window.get_caption_text(),
                class_name: window.get_class_name()?,
                show_state: placement.get_show_state().unwrap(),
                restored_position: placement.get_restored_position(),
            })
        })
        .try_for_each(|info: io::Result<WindowInfo>| -> io::Result<()> {
            println!("{:#?}", info?);
            Ok(())
        })?;
    Ok(())
}
