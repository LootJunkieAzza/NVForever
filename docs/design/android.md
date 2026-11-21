# Android port (#2436)

Status: experimental implementation; device acceptance is pending. This document describes
NVForever's own platform choices, not behavior inferred from Bethesda code.

## Runtime boundary

[documented: Bevy 0.20.1 mobile example](https://github.com/bevyengine/bevy/blob/v0.20.1/examples/mobile/android_basic/Cargo.toml)
Android loads a `cdylib` through NativeActivity and Bevy's `#[bevy_main]` entry point.
The tiny `nvf-android` package supplies that entry; `nvforever` is a shared Rust library
with its ordinary desktop binary. Desktop builds do not produce an Android shared library.

[design] The initial package targets ARM64, Android 10/API 29 or later, and Vulkan 1.1.
This is a build target, not a supported-device or frame-rate guarantee. Android UI uses
the device's display density; the desktop's fixed screenshot scale is retained on desktop. It starts with
MSAA off and the existing vanilla graphics preset. APKs ship code, the project's runtime
resources and licensed artwork, but no original game content. Android has no player-report
collection or submission path.

## Files and import

[documented: Android Storage Access Framework](https://developer.android.com/training/data-storage/shared/documents-files)
The launcher asks the user to choose a game folder or Data folder. It requests read
access and copies the Data tree, plus Fallout_default.ini when the game root was chosen,
into a unique directory in the app's external files directory. File streams are bounded
buffers; source documents are never opened for writing. Providers with recursive IDs,
unsafe names or unreadable files fail the import. The existing selected install remains
selected until the new copy completes and the configuration is atomically replaced.

[design] Only UUID directories directly under this app's external files directory,
marked by the importer, can be removed as failed or superseded imports. The currently
selected import is protected. Cleanup does not follow symbolic links. The user needs
space for both the old and new copies until replacement completes. App uninstall removes
app-owned files, including saves; export important saves before uninstalling.

[design] Settings, logs, saves and generated caches use the internal `files/home` directory
via `NVFOREVER_HOME`. Schema files are extracted from this APK to `files/runtime/schemas`
before parsing engine arguments, and translations to `files/home/lang`. Game input and
engine output stay separate, retaining the existing write guards.

## Loading and process lifetime

[documented: `engine/nvforever/src/handoff.rs`] The desktop engine starts a new process to
load a save or return to the main menu, waiting for pending save writes first.

[design] Android has no game executable to spawn. The engine asks a non-exported restart
activity in the launcher process to terminate only this UID's identified `:game` process.
The activity waits until that process has disappeared before starting a fresh NativeActivity
with the successor arguments. Its NEW_TASK | CLEAR_TASK flags replace the old game task
instead of reusing a retained singleTask activity/Intent ([Android Intent flags](https://developer.android.com/reference/android/content/Intent#FLAG_ACTIVITY_CLEAR_TASK)). Desktop handoff markers and hidden-window placement are
omitted. Restart activities cannot be invoked by other apps. The game activity is a
single task, so selecting Start again cannot create two native engines in one process.
This path needs device testing for load failures, task switching and OS process death.

## Input and remaining qualification

[design] Touch buttons aggregate their held inputs with independently recorded physical
key/mouse events after Bevy's input systems. Releasing either source keeps an action held
until both sources release, preserving the combined press/release edges, including taps
whose press and release both arrive in one frame. Within a frame, hardware events are
processed first and then touch events, each in its own receive order. They use current keyboard bindings for game actions and ordinary navigation keys
in menus. Drag the lower left to move and the right free area to look. Pause, Pip-Boy,
VATS, reload, jump, activate, aim, attack and sneak have buttons. Menu arrows, confirmation,
back, tab and page navigation have buttons. External keyboard/mouse use the existing path.
Controllers, soft-keyboard text entry, accessibility and a fully configurable touch layout
remain unqualified. Background audio/pause and graphics surface resume need device tests.

Do not mark #2436 complete on the strength of compilation alone. Acceptance requires an
installed APK on representative hardware: import a lawful install, start/new game, move,
look, activate, dialogue and menu navigation, save/load/return to menu, audio, background
and resume, restart/process death, a malformed import, and sustained memory/thermal behavior.
Physical-device qualification is pending.

## Touch layout revision after initial device testing

[NVForever device test: user, 2026-11-13; not an original-game observation] The first device build reached the opening interior.
The user found the invisible movement area unclear and the text-only action grid crowded.
Leaving the interior exposed a Rapier arrival-query panic; that is under separate physics
regression work, not evidence of a GPU or memory failure.

[documented] Activision's [mobile control guide](https://blog.activision.com/au/en/call-of-duty/2019-10/Getting-a-Grip-on-the-Call-of-Duty-Mobile-Controls) describes a virtual stick and an editor for position, size and opacity.
PUBG's [control guide](https://pubgmobile.helpshift.com/hc/en/3-pubg-mobile/faq/37-what-are-the-controls/?l=en) describes left-side movement, free-area look and icon actions. These inform common interaction conventions; no artwork or implementation is copied.

[design] The revised overlay draws its own geometric action icons at runtime, shows a
movement base and thumb position, and lights held buttons. Help explains the two thumb
areas and toggles action names. The cog opens a layout editor, releases held gameplay
inputs and requests the ordinary pause menu. Drag selects/repositions a control; explicit
buttons resize it, adjust opacity, show/hide secondary actions, mirror hands, choose a
fixed/floating stick, edit menu controls, reset, save or cancel. The save is versioned
JSON in the engine's own home; write guards and atomic rename protect its previous copy.
Cancelling restores the original layout. Visual bounds and hit testing share geometry;
action targets retain a minimum 48 logical-pixel diameter on a phone. Menu navigation
remains separate from gameplay actions. Editing stays paused until the player resumes.

## Clean-room classification of the port's constants and behaviour

[design] Every number and rule the Android port adds is an NVForever engine/port design
choice, not a claim about how Fallout: New Vegas behaves. None is backed by an
observation of the original game, and none needs to be, because the original game has
no touch, phone, launcher or Android behaviour to match. In the `docs/spec-notes/android-*.md`
notes, `[hypothesis]` marks a port design proposal or pending device acceptance, and
`[documented]` cites this repository's source, a pinned dependency or public platform
documentation. Neither tag there claims anything about the original game. The groups are:

- Touch geometry and gesture timing: 48-logical-unit minimum targets and rows (52 and 51
  for hand-built workbench and repair rows), 12-unit drag/tap slop (`menu/normal_touch.rs`,
  `pipboy/data/touch_map.rs`, `maintenance_touch.rs`, `menu/barter_touch.rs`,
  `touch/world_activate.rs`), 0.35 s double tap (`touch/world_activate.rs`), 0.6 s hold
  to activate (`activate/touch.rs`), 0.45 s VATS hold (`vats_mode/touch.rs`), the
  0.1/0.18/0.25/0.35 stick dead zones (`touch/aiming.rs`), the Pip-Boy swipe directions
  and distances, and the fixed, floating and legacy stick schemes.
- Sizes and defaults: phone dialogue text base 16 logical units against 20 on desktop
  (`dialogue_touch.rs`), 85% choice and 100% subtitle defaults on Android, 50–150% menu
  text sizes, `hud-scale-v2` 25–250% in 5% steps (46 choices, `interface.rs`), 75–150%
  lockpick sizes (`lockpick/touch.rs`), 0.85–1.35 Pip-Boy presentation scale
  (`pipboy/touch_presentation.rs`), the 78-unit appearance preview and the eight-pixel
  split gap, and the touch maintenance presentation On by default on Android only.
- Bounds and limits: at most 128 queued menu-sheet edges and 32 owned contact IDs
  (`touch/menu_sheets.rs`), 32 queued maintenance requests (`maintenance_touch.rs`),
  16 retired VATS marks and 32 points (`vats_mode/touch.rs`), 16 touch-map edits, 8 layout
  profile slots and 128 KiB profile files (`touch/profiles.rs`), 64 tray axis cells
  (`touch/hotbar_layout.rs`), 512-pixel and 8 MiB SVG raster limits (`touch/icons.rs`),
  a 4096-byte companion-wheel settings read, the controller event queue of 512
  (`android_controller.rs`), the render admission limits (128 skins, 32 rigid bodies,
  8 bursts, 8 idle frames, `android_render_admission.rs`), 32 render diagnostics reports,
  16 Pip-Boy UV attempts (`dialogue_pipboy.rs`) and the eight NIF rejection samples
  ([NIF numeric admission](../spec-notes/nif-numeric-admission.md)).
- Timeouts and lifetimes: thermal sampling (10 s, 25 s stale), gyro sample gap
  and age (50 ms, 100 ms), and the loading-cover ticket, lease and generation
  checks ([loading cover](android-loading-cover.md)).
- Mode and layout policy: the Mobile settings tab and its placements
  ([settings page](settings-page.md)), Battery/Balanced/Quality performance presets
  with 30 or 60 FPS caps (`mobile_performance.rs`), borderless and one MSAA sample as
  Android start defaults, the five dialogue styles, speaker side and safe-area framing,
  touch input ownership and cancellation, touch controls reaching scripts through
  `IsControlPressed` as their bound control without inventing physical keys, and the
  launcher's pages, read-only folder import, newest-save Continue, Markdown news/release
  formatting (`MarkdownParser.java` / `MarkdownRenderer.java`, 700/400-character preview
  caps), and user-confirmed HTTPS GitHub Releases ARM64 APK updater (`UpdatePolicy.java`,
  `ApkUpdateProvider.java`, 512 MiB cap, 5 HTTPS-only redirects, optional `SHA256SUMS.txt`
  verification).
- Classic dialogue casing: the 1774 × 887 frame, its four apertures, band layout and
  12-unit caps glyph minimum ([Classic runtime](dialogue-classic-runtime.md)) are
  project design, not original-game observations. The contributor supplied the
  AI-generated image; it is not taken or derived from game files. Its TRADE
  control opens the existing barter menu.
- Engine library choices: the vendored Android `winit` and Rapier 0.36.1 query refresh
  ([query refresh](../spec-notes/query-refresh-island-lifecycle.md)), the DDS decode
  fallback when BC sampling is missing ([DDS](../spec-notes/dds.md)), NIF numeric
  admission, owned SVG icon families and the launcher icon adaptation.

[hypothesis] One statement touches the original game: a character who is not wearing a
Pip-Boy, such as a new character before Doc Mitchell gives one, cannot use Wrist
Terminal and talks on Normal. Wrist Terminal reads the worn Pip-Boy from the live game
state; why it did not present on the reporting phone was not observed.

[design] No text the game shows is transcribed. Touch labels, help, launcher text and
companion-wheel refusal messages are NVForever's own wording. Text from the game (dialogue,
item names, menu settings) is read from the player's installation at run time, and the
Terminal and Pip-Boy models and Font 5 come from that installation; the APK contains none
of them.
