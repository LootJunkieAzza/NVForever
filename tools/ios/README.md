# Local iPad builds

[design] This builds the existing game as an iPad app, not an App Store release. It
does not bundle game data or change signing accounts. Xcode and a Rust toolchain with
`aarch64-apple-ios` (device) or `aarch64-apple-ios-sim` (Apple Silicon simulator)
installed are required. Ensure `cargo` and `rustc` belong to the toolchain into which
`rustup target add` installed the target; Homebrew's standalone Rust is separate.

[design: `build.sh`, `icons.swift`] Packaging generates the iPad app-icon sizes from
`engine/nvforever/assets/icon.png`, with an opaque black backing, and compiles them
with Xcode's asset-catalog tool. Generated icons and `Assets.car` stay in build
output; no new artwork, game files, signing identities or profiles are committed.

```sh
rustup target add aarch64-apple-ios
bash tools/ios/build.sh
```

[design] The unsigned app is in `engine/target/aarch64-apple-ios/NVForever.app`.
Sign it with your own iOS development certificate, matching provisioning profile and
entitlements, then install with `xcrun devicectl device install app --device <UDID>
<app>`. The profile must cover `org.nvforever.game` and the device. No signing files
belong in the repository.

[design] Copy your own installed game's folder into the app's `Documents/Game`,
keeping `Data/FalloutNV.esm`, the archives and other original files in place. For a
connected device, after installing the app:

```sh
xcrun devicectl device copy to --device <UDID> \
  --domain-type appDataContainer --domain-identifier org.nvforever.game \
  --source "/path/to/Fallout New Vegas" --destination Documents/Game
xcrun devicectl device process launch --device <UDID> org.nvforever.game
```

[design] The app defaults to that data path and writes its own settings, saves and
logs into `Library/Application Support/NVForever`, outside the game input folder.
`NVFOREVER_DATA` and `NVFOREVER_HOME` overrides still work. Passing the data path
through this developer route keeps player reports off.

[design] For a simulator, build with `bash tools/ios/build.sh aarch64-apple-ios-sim`,
install with `xcrun simctl install <UDID> <app>`, and copy the game folder to
`Documents/Game` inside the container returned by `xcrun simctl get_app_container
<UDID> org.nvforever.game data`. Simulator device profiles and a compatible runtime
must be installed in Xcode.

[observed: `engine/nvforever/src/reports/mod.rs`] Optional sysinfo hardware fields
are omitted on iOS: the pinned sysinfo version calls macOS-only libc symbols.
[design] This packaging does not add virtual movement controls or a new mobile input
scheme.

[design: `engine/nvforever/src/ios_input.rs`, `ios_input.m`] Physical keyboard presses
and releases, mouse buttons, raw relative mouse motion, menu pointer positions and
scrolling feed Bevy's existing input messages and the game's existing bindings.
Disconnecting a device or leaving the app releases held controls. UIKit/winit still
supplies layout-aware text entry; the native bridge does not duplicate typed text.
UIKit's pointer lock replaces winit's unsupported cursor grab on iPad.

[design: `engine/nvforever/src/window_mode.rs`, `ios_input.rs`] iPad uses its native
full-screen drawable, not the desktop `--size` or stored desktop display mode, and
hides the iOS status bar. The ordinary Frame rate counter setting can be enabled in
the app's `Library/Application Support/NVForever/NVForever.ini` with
`[Settings]` and `fps-counter=1` (or `--fps` for a developer launch).

## Device validation

[observed: local development run, 2026-11-20] Installed and launched on an iPad Pro
13-inch M4 running iPadOS 26.5, using the owner's base-game files copied into the app
container. Native Metal initialized the Apple M4 GPU and audio output. A Goodsprings
run (`--play --cell Goodsprings --radius 1 --no-lodgen --no-sun-shadows`) completed
loading in 14.59 seconds with no GPU validation errors or panics in the captured run
log. This is startup/rendering validation, not a gameplay or performance benchmark.

[observed: same run] The initial native launch failed because combined bindless
materials allocated more than the base layout's 64 resources; the vendored Bevy fix
is documented in `engine/vendor/README.md`. iOS keeps its native Retina scale instead
of the desktop screenshot scale override. Gilrs reports that iOS is unsupported.
This initial launch predates the physical-input bridge and does not validate physical
keyboard/mouse gameplay. Controller support is not established. Touch movement
controls are not present.

[observed: updated device run, 2026-11-20] The physical-input build installed and
launched on the same iPad. GameController reported both a keyboard and a mouse
connected. The game logged a 2752x2064 drawable (1376x1032 logical points), and
screenshots contained non-black scene content at all four edges. The saved Frame
rate counter setting was applied and screenshot OCR detected its numeric readout.
The status bar is hidden through winit's iOS-specific API. Direct screenshot review
was blocked by the account's upload quota. The requester subsequently confirmed
that physical WASD, mouse-look and status-bar hiding work on the iPad. These checks
are not a performance benchmark.
