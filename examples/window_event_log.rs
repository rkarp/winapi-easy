use std::io;

use winapi_easy::hooking::{
    WinEventHook,
    WinEventKind,
    WinEventMessage,
};
use winapi_easy::ui::Rectangle;

fn main() -> io::Result<()> {
    #[expect(dead_code)]
    #[derive(Debug)]
    struct MessageContent {
        kind: WinEventKind,
        caption: String,
        client_area: Rectangle,
    }
    let callback = |message: WinEventMessage| {
        match message.event_kind {
            WinEventKind::ForegroundWindowChanged
            | WinEventKind::WindowUnminimized
            | WinEventKind::WindowMinimized
            | WinEventKind::WindowMoveStart
            | WinEventKind::WindowMoveEnd => {
                let window_handle = message.window_handle.unwrap();
                let message_content = MessageContent {
                    kind: message.event_kind,
                    caption: window_handle.get_caption_text(),
                    client_area: window_handle.get_client_area_coords().unwrap(),
                };
                println!("{:#?}", message_content);
            }
            _ => (),
        };
    };
    WinEventHook::run_hook_loop(callback)?;
    Ok(())
}
