//! xNVSE's and FOSE's console functions, `Con_*` (docs/spec-notes/ext-console.md): each
//! runs a console command from a script [documented: the Con_ pages of the wiki export,
//! "Runs the console command … via a script"]. They go through the console's own
//! commands (`functions::console`): the printing ones print only while SetConsoleEcho is
//! on [documented: Con_Inv, Con_ShowVars, Con_SQV "Requires Console Echo"; hypothesis:
//! every Con_ command's console lines follow the echo the same way].
//!
//! Not built (ext-console.md §3): Con_SCOF (a file in the game folder), Con_TDT (no debug
//! text display), con_SetVel (no way yet for a script to set a body's velocity).

use crate::functions::console::{self, ConsoleCtx};
use crate::functions::toggles::Toggle;
use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// An argument as a console word: a number as the INI writes it, a form as its eight hex
/// digits, a string as it is (quoted when it holds a space, as the console takes it).
fn word(v: &XVal) -> Result<String, String> {
    Ok(match v {
        XVal::Num(x) => crate::nvse_settings::number_text(*x),
        XVal::Ref(r) => format!("{r:08X}"),
        XVal::Str(s) if s.contains(char::is_whitespace) => format!("\"{s}\""),
        XVal::Str(s) => s.clone(),
        other => return Err(format!("not a console word: {other:?}")),
    })
}

fn words(args: &[XVal]) -> Result<Vec<String>, String> {
    args.iter().map(word).collect()
}

/// Lines for the console, while the echo is on.
fn echo(g: &mut Game, lines: Vec<String>) {
    if g.other.echo {
        g.console_out.extend(lines);
    }
}

/// A console command that prints (`console::registry`), on `on`.
fn printing(g: &mut Game, cmd: &str, on: Option<FormId>, args: &[String]) -> Result<XVal, String> {
    let run = console::lookup(cmd).ok_or_else(|| format!("{cmd} is not a console command"))?;
    let ctx = ConsoleCtx { on, active_quest: g.ui.active_quest };
    let lines = run(g, &ctx, args)?;
    echo(g, lines);
    Ok(XVal::Num(0.0))
}

/// A console command that is a script function of the table (`console::call_words`).
fn called(g: &mut Game, cmd: &str, on: Option<FormId>, args: &[String], unit: &str) -> Result<XVal, String> {
    console::call_words(g, on, cmd, args, unit)?;
    Ok(XVal::Num(0.0))
}

/// The functions of this file (lower-case catalogue names); `None` for any other.
/// `subject` is the reference called on, else the calling reference.
pub(crate) fn call(g: &mut Game, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    if !name.starts_with("con_") {
        return None;
    }
    let w = match words(args) {
        Ok(w) => w,
        Err(e) => return Some(Err(e)),
    };
    let r = match name {
        // The printing commands [documented: Con_Inv → `inv`, Con_ShowVars → ShowVars,
        // Con_SQV → ShowQuestVars, on the calling reference / the quest given].
        "con_inv" => printing(g, "ShowInventory", subject, &[]),
        "con_showvars" => printing(g, "ShowVars", subject, &[]),
        "con_sqv" => printing(g, "ShowQuestVars", subject, &w),
        // [documented: con_SetGameSetting "Sets a game setting"].
        "con_setgamesetting" => printing(g, "SetGameSetting", subject, &w),
        // [documented: con_QuitGame "Exit game without going through menus"].
        "con_quitgame" => printing(g, "QuitGame", subject, &[]),
        // [documented: con_CloseAllMenus "Closes all open menus"]: the game's menus close
        // as the console's CloseAllMenus closes them (`Game::close_all_menus`), and the
        // engine closes the console.
        "con_closeallmenus" => {
            g.close_all_menus();
            printing(g, "CloseAllMenus", subject, &[])
        }
        // [documented: con_Save `name [dump]`; con_LoadGame `name [dump]`]. The dump flag
        // (a text file about the save) is not built: no file is written [hypothesis: the
        // save itself is the command's point].
        "con_save" => printing(g, "SaveGame", subject, &w[..w.len().min(1)]),
        "con_loadgame" => printing(g, "LoadGame", subject, &w[..w.len().min(1)]),
        // The toggles and camera commands [documented: Con_TCL → ToggleCollision, Con_TFC →
        // ToggleFlyCam, Con_ToggleMenus → ToggleMenus, Con_SetUFOCamSpeedMult →
        // SetUFOCamSpeedMult; con_SetCameraFOV "Change the camera's field of view"].
        "con_tcl" => called(g, "ToggleCollision", subject, &[], "Con_TCL"),
        "con_tfc" => called(g, "ToggleFlyCam", subject, &[], "Con_TFC"),
        "con_togglemenus" => called(g, "ToggleMenus", subject, &[], "Con_ToggleMenus"),
        "con_setufocamspeedmult" => called(g, "SetUFOCamSpeedMult", subject, &w, "Con_SetUFOCamSpeedMult"),
        "con_setcamerafov" => called(g, "SetCameraFOV", subject, &w, "con_SetCameraFOV"),
        // [documented: Con_TFIK runs ToggleFootIK]: the engine switches its foot IK as the
        // settings page does.
        "con_tfik" => {
            g.toggles.asked.push(Toggle::FootIk);
            Ok(XVal::Num(0.0))
        }
        // The INI [documented: con_GetINISetting prints the value, "SettingName:Category";
        // con_SetINISetting sets it; con_RefreshINI "Refresh INI settings from files";
        // con_SaveINI "Writes all the .ini files"], on the settings the extender's INI
        // functions read (`crate::nvse_settings`).
        "con_getinisetting" => {
            let line = match args.first() {
                Some(XVal::Str(s)) => match g.ini_value(s) {
                    Some(v) => format!("{s} = {v}"),
                    None => format!("No INI setting {s}"),
                },
                _ => "con_GetINISetting \"Name:Category\"".to_string(),
            };
            echo(g, vec![line]);
            Ok(XVal::Num(0.0))
        }
        "con_setinisetting" => {
            if let (Some(XVal::Str(s)), Some(v)) = (args.first(), w.get(1)) {
                g.set_ini_value(s, v.trim_matches('"'));
            }
            Ok(XVal::Num(0.0))
        }
        // The files are never changed by NVForever, so reading them again gives their
        // values back: the session's changes go [hypothesis].
        "con_refreshini" => {
            g.ini.reload();
            Ok(XVal::Num(0.0))
        }
        // NVForever never writes the game's INI files (install-layout.md): nothing to do.
        "con_saveini" => Ok(XVal::Num(0.0)),
        _ => return None,
    };
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_become_console_words() {
        assert_eq!(word(&XVal::Num(2.5)).unwrap(), "2.5");
        assert_eq!(word(&XVal::Num(75.0)).unwrap(), "75");
        assert_eq!(word(&XVal::Ref(0x0010_4C0C)).unwrap(), "00104C0C");
        assert_eq!(word(&XVal::Str("sSomeName".into())).unwrap(), "sSomeName");
        assert_eq!(word(&XVal::Str("two words".into())).unwrap(), "\"two words\"");
    }
}
