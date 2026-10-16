//! Where the original game is, and whether it has the files NVForever needs
//! (docs/design/install-layout.md, "First start").
//!
//! The game folder comes from, in order: `--data` / `NVFOREVER_DATA` (developers and the
//! smoke tests; never stored, and never a dialog, so a scripted run fails instead of
//! waiting), the home's `NVForever.ini`, else the player chooses the folder in a dialog:
//! the game's own folder, the one holding `Data` (e.g.
//! `F:\SteamLibrary\steamapps\common\Fallout New Vegas`; its `Data` folder is accepted
//! too). A chosen folder is stored in the INI for later starts. Whatever the source, the
//! folder must hold every file of `fnv_install::REQUIRED_DATA_FILES` and be apart from
//! NVForever's home and executable (`fnv_install::check_separation`); otherwise the game
//! does not start and says what is wrong.
//!
//! The home (`Documents\My Games\NVForever` on Windows) is made here, and on its first
//! start the settings and saves of an earlier build's home (`%LOCALAPPDATA%\NVForever`)
//! are copied into it (`fnv_install::EngineHome::adopt_legacy`).

use fnv_install::{check_separation, Config, EngineHome, GameInstall, InstallError};
use std::path::Path;

/// The game at `dir`, or why it cannot be used, in words for the player.
fn validate(dir: &Path, home: &EngineHome) -> Result<GameInstall, String> {
    let game = GameInstall::open(dir).map_err(|e| match e {
        InstallError::NotAGameInstall(p) => format!(
            "{} is not a Fallout: New Vegas folder: there is no Data\\FalloutNV.esm in it.\n\
             Choose the folder that holds FalloutNV.exe and the Data folder.",
            p.display()
        ),
        InstallError::Io(p, e) => format!("{}: {e}", p.display()),
        e => e.to_string(),
    })?;
    game.check_files().map_err(|e| format!("{e}\n\nVerify the game's files (Steam: Properties, Installed Files) or reinstall the game."))?;
    let exe = std::env::current_exe().ok();
    let overlap = check_separation(home, &game, exe.as_deref());
    if !overlap.is_empty() {
        return Err(InstallError::Overlap(overlap).to_string());
    }
    Ok(game)
}

/// The game install to run with (module notes). `cli` is `--data` / `NVFOREVER_DATA`.
pub fn game_install(cli: Option<&Path>) -> Result<GameInstall, String> {
    let home = EngineHome::user().ok_or("no folder for NVForever's settings and saves could be found (set NVFOREVER_HOME)")?;
    home.create().map_err(|e| e.to_string())?;
    if let Some(old) = EngineHome::legacy() {
        match home.adopt_legacy(&old) {
            Ok(saves) if !saves.is_empty() => {
                eprintln!("setup: copied {} save(s) from {} to {}", saves.len(), old.saves().display(), home.saves().display())
            }
            Ok(_) => {}
            Err(e) => eprintln!("setup: the earlier settings and saves were not brought over: {e}"),
        }
    }
    if let Some(dir) = cli {
        return validate(dir, &home);
    }
    let cfg = Config::load(&home).map_err(|e| e.to_string())?;
    let mut problem = None;
    if let Some(dir) = &cfg.game_dir {
        match validate(dir, &home) {
            Ok(g) => return Ok(g),
            Err(e) => {
                problem = Some(format!(
                    "The Fallout: New Vegas folder set in {}\n({}) cannot be used:\n\n{e}",
                    home.config_file().display(),
                    dir.display()
                ))
            }
        }
    }
    let game = ask(&home, problem)?;
    Config { game_dir: Some(game.root.clone()) }.save(&home, Some(&game)).map_err(|e| e.to_string())?;
    eprintln!("setup: game folder {} stored in {}", game.root.display(), home.config_file().display());
    Ok(game)
}

#[cfg(windows)]
const INTRO: &str = "Welcome to NVForever.\n\n\
    NVForever runs on your own copy of Fallout: New Vegas and only reads its files; it never \
    changes the game's folder.\n\n\
    Choose the game's folder next: the one that holds FalloutNV.exe and the Data folder, \
    for example\nF:\\SteamLibrary\\steamapps\\common\\Fallout New Vegas\n\n\
    It is remembered for later starts.";

/// Usual places of the game, to open the folder dialog at [our choice: Steam's default
/// library and GOG's default folder].
#[cfg(windows)]
fn likely_start() -> Option<std::path::PathBuf> {
    [
        r"C:\Program Files (x86)\Steam\steamapps\common\Fallout New Vegas",
        r"C:\GOG Games\Fallout New Vegas",
        r"C:\Program Files (x86)\Steam\steamapps\common",
    ]
    .iter()
    .map(std::path::PathBuf::from)
    .find(|p| p.is_dir())
}

/// Ask for the game's folder in the system's dialogs until it can be used or the player
/// gives up. `problem` says why the stored folder was refused.
#[cfg(windows)]
fn ask(home: &EngineHome, problem: Option<String>) -> Result<GameInstall, String> {
    use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let confirm = |level: MessageLevel, text: &str| {
        let r = MessageDialog::new().set_title("NVForever").set_level(level).set_description(text).set_buttons(MessageButtons::OkCancel).show();
        matches!(r, MessageDialogResult::Ok)
    };
    let cancelled = || format!("no Fallout: New Vegas folder was chosen; NVForever needs one (it is stored in {})", home.config_file().display());
    let (level, text) = match &problem {
        Some(p) => (MessageLevel::Warning, format!("{p}\n\nChoose the game's folder again.")),
        None => (MessageLevel::Info, INTRO.to_string()),
    };
    if !confirm(level, &text) {
        return Err(problem.unwrap_or_else(cancelled));
    }
    loop {
        let mut dialog = FileDialog::new().set_title("Choose your Fallout: New Vegas folder");
        if let Some(start) = likely_start() {
            dialog = dialog.set_directory(start);
        }
        let Some(dir) = dialog.pick_folder() else { return Err(cancelled()) };
        match validate(&dir, home) {
            Ok(g) => return Ok(g),
            Err(e) => {
                if !confirm(MessageLevel::Error, &format!("{e}\n\nChoose another folder?")) {
                    return Err(e);
                }
            }
        }
    }
}

/// No dialogs here yet: say where the folder goes.
#[cfg(not(windows))]
fn ask(home: &EngineHome, problem: Option<String>) -> Result<GameInstall, String> {
    let how = format!(
        "pass --data <Fallout New Vegas folder>, or set Path= under [Game] in {}",
        home.config_file().display()
    );
    Err(match problem {
        Some(p) => format!("{p}\n{how}"),
        None => format!("no Fallout: New Vegas folder is set: {how}"),
    })
}
