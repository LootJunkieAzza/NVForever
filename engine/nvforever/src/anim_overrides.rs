//! kNVSE's AnimGroupOverride files as clip replacements (docs/spec-notes/knvse-anim-folders.md):
//! the files a mod gives, by the clip name they stand in for ([`override_stems`]), and
//! the file that plays for a clip name now ([`pick`]). The player's rigs take their list
//! from the game every frame (`player_body.rs`). The other bodies (NPCs, and the co-op
//! puppets, which have no actor in this game) take the files the folders give their
//! weapon with no condition ([`HeldOverrides`], `fnv_game` `weapon_anim_files`): their
//! stance's aim and draw clips (`weapons::Stance::load_with`), and their attack, reload
//! and aim clips (`attack_anims.rs`) [hypothesis: an entry naming a weapon form applies
//! to any actor holding that weapon, not only the player; conditions are left out, as a
//! puppet has no actor in this game to test them on, so the last-shot folders play for
//! the player only].

use bevy::prelude::*;
use fnv_actor::player_clips::{aim_variant, Aim};
use fnv_sim::FormId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A weapon's third-person override files by clip name ([`override_stems`]).
pub type ByStem = HashMap<String, Vec<String>>;

/// Per weapon form: the third-person files of the folders that name it with no
/// condition, by clip name. Filled once the game has read the folders ([`share`]).
#[derive(Default, Clone)]
pub struct HeldOverrides {
    pub map: Arc<Mutex<HashMap<FormId, Arc<ByStem>>>>,
    filled: Arc<AtomicBool>,
}

impl HeldOverrides {
    /// The files for `weapon`, if a folder names it.
    pub fn for_weapon(&self, weapon: Option<FormId>) -> Option<Arc<ByStem>> {
        let w = weapon?;
        self.map.lock().ok()?.get(&w).cloned()
    }

    /// Whether the folders have been read into it.
    pub fn filled(&self) -> bool {
        self.filled.load(Ordering::Relaxed)
    }
}

pub struct AnimOverridesPlugin;

impl Plugin for AnimOverridesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, share);
    }
}

/// Fill [`HeldOverrides`] from the game's folders, once they are read.
fn share(script: Option<Res<nvf_play::script::ScriptState>>, world: Option<Res<crate::actors::ActorWorld>>) {
    let Some(world) = world else { return };
    let held = &world.held;
    if held.filled() {
        return;
    }
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let Some(files) = game.weapon_anim_files(false) else { return };
    let map: HashMap<FormId, Arc<ByStem>> = files.into_iter().map(|(w, list)| (w, Arc::new(override_stems(&list)))).filter(|(_, m)| !m.is_empty()).collect();
    info!("anim overrides: {} form(s) with third-person files no condition holds back (NPCs and co-op puppets holding them)", map.len());
    if let Ok(mut m) = held.map.lock() {
        *m = map;
    }
    held.filled.store(true, Ordering::Relaxed);
}

/// The file that plays for the first of `names` (clip file names or paths) that a set in
/// `by_stem` replaces: of an `_order_N` set the one whose turn it is (`turn`, counted in
/// the set). An archive path (`meshes\…`).
pub fn pick(by_stem: &ByStem, names: &[String], turn: usize) -> Option<String> {
    names.iter().find_map(|n| {
        let set = by_stem.get(&clip_stem(n))?;
        set.get(turn % set.len().max(1)).map(|p| format!("meshes\\{p}"))
    })
}

/// Whether any of `names` is replaced (whatever the turn).
pub fn replaces(by_stem: &ByStem, names: &[String]) -> bool {
    names.iter().any(|n| by_stem.contains_key(&clip_stem(n)))
}

/// A clip file's stem for override matching: its name without folder and `.kf`, lower
/// case, up to its first space (`1hpReloadL v1.6.kf`, `1hpEquip Combat.kf`,
/// `1hpAttackRight last.kf` [observed: Another Millenia's AnimGroupOverride files,
/// 2026-11-06]).
pub(crate) fn clip_stem(file: &str) -> String {
    let name = file.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
    let name = name.strip_suffix(".kf").unwrap_or(&name);
    name.split(' ').next().unwrap_or("").to_string()
}

/// The files that replace the rig's clips, by [`clip_stem`]: a later folder wins, but
/// within one override folder a plain name (`1hpEquip.kf`) beats one with words after a
/// space (`1hpEquip Combat.kf`) [hypothesis: the spaced ones are a mod's alternatives].
/// Files ending `_order_N` make one set of the stem before that ending, played in turn by
/// N ([`Rig::set_turn`]; documented: kNVSE's Nexus page); a file of that stem without the
/// ending, in the same folder, comes first in the set [hypothesis: Another Millenia's
/// `Sneak1hpAttackRightDOWN.kf` beside `Sneak1hpAttackRightDOWN_order_2.kf` stands for
/// order 1]. A later folder with the stem replaces the whole set [hypothesis: the files
/// come from the one folder that wins; the akimbo sets' last-shot folders hold one file
/// of the set each, which then plays alone while its condition holds]. `_hurt` files are
/// left out [hypothesis: played while hurt; not done]. Paths are under meshes.
pub(crate) fn override_stems(paths: &[String]) -> HashMap<String, Vec<String>> {
    // By stem: the folder, and its files (order number, path, a plain name).
    type Files = (String, Vec<(u32, String, bool)>);
    let mut out: HashMap<String, Files> = HashMap::new();
    for p in paths {
        let p = p.replace('/', "\\");
        let name = p.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
        if !name.ends_with(".kf") {
            continue;
        }
        let (stem, order) = order_of(&clip_stem(&name));
        if stem.is_empty() || stem.contains("_hurt") {
            continue;
        }
        let exact = !name.contains(' ');
        let lower = p.to_ascii_lowercase();
        let folder = ["\\_1stperson\\", "\\_male\\"].iter().find_map(|v| lower.find(v)).map_or(lower.clone(), |i| lower[..i].to_string());
        let entry = out.entry(stem).or_insert_with(|| (folder.clone(), Vec::new()));
        if entry.0 != folder {
            *entry = (folder, Vec::new());
        }
        let files = &mut entry.1;
        match files.iter().position(|f| f.0 == order) {
            Some(i) if files[i].2 && !exact => {}
            Some(i) => files[i] = (order, p, exact),
            None => files.push((order, p, exact)),
        }
    }
    out.into_iter()
        .map(|(k, (_, mut files))| {
            files.sort_by_key(|f| f.0);
            (k, files.into_iter().map(|f| f.1).collect())
        })
        .collect()
}

/// A stem without its `_order_N` ending, and N (0 for a stem without one).
pub(crate) fn order_of(stem: &str) -> (String, u32) {
    match stem.rsplit_once("_order_").and_then(|(base, n)| Some((base, n.parse::<u32>().ok()?))) {
        Some((base, n)) => (base.to_string(), n),
        None => (stem.to_string(), 0),
    }
}

/// The up or down form of clip file `path` ([`aim_variant`]), the aim's ending going
/// before an `_order_N` one: `1hpAttackRightUP_order_1.kf` [observed: Another Millenia's
/// akimbo `_male` files].
pub(crate) fn ordered_aim_variant(path: &str, aim: Aim) -> String {
    let stem = &path[..path.len() - if path.to_ascii_lowercase().ends_with(".kf") { 3 } else { 0 }];
    let lower = stem.to_ascii_lowercase();
    match lower.rfind("_order_").filter(|&i| lower[i + 7..].parse::<u32>().is_ok()) {
        Some(i) => aim_variant(&stem[..i], aim).replacen(".kf", &format!("{}.kf", &stem[i..]), 1),
        None => aim_variant(path, aim),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Another Millenia's akimbo third-person files.
    fn akimbo() -> ByStem {
        let f = |n: &str| format!("AnimGroupOverride\\Johnsonn_9mmAkimbo\\_Male\\{n}");
        override_stems(&[f("1hpAim.kf"), f("1hpAimUP.kf"), f("1hpEquip.kf"), f("1hpReloadL.kf"), f("1hpAttackRight_order_1.kf"), f("1hpAttackRight_order_2.kf"), f("1hpAttackRightUP_order_1.kf"), f("Sneak1hpAim.kf")])
    }

    #[test]
    fn a_held_weapons_files_stand_in_for_the_stance_clips_by_name() {
        let m = akimbo();
        let aim = pick(&m, &["meshes\\characters\\_male\\1hpaim.kf".to_string()], 0);
        assert_eq!(aim.as_deref(), Some("meshes\\AnimGroupOverride\\Johnsonn_9mmAkimbo\\_Male\\1hpAim.kf"));
        assert_eq!(pick(&m, &["sneak1hpaim.kf".to_string()], 0).as_deref(), Some("meshes\\AnimGroupOverride\\Johnsonn_9mmAkimbo\\_Male\\Sneak1hpAim.kf"));
        // No file for the holster clip: the stance's own plays.
        assert!(pick(&m, &["1hpholster.kf".to_string()], 0).is_none());
        assert!(replaces(&m, &["1hpholster.kf".to_string(), "1hpequip.kf".to_string()]));
        assert!(!replaces(&m, &["1hpholster.kf".to_string(), "1hpunequip.kf".to_string()]));
    }

    #[test]
    fn an_order_set_goes_round_by_the_turn_and_the_first_named_wins() {
        let m = akimbo();
        let names = ["sneak1hpattackright.kf".to_string(), "1hpattackright.kf".to_string()];
        let one = pick(&m, &names, 0).unwrap();
        let two = pick(&m, &names, 1).unwrap();
        assert!(one.ends_with("1hpAttackRight_order_1.kf") && two.ends_with("1hpAttackRight_order_2.kf"), "{one} {two}");
        assert_eq!(pick(&m, &names, 2), Some(one.clone()));
        // Its up form, the aim's word before `_order_N`.
        assert!(ordered_aim_variant(&one, Aim::Up).to_ascii_lowercase().ends_with("1hpattackrightup_order_1.kf"));
        // A reload by its plain name.
        assert!(pick(&m, &["1hpreloadl.kf".to_string()], 5).unwrap().ends_with("1hpReloadL.kf"));
    }

    /// A body holding the Dual 9mm Police Pistols (add-on 2A0B) takes the akimbo aim and
    /// equip clips, which put the model's `Akimbo` node (its second gun) in the left hand;
    /// one holding the 9mm Police Pistol (mil.esp 5480A) takes only its own one-gun set,
    /// and the game's 9mm pistol, which no entry names, the stance's own clips.
    #[test]
    fn only_a_dual_weapon_holds_the_akimbo_clips() {
        let (dual, single, vanilla) = (FormId(0x1C00_2A0B), FormId(0x1B05_480A), FormId(0x000E_3778));
        let held = HeldOverrides::default();
        let glock = override_stems(&["AnimGroupOverride\\Rock_AM_Glock\\_male\\1hpReloadL.kf".to_string()]);
        *held.map.lock().unwrap() = HashMap::from([(dual, Arc::new(akimbo())), (single, Arc::new(glock))]);
        let stance = ["1hpaim.kf".to_string(), "1hpequip.kf".to_string()];
        let akimbo_clip = |w: FormId| held.for_weapon(Some(w)).is_some_and(|m| stance.iter().any(|n| pick(&m, std::slice::from_ref(n), 0).is_some_and(|p| p.contains("Akimbo"))));
        assert!(akimbo_clip(dual));
        assert!(!akimbo_clip(single));
        let s = held.for_weapon(Some(single)).unwrap();
        assert!(!replaces(&s, &stance), "the single pistol's aim and equip stay the stance's");
        assert!(pick(&s, &["1hpreloadl.kf".to_string()], 0).unwrap().contains("Rock_AM_Glock"));
        assert!(held.for_weapon(Some(vanilla)).is_none());
        assert!(held.for_weapon(None).is_none());
    }
}
