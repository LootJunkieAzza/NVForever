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

Optional: NVIDIA DLSS upscaling on an RTX graphics card (Windows), with files downloaded
from NVIDIA: see [docs/dlss.md](docs/dlss.md).

## Mods

The launcher's Mods page is its own manager. Mod folders live under
`Documents\My Games\NVForever\mods`, not in the game folder.

- Drop a folder or an archive on **Add**. That line cannot be typed. **Add**
  installs it. An installer that needs a choice opens its own window. New plugins
  from that folder appear in the list.
- Drag a folder to move it. The bottom checked folder wins when two share a file.
  **Show overwrites** marks a shared file green when this folder's copy is used,
  and red when a lower folder's copy wins.
- Drag plugins the same way. **Apply** writes the plugin list and the folder
  order the launcher uses. It does not copy mod files.

A profile is a name you type, plus the folder order and the plugin order on the
page. The name is one folder name, not a path.

- **Add** and **Save backup** write `profiles/<name>/` in the launcher home.
  The same name replaces these three files and leaves any other file there:
  - `modlist.txt` — one folder a line. `+` is on, `-` is off. The winning folder
    is first in the file. The page shows that folder last.
  - `plugins.txt` — plugins that are on, a `*` before the name.
  - `loadorder.txt` — every plugin on the page, first loaded first.
- Drop a folder on the profile bar to fill the path. That line cannot be typed.
  **Read** lists profiles there. **Load** puts that folder order and plugin
  order on the page. A save does not write into the dropped folder.
- **Load backup** is the older dated plugin list, not a named profile.
  **Save backup** writes the named profile.

**Apply**, and **Load** of a profile, still symlink turned-on mod files into the
game `Data` folder. A file already there is left alone. Nothing is copied. A
profile save does not do that. An output folder for files the game or a tool
creates is not built yet. Reading another program's instance with `--mo2` stays
read-only.
  
Script extenders (xNVSE, JIP LN, JohnnyGuitar and several smaller ones) are built into the
engine: their script functions run without the plugin `.dll` files, which NVForever never
loads. A mod that needs a plugin's engine patches, or a function not built yet, may not work.

## Building from source

Needs [Rust](https://rustup.rs) and Python 3.11+.

```bash
cd engine
cargo run --release --bin nvforever -- --data "<path to Fallout New Vegas>/Data"
```

The launcher is a separate program, from the repository root:

```bash
cargo build --release --locked -p nvf-launcher
```

The program is `target/release/nvforever-launcher` (`nvforever-launcher.exe` on Windows).

More: [docs/development.md](docs/development.md) (build, test, options, tools) ·
[docs/status.md](docs/status.md) (what's built) ·
[docs/outstanding.md](docs/outstanding.md) (what's left).

## Community

Chat, questions and playtesting: [Discord](https://discord.gg/3HsVrsXSb).

## Bugs

Report bugs and differences from the original with the
[issue forms](https://github.com/LootJunkieAzza/NVForever/issues/new/choose). Crash
reports are written to `Documents\My Games\NVForever\logs`. Never attach game files.

## Contributing

Pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) first: the clean-room
rules there decide what can be merged.

## Support

NVForever is free. If you'd like to support it: [ko-fi.com/lootjunkieazza](https://ko-fi.com/lootjunkieazza).
Donations buy nothing extra; you still need your own copy of the game.

## Licence

GPL-3.0-or-later; see [LICENSE](LICENSE) and [LEGAL.md](LEGAL.md#licence).
