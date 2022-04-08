# midi-sniffer

`midi-sniffer` is portable GUI to inspect MIDI messages on up to 2 ports.
It's usable, but still in early development.

![midi-sniffer UI](assets/screenshot_20220408.png "midi-sniffer UI")

## Build

You need a stable Rust toolchain for the target host. Get it for [this page](https://www.rust-lang.org/fr/tools/install).
On Unix-like systems, you should be able to install `rustup` from your packet
manager.

Clone the git tree and run the following command in an environment where
`cargo` is available:

```
cargo b --release
```

This will compile the executable **without** `jack` support. If you need `jack`
support, use the following command:

```
cargo b --release --features=jack
```

**Note:** `jack` support requires development libraries and headers to be
available in your compilation environment. On Unix-like systems, the package
should look like `jack-audio-connection-kit-devel` or
`pipewire-jack-audio-connection-kit-devel`.

## Run

If compilation succeeds, you should be able to launch the executable with:

```
target/release/midi-sniffer
```

## LICENSE

This crate is licensed under MIT license ([LICENSE-MIT](LICENSE-MIT) or
http://opensource.org/licenses/MIT)
