# winapi-easy
An ergonomic and safe interface to some Windows API functionality.

[![Latest version](https://img.shields.io/crates/v/winapi-easy)](https://crates.io/crates/winapi-easy)
[![Documentation](https://docs.rs/winapi-easy/badge.svg)](https://docs.rs/winapi-easy)
![License](https://img.shields.io/crates/l/winapi-easy)

## Design

This is an **experimental** library designed as an abstraction over the Windows API with the following properties:

* Properly typed parameters, making wrong usage of the API difficult
* Automatic release of resources on drop
* No unsafe functionality exposed to the user outside of 'escape hatches'
* Consistent error handling without special numerical return values

Expect breaking changes between versions. Any kind of feature completeness is also unrealistic given the huge size
of the Windows API.

## Features

* Keyboard and mouse control (sending events, hotkeys, hooking)
* Creating windows and manipulating window properties
* Window actions like minimize, flash, taskbar progress animation
* Creating notification icons & menus
* Magnification API
* Running message loops & listening for events
* Process and thread functionality (including CPU & IO priority)
* File transfers with progress notifications

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
