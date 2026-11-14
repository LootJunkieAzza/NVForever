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
MSAA off and the existing vanilla graphics preset. APKs contain engine code, owned
schemas and translation text only. They contain no game content. Player reports are off.

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
