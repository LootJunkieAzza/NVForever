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

/// Whether `name` matches a wildcard pattern (`*` any run, `?` one character), without
/// regard to case.
fn wild(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    go(&p, &n)
}

/// GetDirectoryFiles's list for a pattern under the game folder (§7).
fn directory_files(pattern: &str) -> Vec<String> {
    let (dir, last) = pattern.rsplit_once('\\').unwrap_or(("", pattern));
    if last.is_empty() {
        return Vec::new();
    }
    let data_rel = |d: &str| -> Option<String> {
        let low = d.to_ascii_lowercase();
        if low == "data" {
            Some(String::new())
        } else {
            low.strip_prefix("data\\").map(|_| d[5..].to_string())
        }
    };
    if !last.contains(['*', '?']) {
        // The folder or file itself, when there is one.
        let whole = data_rel(pattern);
        let there = whole.is_some_and(|w| crate::mod_files::exists(&w) || !crate::mod_files::entries_in(&w).is_empty());
        return if there { vec![pattern.to_string()] } else { Vec::new() };
    }
    let Some(rel) = data_rel(dir) else { return Vec::new() };
    let mut names = crate::mod_files::entries_in(&rel);
    names.extend([".".to_string(), "..".to_string()]);
    let mut out: Vec<String> = names.into_iter().filter(|n| wild(last, n)).collect();
    out.sort_by_key(|n| std::cmp::Reverse(n.to_ascii_lowercase()));
    out.into_iter().map(|n| format!("{dir}\\{n}")).collect()
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
        // ---- kNVSE (§7) ----
        // The actor's current animation file for sequence type 0, else "" [observed:
        // knv2.pathbytype.0 Doc's mtidle.kf, the player's
        // first-person idle; types 1-3 ""; hypothesis: type 0 is the body's clip].
        "getanimpathbysequencetype" => {
            let Some(a) = subject else { return Some(Err("GetAnimPathBySequenceType: no actor".into())) };
            let t = args.first().and_then(|v| if let XVal::Num(n) = v { Some(*n) } else { None }).unwrap_or(0.0);
            let clip = if t == 0.0 { g.playing_sequences(a).into_iter().next() } else { None };
            XVal::Str(clip.unwrap_or_default())
        }
        // The answers kNVSE gave for vanilla animations, even the one playing [observed:
        // knv2, knv3: weight 0, hand type -1, additive 0, queued 0, priority -1, text key
        // time -1; hypothesis: they read kNVSE's own overrides, which NVForever has none of].
        "getanimweight" | "isanimadditive" | "isanimqueued" => XVal::Num(0.0),
        "getanimhandtype" | "getaniminterppriority" | "getanimtextkeytime" => XVal::Num(-1.0),
        // Setters that answered 0 and changed nothing a getter showed [observed: knv2.setprio
        // then prio.after -1, copyprio, removetextkey, queue then queued.after 0].
        "setaniminterppriority" | "copyanimpriorities" | "removeanimtextkey" | "queuenextanim" | "setanimadditiveinterpsweightmult"
        | "registercustomanimgroup" | "ejectweapon" => XVal::Num(0.0),
        // 1 for an animation file of the game [observed: knv2.cycletype, destframe;
        // hypothesis: 0 for none].
        "setanimcycletype" | "setanimdestframe" => {
            let p = match args.first() {
                Some(XVal::Str(s)) => s.clone(),
                _ => String::new(),
            };
            XVal::Num(f64::from(u8::from(crate::nvse_anim::file_exists(&p))))
        }
        // [observed: knv2r2 erase.player 1]
        "erasecurrentanimaction" => XVal::Num(1.0),
        // "" outside an animation's own script [observed: knv1.thisanim.*, thisanimdir].
        "thisanim" | "thisanimdir" => XVal::Str(String::new()),
        // No form [decision: the original crashed with a real path and with none,
        // knv2r0, knv2r1].
        "createidleanimform" => XVal::Ref(0),
        // `group flag`: plays as PlayGroup and answers 1 [observed: knv3.playgroupalt.*].
        "playgroupalt" => {
            let Some(on) = subject else { return Some(Ok(XVal::Num(0.0))) };
            let code = match args.first() {
                Some(XVal::Num(n)) => Some(*n as u16),
                Some(XVal::Str(s)) => g.data.table.enums.get("anim_group").and_then(|m| m.iter().find(|(_, n)| n.eq_ignore_ascii_case(s)).map(|(c, _)| *c)),
                _ => None,
            };
            let Some(group) = code else { return Some(Ok(XVal::Num(0.0))) };
            let flag = args.get(1).and_then(|v| if let XVal::Num(n) = v { Some(*n as i32) } else { None }).unwrap_or(0);
            let name = g.anim_group_name(group);
            g.ask_anim(crate::anims::AnimRequest { on, group, name, start: crate::anims::Start::from_flag(flag) });
            XVal::Num(1.0)
        }
        // `pattern [n]`: the paths a wildcard pattern matches, folders and the `.` and
        // `..` entries too, in reverse order of name; the second argument changed nothing;
        // a pattern with no wildcard gives the folder itself [observed: knv3.dir.*,
        // knv2.dir.*]. NVForever lists Data through its read roots [decision].
        "getdirectoryfiles" => {
            let pat = match args.first() {
                Some(XVal::Str(s)) => s.replace('/', "\\"),
                _ => String::new(),
            };
            let found = directory_files(&pat);
            let entries = found.into_iter().enumerate().map(|(i, p)| (fnv_script::vm::XKey::Num(i as f64), XVal::Str(p))).collect();
            XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, entries))
        }
        // 1 in a game started from the New Game menu until a save is loaded [observed:
        // rest1.isnewgame 0 in a loaded save; hypothesis: the rest] (§5).
        "isnewgame" => XVal::Num(f64::from(u8::from(g.other.unlisted2.new_game))),
        _ => return None,
    };
    Some(Ok(v))
}
