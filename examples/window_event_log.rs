use std::io;

use winapi_easy::hooking::{
    WinEventHook,
    WinEventKind,
    WinEventMessage,
};
use winapi_easy::ui::Rectangle;
use winapi_easy::ui::window::WindowHandle;

fn main() -> io::Result<()> {
    #[expect(dead_code)]
    #[derive(Debug)]
    struct MessageContent {
        kind: WinEventKind,
        caption: Option<String>,
        client_area: Option<Rectangle>,
    }
    let callback = |message: WinEventMessage| {
        match message.event_kind {
            WinEventKind::ForegroundWindowChanged
            | WinEventKind::WindowUnminimized
            | WinEventKind::WindowMinimized
            | WinEventKind::WindowMoveStart
            | WinEventKind::WindowMoveEnd => {
                let window_handle = message.window_handle;
                let message_content = MessageContent {
                    kind: message.event_kind,
                    caption: window_handle.map(WindowHandle::get_caption_text),
                    client_area: window_handle
                        .map(WindowHandle::get_client_area_coords)
                        .transpose()
                        .ok()
                        .flatten(),
                };
                println!("{:#?}", message_content);
            }
            _ => (),
        };
    };
    WinEventHook::run_hook_loop(callback)?;
    Ok(())
}
