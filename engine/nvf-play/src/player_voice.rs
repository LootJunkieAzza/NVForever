//! Player voices in the game (docs/design/player-voices.md, `fnv_game::player_voice`).
//!
//! - **Sets.** Read once the data is mounted: every `nvforever\voices\*.json` of the data
//!   (the MO2 mod folder `NVForever Player Voices`; the co-op package's own sets in
//!   `coop-assets\nvforever\voices` beside the exe, laid over the data by nvforever's start:
//!   the six base-game sets, references to the game's own lines, no audio and no Nexus
//!   set [owner via parity 2026-11-23]), then every `*.json` in the folder
//!   `NVF_VOICE_SETS` names (tests, a set being reviewed). A set with the id of one read
//!   before is left out. Log: `player voices: <n> sets: <ids>`.
//! - **The character's voice.** Each frame the voice the player chose
//!   (`PlayerState::voice`, the creator's Voice row, saved with the game) is resolved
//!   against the load order into `Game::player_voice` when it changed, or when a load
//!   brought a new game (`player voice: <name> (<id>): <n> lines, rate <r>`). A set that
//!   does not resolve is said once (`player voice: <id>: ...`) and leaves the player silent.
//!   So does a voice that does not fit the character's race and sex (a saved choice, a
//!   SexChange; `player voice: <id> is a <race> <sex> voice ...`): a ghoul speaks only
//!   with a ghoul voice, each sex with its own [owner 2026-11-06]. The choice stays in the
//!   save; the creator offers only the voices that fit.
//! - **Settings** (Gameplay): First Person Quips (the player's own character says its
//!   voice's lines; `Game::quips`) and Party Quips (the other players' lines in co-op,
//!   read by the co-op code).
//! - **Preview** ([`PreviewVoice`]): the creator asks for a line of a voice; one of its
//!   first-person lines is played flat at the voice's rate (`crate::voice`).
//! - **Test aid** `NVF_PLAY_TEST_VOICE=<set id>`: the player has that voice from the start,
//!   whatever the character's race and sex.

use crate::script::ScriptState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_game::player_voice::{PlayerVoice, VoiceSet, SETS_DIR};
use std::collections::HashSet;

/// The voice sets found.
#[derive(Resource, Default)]
pub struct VoiceSets {
    pub sets: Vec<VoiceSet>,
    read: bool,
}

impl VoiceSets {
    pub fn get(&self, id: &str) -> Option<&VoiceSet> {
        self.sets.iter().find(|s| s.id == id)
    }

    /// The creator's list: female, male, then the others, each by name. A set that speaks
    /// through a plugin's actor is offered only with that plugin loaded (a follower mod's
    /// voice whose sounds are not carried: the player installs the mod).
    pub fn options(&self, data: &fnv_game::data::GameData) -> Vec<crate::creator::model::VoiceOption> {
        let rank = |s: &str| match s {
            "female" => 0,
            "male" => 1,
            _ => 2,
        };
        let mut v: Vec<&VoiceSet> = self.sets.iter().filter(|s| s.speaker.is_empty() || fnv_game::player_voice::resolve_ref(data, &s.speaker).is_some()).collect();
        v.sort_by(|a, b| (rank(&a.sex), &a.name).cmp(&(rank(&b.sex), &b.name)));
        v.into_iter().map(|s| crate::creator::model::VoiceOption { id: s.id.clone(), name: s.name.clone(), sex: s.sex.clone(), race: s.race_kind().to_string() }).collect()
    }
}

/// The quip switches of the settings page.
#[derive(Resource, Clone, Copy, Debug)]
pub struct QuipSwitches {
    /// First Person Quips.
    pub first_person: bool,
    /// Party Quips: hear the other players' quips (co-op).
    pub party: bool,
}

impl Default for QuipSwitches {
    fn default() -> Self {
        QuipSwitches { first_person: true, party: true }
    }
}

/// The player's voice is resolved into the game this frame (systems that use it run after).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Applied;

/// Play a line of voice set `id` once, flat (the creator's preview).
#[derive(Message, Clone, Debug)]
pub struct PreviewVoice {
    pub id: String,
}

/// Every voice set of the data and of `NVF_VOICE_SETS`, each id once (the first read).
/// Problems are logged.
pub fn read_set_files(vfs: &fnv_vfs::Vfs) -> Vec<VoiceSet> {
    let mut found: Vec<(String, Vec<u8>)> = Vec::new();
    for p in vfs.paths(|p| p.starts_with(SETS_DIR) && p.ends_with(".json")) {
        if let Some(Ok(b)) = vfs.read(&p) {
            found.push((p, b));
        }
    }
    if let Ok(dir) = std::env::var("NVF_VOICE_SETS") {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
        files.sort();
        for f in files.into_iter().filter(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"))) {
            match std::fs::read(&f) {
                Ok(b) => found.push((f.display().to_string(), b)),
                Err(e) => warn!("player voices: {}: {e}", f.display()),
            }
        }
    }
    let mut ids = HashSet::new();
    let mut sets = Vec::new();
    for (path, bytes) in found {
        match serde_json::from_slice::<VoiceSet>(&bytes) {
            Ok(s) if ids.insert(s.id.clone()) => sets.push(s),
            Ok(s) => info!("player voices: {path}: {} was read before; left out", s.id),
            Err(e) => warn!("player voices: {path}: not a voice set: {e}"),
        }
    }
    sets
}

fn read_sets(mut sets: ResMut<VoiceSets>, assets: Option<Res<nvf_assets::GameData>>) {
    if sets.read {
        return;
    }
    let Some(assets) = assets else { return };
    sets.read = true;
    sets.sets = read_set_files(&assets.vfs);
    let names: Vec<&str> = sets.sets.iter().map(|s| s.id.as_str()).collect();
    info!("player voices: {} sets: {}", names.len(), names.join(", "));
}

/// The voice the player chose, resolved into the game; the switches, into the game.
#[allow(clippy::too_many_arguments)]
fn apply(sets: Res<VoiceSets>, switches: Res<QuipSwitches>, mut script: ResMut<ScriptState>, mut failed: Local<Option<String>>, mut aid: Local<Option<Option<String>>>, mut aided: Local<Option<String>>, mut told: Local<Option<String>>, mut hidden_told: Local<bool>) {
    if !sets.read {
        return;
    }
    let Some(rt) = script.rt.as_mut() else { return };
    let g = &mut rt.game;
    // Sets whose speaker's plugin is not loaded (Follows-Chalk without Honest Hearts) are
    // not offered (`VoiceSets::options`): said once.
    if !*hidden_told {
        *hidden_told = true;
        for s in sets.sets.iter().filter(|s| !s.speaker.is_empty() && fnv_game::player_voice::resolve_ref(g.data, &s.speaker).is_none()) {
            info!("player voices: {} ({}) is hidden: its speaker {} is not in the load order", s.name, s.id, s.speaker);
        }
    }
    // The test aid, once there is a game.
    let aid = aid.get_or_insert_with(|| std::env::var("NVF_PLAY_TEST_VOICE").ok().filter(|v| !v.is_empty()));
    if let Some(id) = aid.take() {
        info!("test aid: the player's voice is {id} (its race and sex are not checked)");
        g.player.voice = Some(id.clone());
        *aided = Some(id);
    }
    g.quips.first_person = switches.first_person;
    // A voice that does not fit the character is not used (the test aid's is).
    let female = g.actor_female(fnv_sim::FormId::PLAYER_REF);
    let kind = fnv_game::player_voice::race_kind(g.actor_race(fnv_sim::FormId::PLAYER_REF).and_then(|r| g.data.edid_of.get(&r)).map_or("", |s| s.as_str()));
    let unfit = g.player.voice.as_deref().filter(|id| aided.as_deref() != Some(*id)).and_then(|id| sets.get(id)).filter(|s| !s.fits(kind, female));
    let want = if unfit.is_some() { None } else { g.player.voice.clone() };
    if let Some(s) = unfit {
        if told.as_deref() != Some(s.id.as_str()) {
            warn!(
                "player voice: {} is a {} {} voice and the character a {kind} {}; the player stays silent",
                s.id,
                s.race_kind(),
                s.sex,
                if female { "female" } else { "male" }
            );
            *told = Some(s.id.clone());
        }
    } else {
        *told = None;
    }
    let have = g.player_voice.as_ref().map(|v| v.id.clone());
    if want == have {
        return;
    }
    let Some(id) = want else {
        g.player_voice = None;
        info!("player voice: none (silent)");
        return;
    };
    if failed.as_deref() == Some(id.as_str()) && have.is_none() {
        return;
    }
    let Some(set) = sets.get(&id) else {
        warn!("player voice: {id}: no such voice set is installed; the player stays silent");
        g.player_voice = None;
        *failed = Some(id);
        return;
    };
    match PlayerVoice::resolve(set, g.data) {
        Ok((v, missing)) => {
            let fp = v.first_person.len();
            let events = v.by_event.len();
            info!(
                "player voice: {} ({}): {} lines in use ({fp} first person) over {events} events, rate {:.2}, voice of {}{}",
                v.name,
                v.id,
                v.lines.len(),
                v.rate,
                g.data.name(v.base),
                if missing > 0 { format!(", {missing} lines not in the load order") } else { String::new() }
            );
            g.player_voice = Some(v);
            *failed = None;
        }
        Err(e) => {
            warn!("player voice: {e}; the player stays silent");
            g.player_voice = None;
            *failed = Some(id);
        }
    }
}

pub struct PlayerVoicePlugin;

impl Plugin for PlayerVoicePlugin {
    fn build(&self, app: &mut App) {
        use crate::settings::{add_setting, Setting};
        app.init_resource::<VoiceSets>().init_resource::<QuipSwitches>().add_message::<PreviewVoice>().add_systems(Update, (read_sets, apply).chain().in_set(PlaySet::Scripts).in_set(Applied));
        add_setting(
            app,
            Setting {
                key: "first-person-quips",
                group: crate::pause::GROUP_MENUS,
                label: "First Person Quips",
                note: "On: your character says its voice's lines (chosen in the character creator) when it fights, is hurt or runs dry. Generic lines only, nothing about the story. Your friends hear you by their own Party Quips.",
                switches: "",
                get: |w| w.resource::<QuipSwitches>().first_person,
                set: |w, on| w.resource_mut::<QuipSwitches>().first_person = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "party-quips",
                group: crate::pause::GROUP_MENUS,
                label: "Party Quips",
                note: "On: in co-op you hear the other players' characters speak in their voices. Off: their lines are silent and have no subtitles.",
                switches: "",
                get: |w| w.resource::<QuipSwitches>().party,
                set: |w, on| w.resource_mut::<QuipSwitches>().party = on,
                slider: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(id: &str, name: &str) -> String {
        format!(r#"{{"format": "nvforever.voice_set/1", "id": "{id}", "name": "{name}", "sex": "female", "lines": []}}"#)
    }

    /// The co-op package's sets (`coop-assets\nvforever\voices` beside the exe, laid over the
    /// data as loose files, as nvforever's start does with `Vfs::add_loose_dir`) are read
    /// with a mod's in the data's own `nvforever\voices`; an id read before is left out
    /// [design: owner via parity 2026-11-23, the vanilla sets ship, no Nexus set does].
    #[test]
    fn the_packaged_sets_beside_the_exe_are_read_with_a_mods_sets() {
        let root = std::env::temp_dir().join(format!("nvf-voice-sets-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&root);
        let (data, pack) = (root.join("Data"), root.join("coop-assets"));
        std::fs::create_dir_all(data.join("nvforever").join("voices")).unwrap();
        std::fs::create_dir_all(pack.join("nvforever").join("voices")).unwrap();
        std::fs::write(data.join("nvforever").join("voices").join("mod.willow.json"), set("mod.willow", "Willow")).unwrap();
        std::fs::write(data.join("nvforever").join("voices").join("mod.cass-again.json"), set("vanilla.cass", "Cass (a mod's copy)")).unwrap();
        std::fs::write(pack.join("nvforever").join("voices").join("vanilla.cass.json"), set("vanilla.cass", "Cass")).unwrap();
        std::fs::write(pack.join("nvforever").join("voices").join("vanilla.boone.json"), set("vanilla.boone", "Boone")).unwrap();
        let mut vfs = fnv_vfs::Vfs::open(&data, &[]).unwrap();
        assert_eq!(vfs.add_loose_dir(&pack), 2);
        let sets = read_set_files(&vfs);
        let ids: Vec<&str> = sets.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), 3, "{ids:?}");
        for id in ["mod.willow", "vanilla.cass", "vanilla.boone"] {
            assert!(ids.contains(&id), "{id} missing from {ids:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
