# NVForever launcher

The window that starts NVForever: news, Play / Continue, settings, and mods.

This repository is only that launcher and the crates it builds. It is not the game.
It starts an `nvforever` program you already have, beside the launcher or named in
`NVForever.ini`.

is a mod manager thing

## Build

Needs [Rust](https://rustup.rs). On Linux also `pkg-config` and `libssl-dev`.

```bash
cargo build --release --locked -p nvf-launcher
```

'don't do it wait for it to be finished and pulled into main branch

The program is `target/release/nvforever-launcher` (`nvforever-launcher.exe` on Windows).

Put it next to your existing `nvforever` program, outside the Fallout: New Vegas folder.
Settings and saves stay in `Documents/My Games/NVForever`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and [LEGAL.md](LEGAL.md).
