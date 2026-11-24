//! Weapon bash (the fork; `fnv_combat::bash` has the rules, from B42 Melee Bash): with a
//! gun drawn, the Weapon Bash control (X) strikes with it as a club.
//!
//! - **Starting** ([`tick`]): the gun drawn and in hand, no menu, VATS or aim down the
//!   sights, not mid-bash or in its cooldown, and B42 Melee Bash's clips installed
//!   ([`WeaponBash::clips`], set by nvforever's `consume_anims` when it reads them). A
//!   [`BashCue`] asks for the clip; with fewer than `AP_COST` Action Points it asks for the
//!   fail clip and strikes nothing.
//! - **The blow**: when the clip's `B42BashHitFrameDetect` key passes the clip player
//!   writes [`BashStrike`]; the Action Points are spent then and the melee strike
//!   (`crate::combat`'s `melee`) deals the blow with [`WeaponBash::strike`]'s weapon: the
//!   base melee weapon of the gun's kind, its damage the mod's, the gun's for everything
//!   else (its impact, the hit's weapon).
//! - **The end**: [`BashOver`] when the clip ends (or it could not play), else
//!   `LONGEST` seconds after the start; the next may start `COOLDOWN` later.
//!
//! Log: `bash: …`; at exit `bash summary: …`.

use bevy::prelude::*;
use fnv_combat::bash::{self as rules, BashBlow};
use fnv_sim::FormId;

use crate::bindings::{Control, ControlInput};
use crate::combat::Combat;
use crate::control::PlayerControl;
use crate::menu::ActiveMenu;
use crate::script::{with_world, ScriptState};
use crate::sfx::PlaySound;
use crate::sim::SimState;

/// Play a bash clip (`fail`: the no-Action-Points one) for weapons of `clip_type`.
#[derive(Message, Clone, Copy, Debug)]
pub struct BashCue {
    pub clip_type: i64,
    pub fail: bool,
}

/// The bash clip's blow lands now.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct BashStrike;

/// The bash clip is over (or did not play).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct BashOver;

#[derive(Debug, Clone)]
struct Active {
    age: f32,
    gun: FormId,
    blow: BashBlow,
    fail: bool,
    struck: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BashStats {
    pub bashes: u32,
    pub fails: u32,
    pub strikes: u32,
    pub refused: u32,
}

#[derive(Resource, Default, Debug)]
pub struct WeaponBash {
    /// B42 Melee Bash's clips are installed (set by the clip player).
    pub clips: bool,
    active: Option<Active>,
    cooldown: f32,
    /// The blow to strike this frame: the gun and what it strikes as (taken by combat).
    pub(crate) strike: Option<(FormId, BashBlow)>,
    pub stats: BashStats,
    told_no_clips: bool,
}

impl WeaponBash {
    /// A bash plays.
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
}

pub struct BashPlugin;

impl Plugin for BashPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponBash>().add_message::<BashCue>().add_message::<BashStrike>().add_message::<BashOver>().add_systems(Last, report);
    }
}

/// The perks of the mod the player has: Basher ranks, Piercing Bash, Super Slam.
fn perks(rt: &crate::script::Runtime) -> (u8, bool, bool) {
    let g = &rt.game;
    let held: Vec<FormId> = g.held_perks(FormId::PLAYER_REF).into_iter().map(|(p, _)| p).collect();
    let has = |edid: &str| g.data.by_edid(edid).is_some_and(|p| held.contains(&p));
    let basher = rules::PERK_BASHER.iter().filter(|e| has(e)).count() as u8;
    (basher, has(rules::PERK_PIERCING), has(rules::SUPER_SLAM))
}

/// Starts a bash on the key, spends its Action Points at the blow, ends it (module notes).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick(
    time: Res<Time>,
    input: ControlInput,
    mut bash: ResMut<WeaponBash>,
    c: Res<Combat>,
    control: Res<PlayerControl>,
    active_menu: Option<Res<ActiveMenu>>,
    vats: Option<Res<crate::vats_mode::VatsMode>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    (mut cues, mut strikes, mut overs, mut sounds): (MessageWriter<BashCue>, MessageReader<BashStrike>, MessageReader<BashOver>, MessageWriter<PlaySound>),
) {
    let dt = time.delta_secs();
    let b = &mut *bash;
    b.cooldown = (b.cooldown - dt).max(0.0);
    // The mod's own quest scripts would bash too (and hold the controls): this bash
    // replaces them, so they stay stopped (its perks still count).
    if b.clips {
        if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
            for edid in rules::MOD_QUESTS {
                if let Some(q) = rt.game.data.by_edid(edid) {
                    if rt.game.quests.get(&q).is_some_and(|s| s.running) {
                        rt.game.stop_quest(q);
                        info!("bash: {edid} (B42Bash.esp's own script) stopped: the native bash plays instead");
                    }
                }
            }
        }
    }
    let struck_now = strikes.read().count() > 0;
    let over = overs.read().count() > 0;
    if let Some(a) = b.active.as_mut() {
        a.age += dt;
        let strike = !a.fail && !a.struck && (struck_now || (!b.clips && a.age >= rules::STRIKE_FALLBACK));
        if strike {
            a.struck = true;
            b.stats.strikes += 1;
            if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
                with_world(rt, &mut sim, |rt| fnv_game::functions::add_actor_value_damage(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::ACTION_POINTS, f64::from(rules::AP_COST)));
            }
            info!("bash: the blow lands ({:.2} s in; {} AP spent)", a.age, rules::AP_COST);
            b.strike = Some((a.gun, a.blow.clone()));
        }
        if over || a.age >= rules::LONGEST {
            info!("bash: over ({}, {:.2} s){}", if over { "the clip ended" } else { "no clip end in time" }, a.age, if a.struck || a.fail { "" } else { ": it never struck" });
            b.active = None;
            b.cooldown = rules::COOLDOWN;
        }
        return;
    }
    if !input.just_pressed(Control::Bash) {
        return;
    }
    let refuse = |b: &mut WeaponBash, why: &str| {
        b.stats.refused += 1;
        info!("bash: not now ({why})");
    };
    if !b.clips {
        if !b.told_no_clips {
            b.told_no_clips = true;
            info!("bash: B42 Melee Bash's clips are not installed: no weapon bash");
        }
        return;
    }
    if b.cooldown > 0.0 {
        return refuse(b, "cooling down");
    }
    // Nor under a match's end view and its fade (`PlayerControl::end_view`).
    if active_menu.as_deref().is_some_and(|m| m.0.is_some()) || control.cinematic || control.portrait.is_some() || control.end_view {
        return refuse(b, "a menu or a scene");
    }
    if vats.as_deref().is_some_and(|v| v.active()) {
        return refuse(b, "VATS");
    }
    if !control.weapon_out || control.aiming {
        return refuse(b, if control.aiming { "aiming down the sights" } else { "no weapon drawn" });
    }
    let Some(gun) = c.gun() else { return refuse(b, "not a gun") };
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let Some(weapon_type) = rt.game.data.weapon_anim_types.get(&gun).copied() else { return refuse(b, "the gun has no type") };
    let Some(clip_type) = rules::clip_type(weapon_type) else { return refuse(b, "this weapon does not bash") };
    let base_edid = rules::base_weapon(clip_type);
    let Some(base) = rt.game.data.by_edid(base_edid).and_then(|id| c.forms.weapons.get(&id)) else { return refuse(b, "no base weapon") };
    let weight = c.forms.weapons.get(&gun).map_or(0.0, |w| w.weight);
    let (basher, piercing, _) = perks(rt);
    let blow = BashBlow::of(base, weight, basher, piercing);
    let ap = crate::combat::actor_av(script.as_deref_mut(), &mut sim, FormId::PLAYER_REF, fnv_game::actor_values::ACTION_POINTS).unwrap_or(0.0);
    let fail = ap < rules::AP_COST;
    let gun_edid = c.forms.weapons.get(&gun).and_then(|w| w.edid.clone()).unwrap_or_else(|| gun.to_string());
    if fail {
        b.stats.fails += 1;
        info!("bash: {gun_edid}: {ap:.0} AP, {} needed: the fail clip", rules::AP_COST);
    } else {
        b.stats.bashes += 1;
        info!("bash: {gun_edid} (type {weapon_type}, clips {clip_type}) bashes as {base_edid}: {} damage (Basher {basher}{})", blow.stats.base_damage, if piercing { ", Piercing Bash" } else { "" });
        if let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) {
            if let Some(sound) = rt.game.data.by_edid("WPNArmSwing") {
                sounds.write(PlaySound { sound, at: None });
            }
        }
    }
    cues.write(BashCue { clip_type, fail });
    b.active = Some(Active { age: 0.0, gun, blow, fail, struck: false });
}

fn report(mut exit: MessageReader<AppExit>, b: Res<WeaponBash>) {
    if exit.read().last().is_some() {
        let s = b.stats;
        info!("bash summary: {} bashes ({} struck), {} without the Action Points, {} refused; clips {}", s.bashes, s.strikes, s.fails, s.refused, if b.clips { "installed" } else { "not installed" });
    }
}
