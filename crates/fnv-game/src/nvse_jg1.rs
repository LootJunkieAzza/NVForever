//! JohnnyGuitar NVSE, batch 1 (#1007): record getters and setters of books, factions,
//! creatures, races, weapons' world models, music types, containers' sounds and lighting
//! templates [documented: each page in the wiki export; docs/spec-notes/ext-jg1.md].
//! Setters change the record for the session, read by the getters.

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        XVal::Num(x) if *x >= 1.0 && x.fract() == 0.0 => Some(FormId(*x as u32)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => dflt,
    }
}

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// A container's sound by GetContainerSound's number: 0 open (SNAM), 1 close (QNAM),
/// 2 looping (RNAM) [documented: GetContainerSound; the subrecords: xEdit
/// wbDefinitionsFNV.pas CONT].
fn container_sound_sub(which: f64) -> Option<&'static str> {
    match which as i64 {
        0 => Some("SNAM"),
        1 => Some("QNAM"),
        2 => Some("RNAM"),
        _ => None,
    }
}

/// A lighting template trait 1-16 [documented: GetLightingTemplateTraitNumeric's table]:
/// the LGTM DATA field (sub, path).
fn lighting_trait(id: f64) -> Option<(&'static str, String)> {
    let colour = |c: &str, i: i64| format!("{c}.{}", ["red", "green", "blue"][(i % 3) as usize]);
    let id = id as i64;
    Some(match id {
        1..=3 => ("DATA", colour("ambient_color", id - 1)),
        4..=6 => ("DATA", colour("directional_color", id - 4)),
        7..=9 => ("DATA", colour("fog_color", id - 7)),
        10 => ("DATA", "fog_near".into()),
        11 => ("DATA", "fog_far".into()),
        12 => ("DATA", "directional_rotation_xy".into()),
        13 => ("DATA", "directional_rotation_z".into()),
        14 => ("DATA", "directional_fade".into()),
        15 => ("DATA", "fog_clip_dist".into()),
        16 => ("DATA", "fog_power".into()),
        _ => return None,
    })
}

impl Game<'_> {
    /// Transient `(game_instance, revision)` token for live interior lighting session edits.
    /// Changes whenever a fresh/restored `Game` is created or any session lighting setter
    /// writes `CELL LTMP`, `CELL LNAM` (`SetCellFlag` 8..=16), `LGTM DATA`
    /// (`SetLightingTemplateTraitNumeric`), or `CELL XCLL` (`SetInteriorLightingTraitNumeric`).
    pub fn lighting_token(&self) -> (u64, u64) {
        (self.lighting_instance, self.lighting_revision)
    }

    pub(crate) fn bump_lighting_revision(&mut self) {
        self.lighting_revision = self.lighting_revision.wrapping_add(1);
    }

    /// Interior lighting at an engine cell-application or live-refresh boundary, including
    /// session `CELL LTMP`/`LNAM`/`XCLL` and selected `LGTM DATA` writes (#2272).
    /// This does not persist session edits across `Game::restore`.
    pub fn session_cell_lighting(&self, lo: &fnv_esm::LoadOrder, schema: &fnv_esm::Schema, cell: FormId) -> Option<fnv_world::lighting::CellLighting> {
        // Only explicit session choices replace LTMP/LNAM. The unchanged world
        // reader owns the base record's plugin-local -> global FormID mapping.
        let template = match self.field_overrides.get(&(cell, "LTMP/".into())) {
            Some(XVal::Ref(id)) => Some(*id),
            _ => None,
        };
        let flags = self.field_overrides.get(&(cell, "LNAM/".into())).and_then(|v| v.num().ok()).map(|n| n as u32);
        let has_own_override = (0u8..=15).any(|id| {
            let path = crate::nvse_jg2::interior_trait(f64::from(id)).expect("the documented sixteen interior lighting traits");
            self.field_overrides.contains_key(&(cell, format!("XCLL/{path}")))
        });
        fnv_world::lighting::cell_lighting_with_session(
            lo,
            schema,
            cell.0,
            template,
            flags,
            has_own_override,
            |light| {
                // Apply explicit SetInteriorLightingTraitNumeric (CELL XCLL) session overrides.
                for id in 0u8..=15 {
                    let path = crate::nvse_jg2::interior_trait(f64::from(id)).expect("the documented sixteen interior lighting traits");
                    let Some(value) = self.field_overrides.get(&(cell, format!("XCLL/{path}"))).and_then(|v| v.num().ok()) else { continue };
                    match id {
                        0..=2 => light.ambient[id as usize] = value as u8,
                        3..=5 => light.directional[(id - 3) as usize] = value as u8,
                        6 => light.directional_rotation_xy = value as i32,
                        7 => light.directional_rotation_z = value as i32,
                        8 if (value as f32).is_finite() => light.directional_fade = value as f32,
                        9..=11 => light.fog_color[(id - 9) as usize] = value as u8,
                        12 if (value as f32).is_finite() => light.fog_near = value as f32,
                        13 if (value as f32).is_finite() => light.fog_far = value as f32,
                        14 if (value as f32).is_finite() => light.fog_power = value as f32,
                        15 if (value as f32).is_finite() => light.fog_clip_dist = value as f32,
                        _ => {}
                    }
                }
            },
            |template, light| {
                // Reuse the getters' field mapping for a LoadOrder-backed template.
                // The world reader admits that record before this callback; runtime
                // clone IDs are not resolved here. Casts retain the typed destinations.
                for id in 1u8..=16 {
                    let (sub, path) = lighting_trait(f64::from(id)).expect("the documented sixteen lighting traits");
                    let Some(value) = self.field_overrides.get(&(FormId(template), format!("{sub}/{path}"))).and_then(|v| v.num().ok()) else { continue };
                    match id {
                        1..=3 => light.ambient[(id - 1) as usize] = value as u8,
                        4..=6 => light.directional[(id - 4) as usize] = value as u8,
                        7..=9 => light.fog_color[(id - 7) as usize] = value as u8,
                        10 if (value as f32).is_finite() => light.fog_near = value as f32,
                        11 if (value as f32).is_finite() => light.fog_far = value as f32,
                        12 => light.directional_rotation_xy = value as i32,
                        13 => light.directional_rotation_z = value as i32,
                        14 if (value as f32).is_finite() => light.directional_fade = value as f32,
                        15 if (value as f32).is_finite() => light.fog_clip_dist = value as f32,
                        16 if (value as f32).is_finite() => light.fog_power = value as f32,
                        _ => {}
                    }
                }
            },
        )
    }

    /// A race's model path from its head (NAM0) or body (NAM1) part list, male (MNAM) or
    /// female (FNAM), the part's INDX followed by its MODL [documented: xEdit
    /// wbDefinitionsFNV.pas RACE, "Head Data" and "Body Data"; the parts' numbers:
    /// GetRaceHeadModelPath, GetRaceBodyModelPath].
    fn race_model(&self, race: FormId, body: bool, female: bool, part: i64) -> String {
        let Some(fields) = self.data.record_fields.get(&race) else { return String::new() };
        let (mut section, mut sex, mut index) = (None, None, None::<i64>);
        for (sig, v) in fields {
            match sig.as_slice() {
                b"NAM0" => (section, sex, index) = (Some(false), None, None),
                b"NAM1" => (section, sex, index) = (Some(true), None, None),
                b"MNAM" if section.is_some() => (sex, index) = (Some(false), None),
                b"FNAM" if section.is_some() => (sex, index) = (Some(true), None),
                b"INDX" => index = v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())),
                b"MODL" if section == Some(body) && sex == Some(female) && index == Some(part) => {
                    return v.as_str().unwrap_or_default().to_string();
                }
                _ => {}
            }
        }
        String::new()
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let base = |g: &Game, f: FormId| crate::functions::base_of(g, f).unwrap_or(f);
    let field = |g: &Game, f: FormId, sub: &str, path: &str| crate::nvse::record_value(g, f, sub, path);
    let of_sig = |g: &Game, f: Option<FormId>, sig: &[u8; 4]| f.filter(|&f| g.data.sig(f) == Some(*sig));
    let zero = XVal::Num(0.0);
    let v = match name {
        // `book`: 1 scroll, 2 can't be taken [documented: GetBookFlags]; SetBookFlags
        // `book flags` [documented].
        "getbookflags" => XVal::Num(of_sig(g, form(args, 0), b"BOOK").and_then(|b| field(g, b, "DATA", "flags")).and_then(|v| v.num().ok()).unwrap_or(0.0)),
        "setbookflags" => {
            if let Some(b) = of_sig(g, form(args, 0), b"BOOK") {
                // DATA's flags are one byte [documented: xEdit BOOK DATA].
                g.field_overrides.insert((b, "DATA/flags".into()), XVal::Num(f64::from(num(args, 1, 0.0).max(0.0) as u32 & 0xFF)));
            }
            zero
        }
        // `book`: the skill actor value it teaches, -1 for none [documented: GetBookSkill];
        // SetBookSkill `book code` takes 32-46 or -1 [documented].
        // The record keeps the skill's index from Barter, -1 for none; the code is 32 more
        // [observed: jg1, BookSkillSurvival reads 44]. SetBookSkill's code is kept as given.
        "getbookskill" => {
            let Some(b) = of_sig(g, form(args, 0), b"BOOK") else { return Some(Ok(XVal::Num(-1.0))) };
            if let Some(v) = g.field_overrides.get(&(b, "DATA/skill_code".into())) {
                return Some(Ok(v.clone()));
            }
            let i = field(g, b, "DATA", "skill").and_then(|v| v.num().ok()).unwrap_or(-1.0);
            XVal::Num(if i < 0.0 { -1.0 } else { i + 32.0 })
        }
        "setbookskill" => {
            let code = num(args, 1, -2.0).trunc();
            if let Some(b) = of_sig(g, form(args, 0), b"BOOK").filter(|_| code == -1.0 || (32.0..=46.0).contains(&code)) {
                g.field_overrides.insert((b, "DATA/skill_code".into()), XVal::Num(code));
            }
            zero
        }
        // `faction`: DATA's two flag bytes as one mask, bit 8 on (track crime, allow sell)
        // [documented: GetFactionFlags' table; the bytes: xEdit FACT DATA]; SetFactionFlags
        // `faction flags`.
        "getfactionflags" => {
            let Some(f) = of_sig(g, form(args, 0), b"FACT") else { return Some(Ok(zero)) };
            let b = |k: &str| field(g, f, "DATA", k).and_then(|v| v.num().ok()).unwrap_or(0.0) as u32;
            XVal::Num(f64::from(b("flags1") | (b("flags2") << 8)))
        }
        "setfactionflags" => {
            if let Some(f) = of_sig(g, form(args, 0), b"FACT") {
                let m = num(args, 1, 0.0).max(0.0) as u32;
                g.field_overrides.insert((f, "DATA/flags1".into()), XVal::Num(f64::from(m & 0xFF)));
                g.field_overrides.insert((f, "DATA/flags2".into()), XVal::Num(f64::from((m >> 8) & 0xFF)));
            }
            zero
        }
        // `creature`: DATA's combat skill [documented: GetCreatureCombatSkill].
        "getcreaturecombatskill" => {
            let c = form(args, 0).map(|f| base(g, f));
            XVal::Num(of_sig(g, c, b"CREA").and_then(|c| field(g, c, "DATA", "combat_skill")).and_then(|v| v.num().ok()).unwrap_or(0.0))
        }
        // `[actor]`: a creature's base scale (BNAM), an NPC's height (NAM6, 0 reading 1)
        // [documented: GetBaseScale, "If called on an NPC, will return their base height",
        // alias GetBasedScale]; 0 for anything else [observed: jg1, the 10mm pistol].
        "getbasescale" | "getbasedscale" => {
            let f = form(args, 0).or(subject).map(|f| base(g, f)).unwrap_or(FormId(0));
            let read = |sub| field(g, f, sub, "").and_then(|v| v.num().ok());
            XVal::Num(match g.data.sig(f) {
                Some(s) if &s == b"CREA" => read("BNAM").unwrap_or(1.0),
                Some(s) if &s == b"NPC_" => read("NAM6").filter(|&h| h > 0.0).unwrap_or(1.0),
                _ => 0.0,
            })
        }
        // [documented: Sign, 5 gives 1; observed: jg1, -2 gives -1, 0 gives 0, 0.5 gives 1].
        "sign" => {
            let x = num(args, 0, 0.0);
            XVal::Num(if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 })
        }
        // `race part female` [documented: GetRaceBodyModelPath, 0 upper body, 1 left hand,
        // 2 right hand; GetRaceHeadModelPath, 0 head … 7 right eye].
        "getracebodymodelpath" | "getraceheadmodelpath" => {
            let Some(r) = of_sig(g, form(args, 0), b"RACE") else { return Some(Ok(XVal::Str(String::new()))) };
            XVal::Str(g.race_model(r, name == "getracebodymodelpath", num(args, 2, 0.0) != 0.0, num(args, 1, -1.0) as i64))
        }
        // `weapon`: its world model, the record's MOD4, "" without one [documented:
        // GetWeaponWorldModelPath; observed: jg1, the Shishkebab's ShishkebabGO.NIF (its
        // MOD4), "" for the 10mm pistol, the combat knife and the nail board, which have
        // none]; SetWeaponWorldModelPath `weapon path`, relative to Data\Meshes
        // [documented].
        "getweaponworldmodelpath" => XVal::Str(match of_sig(g, form(args, 0), b"WEAP").and_then(|w| field(g, w, "MOD4", "")) {
            Some(XVal::Str(s)) => s,
            _ => String::new(),
        }),
        "setweaponworldmodelpath" => {
            if let Some(w) = of_sig(g, form(args, 0), b"WEAP") {
                g.field_overrides.insert((w, "MOD4/".into()), XVal::Str(text(args, 1)));
            }
            zero
        }
        // `music type`: the file (FNAM) under Data\Music, and dB (ANAM), -6 when the record
        // has none [documented: GetMusicTypePath, "Data/Music/Special/Success.mp3";
        // GetMusicTypeDB, DefaultSuccess -6; observed: jg1, "Data\Music\special\success.mp3"
        // and -6 for DefaultSuccess, which has no ANAM]; SetMusicTypeDB `type dB`.
        "getmusictypepath" => XVal::Str(match of_sig(g, form(args, 0), b"MUSC").and_then(|m| field(g, m, "FNAM", "")) {
            Some(XVal::Str(s)) => format!("Data\\Music\\{s}"),
            _ => String::new(),
        }),
        "getmusictypedb" => XVal::Num(of_sig(g, form(args, 0), b"MUSC").map_or(0.0, |m| field(g, m, "ANAM", "").and_then(|v| v.num().ok()).unwrap_or(-6.0))),
        "setmusictypedb" => {
            if let Some(m) = of_sig(g, form(args, 0), b"MUSC") {
                g.field_overrides.insert((m, "ANAM/".into()), XVal::Num(num(args, 1, 0.0)));
            }
            zero
        }
        // Legacy getters share container sound/session overrides. An explicit null wins
        // over the receiver [observed: container-sound-getters.md, native_prep_container_46].
        "getcontaineropensound" | "getcontainerclosesound" => {
            let container = if args.is_empty() { subject } else { form(args, 0) }.map_or(0, |form| form.0);
            let which = if name == "getcontaineropensound" { 0.0 } else { 1.0 };
            return call(g, ctx, "getcontainersound", None, &[XVal::Ref(container), XVal::Num(which)]);
        }
        // `container which [sound]` [documented: GetContainerSound; SetContainerSound].
        "getcontainersound" | "setcontainersound" => {
            let (Some(c), Some(sub)) = (of_sig(g, form(args, 0).map(|f| base(g, f)), b"CONT"), container_sound_sub(num(args, 1, -1.0))) else { return Some(Ok(XVal::Ref(0))) };
            // No form leaves the sound as it was [observed: jg1, container.setsound.restore].
            if name == "setcontainersound" {
                // A sound form only [documented: SetContainerSound, "Sets a Sound form"].
                if let Some(s) = form(args, 2).filter(|&s| g.data.sig(s) == Some(*b"SOUN")) {
                    g.field_overrides.insert((c, format!("{sub}/")), XVal::Ref(s.0));
                }
                zero
            } else {
                XVal::Ref(match field(g, c, sub, "") {
                    Some(XVal::Ref(r)) => r,
                    _ => 0,
                })
            }
        }
        // `template trait [value]` [documented: Get/SetLightingTemplateTraitNumeric].
        "getlightingtemplatetraitnumeric" | "setlightingtemplatetraitnumeric" => {
            let (Some(t), Some((sub, path))) = (of_sig(g, form(args, 0), b"LGTM"), lighting_trait(num(args, 1, 0.0))) else { return Some(Ok(zero)) };
            if name.starts_with("set") {
                let value = num(args, 2, 0.0);
                let prev = g.field_overrides.insert((t, format!("{sub}/{path}")), XVal::Num(value));
                let same = matches!(prev, Some(XVal::Num(p)) if p.to_bits() == value.to_bits());
                if !same {
                    g.bump_lighting_revision();
                }
                zero
            } else {
                XVal::Num(field(g, t, sub, &path).and_then(|v| v.num().ok()).unwrap_or(0.0))
            }
        }
        _ => return None,
    };
    let _ = ctx;
    Some(Ok(v))
}
