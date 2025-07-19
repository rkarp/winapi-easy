#![allow(clippy::uninlined_format_args)]

use std::io;

use winapi_easy::clipboard;

fn main() -> io::Result<()> {
    clipboard::get_file_list()?.into_iter().for_each(|name| {
        println!("{}", name.display());
    });
    Ok(())
}
