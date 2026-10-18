# NVForever

An open-source, clean-room reimplementation of the **Fallout: New Vegas** engine, in the
spirit of OpenMW. It runs on the data files of *your own copy* of the game; it ships no
Bethesda content and contains no Bethesda or Gamebryo code.

NVForever is not affiliated with or endorsed by Bethesda Softworks, ZeniMax Media,
Microsoft or Obsidian Entertainment. See [LEGAL.md](LEGAL.md).

## Getting started

1. Own and install *Fallout: New Vegas* (base game; DLC optional).
2. Download a build from the [releases page](https://github.com/LootJunkieAzza/NVForever/releases)
   and unpack it **outside** the game folder.
3. Run `nvforever.exe` (or `./nvforever.sh` on Linux) and point it at your game folder
   when asked.

NVForever never writes into the game folder. Its settings, saves and logs live in
`Documents\My Games\NVForever`.

## Building from source

Needs [Rust](https://rustup.rs) and Python 3.11+.

```bash
cd engine
cargo run --release --bin nvforever -- --data "<path to Fallout New Vegas>/Data"
```

More: [docs/development.md](docs/development.md) (build, test, options, tools) ·
[docs/status.md](docs/status.md) (what's built) ·
[docs/outstanding.md](docs/outstanding.md) (what's left).

## Bugs

Report bugs and differences from the original with the
[issue forms](https://github.com/LootJunkieAzza/NVForever/issues/new/choose). Crash
reports are written to `Documents\My Games\NVForever\logs`. Never attach game files.

## Support

NVForever is free. If you'd like to support it: [ko-fi.com/lootjunkieazza](https://ko-fi.com/lootjunkieazza).
Donations buy nothing extra; you still need your own copy of the game.

## Licence

GPL-3.0-or-later; see [LICENSE](LICENSE) and [LEGAL.md](LEGAL.md#licence).
