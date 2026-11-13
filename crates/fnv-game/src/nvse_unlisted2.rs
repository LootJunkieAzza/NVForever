//! The registered extender commands no wiki page lists, the rest after the mod lists'
//! (#2326), batch 1: SUP's network services and test commands, the debug and address
//! commands, kept as stated neutral answers (extender-compat.md §5.8); ShowOff's `_OLD`
//! commands, which answer as the current ones; IsJumping_BROKEN; StringContainsOnlyASCII,
//! GetFuncCodeByName, GetFuncNameByCode and IsNewGame. ShowOff's string maps are the VM's
//! (`fnv_script::vm::jip`). Argument shapes and answers from probes in the original game
//! (set rest1 of tools/capture/jip_probes.py and the compile-shape run), in our words in
//! docs/spec-notes/ext-unlisted-rest.md, which every comment here cites.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

/// Commands that reach a network service, write the plugin's own log, test the plugin or
/// read the original's memory: NVForever answers 0 and does nothing (§2) [decision:
/// extender-compat.md §5.8, NoOp for the services and tests, refused for RefAddrxData].
pub const NEUTRAL: &[&str] = &[
    // SUP's bulletin board (a web service) [hypothesis: by the names; never probed].
    "bbpostmessage", "bbreturnallmessagestodefaultcell", "bboncellchange", "bbgetinfoforreference", "bbisreferencenote",
    "bbnexusapikeyexist", "bbapikeyexist", "bbgetbbkey", "bbisbbloaded", "bbaremessagesfetched", "bbgetusermessages",
    "bbdeleteusermessage", "bblikemessage", "bbistexteditmenuactive", "bbgettextmenustring", "bbgetuserinfo",
    "bbsettextmenutitle", "bbreportmessage", "bbhidemessage", "bbgetcurrentnotesincell",
    // SUP's update checker and pastebin reader (web services).
    "updatecheckercheckforupdates", "updatecheckerreturnarray", "getsupdatecheckercurrentstage", "getsupdatecheckerdonestage",
    "getpastebinreaderdonestage", "getpastebinreadercurrentstage",
    // Tests, the plugins' own logs, a memory address, a reset of the plugin.
    "suptest", "erdtestfunc", "lunettecmd", "refaddrxdata", "nullargs", "knvsereset", "dumptileinfochilds", "supwritetolog",
];

/// What these commands keep for the session.
#[derive(Debug, Default)]
pub struct Unlisted2State {
    /// The game was started from the New Game menu and no save has been loaded since.
    pub new_game: bool,
}

/// The current command an `_OLD` one answers as [observed: rest1.old.* and new.* read
/// the same where both compiled: IsMatrix, Dump, ListAddArray] (§4).
fn current_of(name: &str) -> Option<&str> {
    name.strip_suffix("_old")
}

/// The name a command opcode belongs to: the catalogue's own name for it, else the
/// longest of the names the opcode table gives it (an alias is the shorter) (§5).
fn name_of_code(g: &Game, code: u32) -> Option<String> {
    let names = crate::nvse_debug::opcode_names(code);
    let canonical = names.iter().find_map(|n| g.data.table.by_name(n).and_then(|op| g.data.table.get(op)).map(|f| f.name.clone()));
    canonical.or_else(|| names.iter().max_by_key(|n| n.len()).cloned())
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    if NEUTRAL.contains(&name) {
        return Some(Ok(XVal::Num(0.0)));
    }
    if let Some(current) = current_of(name) {
        return crate::nvse::call(g, ctx, current, subject, args);
    }
    let v = match name {
        // 1 for the player standing still: the plugin's own name says it is broken
        // [observed: rest1.old.isjumping; hypothesis: always 1] (§4).
        "isjumping_broken" => XVal::Num(1.0),
        // 1 when every character is below 128 [observed: rest1.ascii.plain 1,
        // ascii.high 0] (§5).
        "stringcontainsonlyascii" => match args.first() {
            Some(XVal::Str(s)) => XVal::Num(f64::from(u8::from(s.is_ascii()))),
            _ => XVal::Num(0.0),
        },
        // A command's opcode by name or alias, any case; -1 for none [observed:
        // rest1.funccode.*: GetAV and GetActorValue 4110, AttachLight 10361] (§5).
        "getfunccodebyname" => {
            let n = match args.first() {
                Some(XVal::Str(s)) => s.clone(),
                _ => String::new(),
            };
            XVal::Num(crate::nvse_debug::opcode_of(&n).map_or(-1.0, f64::from))
        }
        // The command's name; no such opcode fails the call [observed: rest1.funcname.*:
        // 4110 GetActorValue, 10361 AttachLight, 1 and 65000 "script call failed"] (§5).
        "getfuncnamebycode" => {
            let code = match args.first() {
                Some(XVal::Num(x)) if *x >= 0.0 => *x as u32,
                _ => return Some(Err("GetFuncNameByCode: no such command".into())),
            };
            match name_of_code(g, code) {
                Some(n) => XVal::Str(n),
                None => return Some(Err(format!("GetFuncNameByCode: no command {code}"))),
            }
        }
        // 1 in a game started from the New Game menu until a save is loaded [observed:
        // rest1.isnewgame 0 in a loaded save; hypothesis: the rest] (§5).
        "isnewgame" => XVal::Num(f64::from(u8::from(g.other.unlisted2.new_game))),
        _ => return None,
    };
    Some(Ok(v))
}
