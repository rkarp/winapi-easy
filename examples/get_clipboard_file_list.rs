use std::io;
use winapi_easy::clipboard::Clipboard;

fn main() -> io::Result<()> {
    Clipboard::new()?
        .get_file_list()?
        .into_iter()
        .for_each(|name| {
            println!("{:#?}", name);
        });
    Ok(())
}
