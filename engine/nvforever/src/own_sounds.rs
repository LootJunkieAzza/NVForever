//! Sounds of things that are drawn here: add-on nodes, projectiles in flight and the
//! weapons in actors' hands (docs/spec-notes/audio.md §15.2).
//!
//! - **Add-on nodes** [hypothesis: the fire add-ons' `FXFireLP` is the fire's crackle]:
//!   an add-on node in the world whose ADDN names a sound (`SNAM`) is a [`SoundSource`]
//!   where it is while it shows; the ambient plays it as it plays a sound marker
//!   (sounds with neither Loop nor Play At Random, the bullet-impact add-ons', are not:
//!   the impact plays its IPCT sound).
//! - **Projectiles** (PROJ DATA Sound), by the PROJ's Supersonic flag: "If checked, the
//!   sound emanates from the point of the projectile's path closest to the listener
//!   (Think bullets whizzing by your head). If unchecked, the sound emitter follows the
//!   projectile around until it is destroyed (Think rockets flying through the air)"
//!   [documented: GECK wiki Projectile]. A supersonic one plays its sound once, at the
//!   point of its path nearest the listener ([`nearest_on`]), when it has passed it: the
//!   frame it starts to draw away after coming closer, when that point was within the
//!   sound's maximum distance [design: when]. A shot that only draws away (the player's
//!   own) never whizzes, nor one that ends on the listener (its impact sounds instead).
//!   Any other sounds its sound where it flies, going round until it is gone
//!   [hypothesis: their sounds are loops by name, `WPNPlasmaRifleProjectileLP`,
//!   `WPNPulseGunProjectileLP`, though only `FXMissileFlyBy` has the Loop flag].
//!   48 PROJ of the masters are supersonic (the bullets, `FXBulletWhiz`), 30 of the
//!   others name a sound [observed].
//! - **Idle** (WEAP `UNAM`): an actor with its weapon out sounds the weapon's idle sound
//!   at the actor, going round whatever its flags [hypothesis: 12 of the 16 have neither
//!   flag, their files are loops by name]. The player's is `nvf_play::combat`'s.
//!
//! An `own sounds summary` line at the end.

use crate::particles::AddonEmitter;
use crate::weapons::HeldWeapon;
use crate::world::WorldRoot;
use bevy::prelude::*;
use nvf_play::combat::ShotVisuals;
use nvf_play::player::Player;
use nvf_play::script::ScriptState;
use nvf_play::sfx::{PlaySound, Sfx, SoundSource};
use nvf_play::sim::SimState;
use fnv_sim::FormId;
use std::collections::{HashMap, HashSet};

/// The [`SoundSource::key`] kinds used here.
const ADDON_KIND: u8 = 10;
const PROJECTILE_KIND: u8 = 11;
const IDLE_KIND: u8 = 12;

#[derive(Resource, Default)]
struct OwnSounds {
    /// Supersonic projectiles seen, by their number (`None` once it whizzed).
    shots: HashMap<u64, Option<Passing>>,
    addons: HashSet<Entity>,
    projectile_loops: HashSet<u64>,
    whizzes: u32,
    idles: HashSet<u32>,
}

/// A supersonic projectile on its way past the listener: its distance last frame, whether
/// it has come closer, and where it was the last two frames.
#[derive(Clone, Copy, Debug)]
struct Passing {
    last: f32,
    closer: bool,
    at: [f32; 3],
    before: [f32; 3],
}

/// The point of the stretch from `a` to `b` nearest `to`.
fn nearest_on(a: [f32; 3], b: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if len2 < 1e-6 {
        return a;
    }
    let t = (((to[0] - a[0]) * ab[0] + (to[1] - a[1]) * ab[1] + (to[2] - a[2]) * ab[2]) / len2).clamp(0.0, 1.0);
    [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t]
}

fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Add-on nodes' sounds (see the module notes).
fn addons(
    mut st: ResMut<OwnSounds>,
    sfx: Option<Res<Sfx>>,
    root: Query<Entity, With<WorldRoot>>,
    emitters: Query<(Entity, &AddonEmitter, Option<&InheritedVisibility>)>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    mut out: MessageWriter<SoundSource>,
) {
    let (Some(sfx), Ok(root)) = (sfx, root.single()) else { return };
    for (e, em, v) in &emitters {
        if v.is_some_and(|v| !v.get()) {
            continue;
        }
        let Some(sound) = sfx.forms.addons.get(&em.index).and_then(|a| a.sound) else { continue };
        let at = crate::particles::game_affine(e, &transforms, &parents, root).translation;
        st.addons.insert(e);
        out.write(SoundSource { key: SoundSource::key(ADDON_KIND, e.to_bits() as u32), sound, at: at.to_array(), looping: false });
    }
}

/// Projectiles' sounds in flight (see the module notes).
fn projectiles(
    mut st: ResMut<OwnSounds>,
    sfx: Option<Res<Sfx>>,
    visuals: Res<ShotVisuals>,
    player: Option<Res<Player>>,
    bullet_snap: Option<Res<nvf_play::bullet_snap::BulletSnap>>,
    mut out: MessageWriter<SoundSource>,
    mut sounds: MessageWriter<PlaySound>,
) {
    let (Some(sfx), Some(player)) = (sfx, player) else { return };
    let st = &mut *st;
    let eye = player.eye();
    let snap_replaces_whiz = bullet_snap.as_deref().is_some_and(nvf_play::bullet_snap::BulletSnap::active);
    let mut now: HashSet<u64> = HashSet::new();
    for v in &visuals.flying {
        now.insert(v.id);
        let Some(sound) = sfx.forms.projectiles.get(&v.projectile).and_then(|p| p.sound) else { continue };
        let Some(s) = sfx.forms.sounds.get(&sound) else { continue };
        let supersonic = sfx.forms.projectiles.get(&v.projectile).is_some_and(|p| p.supersonic);
        if !supersonic {
            st.projectile_loops.insert(v.id);
            out.write(SoundSource { key: SoundSource::key(PROJECTILE_KIND, v.id as u32), sound, at: v.pos, looping: true });
            continue;
        }
        // When `BulletSnap.ini` is present and active, `nvf_play::bullet_snap` plays
        // pass-by snap audio instead of vanilla `FXBulletWhiz` (audio.md §19).
        if snap_replaces_whiz {
            continue;
        }
        let d = distance2(v.pos, eye).sqrt();
        let Some(seen) = st.shots.entry(v.id).or_insert(Some(Passing { last: d, closer: false, at: v.pos, before: v.pos })) else { continue };
        let p = *seen;
        if d < p.last {
            *seen = Passing { last: d, closer: true, before: p.at, at: v.pos };
        } else if p.closer && d > p.last {
            // The nearest point lies between where it was two frames ago and now.
            let at = nearest_on(p.before, v.pos, eye);
            let nearest = distance2(at, eye).sqrt();
            st.shots.insert(v.id, None);
            if nearest < s.max_distance {
                st.whizzes += 1;
                debug!("own sounds: projectile {} whizzes by, {nearest:.0} units from the listener: {}", v.id, s.edid.as_deref().unwrap_or("?"));
                sounds.write(PlaySound { sound, at: Some(at) });
            }
        } else {
            *seen = Passing { last: d, closer: p.closer, before: p.at, at: v.pos };
        }
    }
    st.shots.retain(|id, _| now.contains(id));
}

/// Actors' weapons' idle sounds (see the module notes).
fn idles(mut st: ResMut<OwnSounds>, sfx: Option<Res<Sfx>>, script: Option<Res<ScriptState>>, sim: Res<SimState>, held: Query<&HeldWeapon>, mut out: MessageWriter<SoundSource>) {
    let (Some(sfx), Some(rt)) = (sfx, script.as_deref().and_then(|s| s.rt.as_ref())) else { return };
    let mut done: HashSet<u32> = HashSet::new();
    for h in &held {
        if h.reference == FormId::PLAYER_REF.0 || !done.insert(h.reference) {
            continue;
        }
        let Some(sound) = sfx.forms.weapons.get(&h.weapon).and_then(|w| w.idle) else { continue };
        let me = FormId(h.reference);
        if !rt.game.extra.get(&me).is_some_and(|e| e.weapon_out) || rt.game.is_dead(me) {
            continue;
        }
        let Some(at) = sim.world.peek(me).map(|s| s.pos) else { continue };
        st.idles.insert(h.reference);
        out.write(SoundSource { key: SoundSource::key(IDLE_KIND, h.reference), sound, at, looping: true });
    }
}

fn report(mut exit: MessageReader<AppExit>, st: Res<OwnSounds>) {
    if exit.read().last().is_some() {
        info!(
            "own sounds summary: {} add-on nodes with a sound shown; {} projectiles with a looping sound, {} whizzed by; {} actors' weapons idled",
            st.addons.len(),
            st.projectile_loops.len(),
            st.whizzes,
            st.idles.len()
        );
    }
}

pub struct OwnSoundsPlugin;

impl Plugin for OwnSoundsPlugin {
    fn build(&self, app: &mut App) {
        // Before sfx and the ambient play what they send (nvf_play::sfx::SfxSet).
        // `idles` runs after PlaySet::Sync so same-frame SetWeaponSound (slot 6) and
        // script sound-source syncs reach NPC held-weapon idle loops before SfxSet plays.
        app.init_resource::<OwnSounds>()
            .add_systems(Update, (addons, projectiles).before(nvf_play::sfx::SfxSet))
            .add_systems(Update, idles.after(nvf_play::PlaySet::Sync).before(nvf_play::sfx::SfxSet))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod weapon_sound_bridge_tests {
    use super::*;
    use fnv_combat::fx::{FxForms, WeaponFx};
    use fnv_game::game::RunCtx;
    use fnv_sim::{SimConfig, World};
    use nvf_play::world_sounds::{script_weapon_sounds, WeaponSoundSync};

    const WEAP_A: FormId = FormId(0x0000_0F10);
    const WEAP_B: FormId = FormId(0x0000_0F11);
    const SND_AUTH_IDLE: FormId = FormId(0x0000_0F20);
    const SND_OVR_IDLE: FormId = FormId(0x0000_0F21);
    const NPC_BASE: FormId = FormId(0x0000_0F30);
    const NPC_A: FormId = FormId(0x0000_0F31);
    const NPC_B: FormId = FormId(0x0000_0F32);
    const CELL_ID: FormId = FormId(0x0000_0F40);

    fn sub(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(6 + data.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn rec(sig: &[u8; 4], id: u32, subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.concat();
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&body);
        out
    }

    fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(b"GRUP");
        out.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(&label);
        out.extend_from_slice(&gtype.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(body);
        out
    }

    fn test_plugin() -> fnv_esm::Plugin {
        let sounds = [
            rec(b"SOUN", SND_AUTH_IDLE.0, &[sub(b"EDID", b"AuthIdle\0"), sub(b"FNAM", b"fx\\owned\\i.wav\0")]),
            rec(b"SOUN", SND_OVR_IDLE.0, &[sub(b"EDID", b"OvrIdle\0"), sub(b"FNAM", b"fx\\owned\\i2.wav\0")]),
        ]
        .concat();
        let weapons = [
            rec(b"WEAP", WEAP_A.0, &[sub(b"EDID", b"IdleWeapA\0"), sub(b"UNAM", &SND_AUTH_IDLE.0.to_le_bytes())]),
            rec(b"WEAP", WEAP_B.0, &[sub(b"EDID", b"IdleWeapB\0"), sub(b"UNAM", &SND_AUTH_IDLE.0.to_le_bytes())]),
        ]
        .concat();
        let npcs = [
            rec(b"NPC_", 7, &[sub(b"EDID", b"Player\0")]),
            rec(b"NPC_", NPC_BASE.0, &[sub(b"EDID", b"IdleNpcBase\0")]),
        ]
        .concat();
        let cell = [
            rec(b"CELL", CELL_ID.0, &[sub(b"EDID", b"IdleCell\0")]),
            group(
                CELL_ID.0.to_le_bytes(),
                6,
                &group(
                    CELL_ID.0.to_le_bytes(),
                    8,
                    &[
                        rec(b"ACHR", NPC_A.0, &[sub(b"EDID", b"IdleNpcA\0"), sub(b"NAME", &NPC_BASE.0.to_le_bytes())]),
                        rec(b"ACHR", NPC_B.0, &[sub(b"EDID", b"IdleNpcB\0"), sub(b"NAME", &NPC_BASE.0.to_le_bytes())]),
                    ]
                    .concat(),
                ),
            ),
        ]
        .concat();
        let bytes = [
            rec(b"TES4", 0, &[sub(b"HEDR", &[0, 0, 0x80, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0])]),
            group(*b"SOUN", 0, &sounds),
            group(*b"WEAP", 0, &weapons),
            group(*b"NPC_", 0, &npcs),
            group(*b"CELL", 0, &cell),
        ]
        .concat();
        fnv_esm::Plugin::parse(&bytes, "OwnedIdleSounds.esm".into()).unwrap()
    }

    #[test]
    fn npc_weapon_idle_tracks_live_set_weapon_sound_slot_6_and_clear() {
        let plugins = Box::leak(vec![test_plugin()].into_boxed_slice());
        let mut script = ScriptState::over_plugins(plugins);
        let (forms, refs) = {
            let g = &mut script.rt.as_mut().unwrap().game;
            for npc in [NPC_A, NPC_B] {
                g.extra.entry(npc).or_default().weapon_out = true;
            }
            (g.world.forms, g.world.placed)
        };
        let mut world = World::new(forms, refs, SimConfig::default());
        for npc in [NPC_A, NPC_B] {
            world.get(npc).unwrap().pos = [100.0, 0.0, 0.0];
        }

        let mut fx = FxForms::default();
        fx.weapons.insert(WEAP_A, WeaponFx { idle: Some(SND_AUTH_IDLE), ..WeaponFx::default() });
        fx.weapons.insert(WEAP_B, WeaponFx { idle: Some(SND_AUTH_IDLE), ..WeaponFx::default() });

        let mut app = App::new();
        app.init_resource::<OwnSounds>()
            .init_resource::<WeaponSoundSync>()
            .insert_resource(script)
            .insert_resource(SimState { forms, refs, world })
            .insert_resource(Sfx::new(fx))
            .add_message::<SoundSource>()
            .add_systems(Update, (script_weapon_sounds, idles).chain());

        app.world_mut().spawn(HeldWeapon { reference: NPC_A.0, weapon: WEAP_A });
        app.world_mut().spawn(HeldWeapon { reference: NPC_B.0, weapon: WEAP_B });

        // Baseline: both NPCs emit authored UNAM idle sound.
        app.update();
        let emitted: HashMap<u64, FormId> = app.world_mut().resource_mut::<Messages<SoundSource>>().drain().map(|s| (s.key, s.sound)).collect();
        assert_eq!(emitted.get(&SoundSource::key(IDLE_KIND, NPC_A.0)), Some(&SND_AUTH_IDLE));
        assert_eq!(emitted.get(&SoundSource::key(IDLE_KIND, NPC_B.0)), Some(&SND_AUTH_IDLE));

        // Compiled NVSE SetWeaponSound slot 6 overrides IdleWeapA while leaving IdleWeapB untouched.
        app.world_mut()
            .resource_mut::<ScriptState>()
            .rt
            .as_mut()
            .unwrap()
            .game
            .run_query("Begin Function {}\nSetWeaponSound IdleWeapA 6 OvrIdle\nEnd", RunCtx::default())
            .unwrap();
        app.update();
        let emitted: HashMap<u64, FormId> = app.world_mut().resource_mut::<Messages<SoundSource>>().drain().map(|s| (s.key, s.sound)).collect();
        assert_eq!(emitted.get(&SoundSource::key(IDLE_KIND, NPC_A.0)), Some(&SND_OVR_IDLE));
        assert_eq!(emitted.get(&SoundSource::key(IDLE_KIND, NPC_B.0)), Some(&SND_AUTH_IDLE));

        // Clearing IdleWeapA slot 6 to 0 suppresses NPC_A's idle source while NPC_B stays active.
        app.world_mut()
            .resource_mut::<ScriptState>()
            .rt
            .as_mut()
            .unwrap()
            .game
            .run_query("Begin Function {}\nSetWeaponSound IdleWeapA 6 0\nEnd", RunCtx::default())
            .unwrap();
        app.update();
        let emitted: HashMap<u64, FormId> = app.world_mut().resource_mut::<Messages<SoundSource>>().drain().map(|s| (s.key, s.sound)).collect();
        assert!(!emitted.contains_key(&SoundSource::key(IDLE_KIND, NPC_A.0)), "cleared idle slot stops emitting SoundSource");
        assert_eq!(emitted.get(&SoundSource::key(IDLE_KIND, NPC_B.0)), Some(&SND_AUTH_IDLE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_combat::fx::{FxForms, ProjectileFx, Sound};
    use nvf_play::bullet_snap::BulletSnap;
    use nvf_play::combat::ShotVisual;

    #[test]
    fn nearest_point_of_a_pass() {
        // A shot along x passing 50 units from a listener at the origin.
        assert_eq!(nearest_on([-100.0, 50.0, 0.0], [100.0, 50.0, 0.0], [0.0; 3]), [0.0, 50.0, 0.0]);
        // Wholly before the listener: the end nearer it.
        assert_eq!(nearest_on([-300.0, 50.0, 0.0], [-100.0, 50.0, 0.0], [0.0; 3]), [-100.0, 50.0, 0.0]);
        assert_eq!(nearest_on([5.0; 3], [5.0; 3], [0.0; 3]), [5.0; 3]);
    }

    fn whiz_app(snap: BulletSnap) -> (App, FormId, FormId, FormId, FormId) {
        let proj_sup = FormId(0x0001_0001);
        let proj_loop = FormId(0x0001_0002);
        let snd_whiz = FormId(0x0002_0001);
        let snd_loop = FormId(0x0002_0002);
        let mut fx = FxForms::default();
        fx.sounds.insert(snd_whiz, Sound { edid: Some("FXBulletWhiz".into()), max_distance: 2000.0, ..Default::default() });
        fx.sounds.insert(snd_loop, Sound { edid: Some("FXMissileFlyBy".into()), max_distance: 2000.0, ..Default::default() });
        fx.projectiles.insert(proj_sup, ProjectileFx { sound: Some(snd_whiz), supersonic: true, ..Default::default() });
        fx.projectiles.insert(proj_loop, ProjectileFx { sound: Some(snd_loop), supersonic: false, ..Default::default() });

        let mut app = App::new();
        app.init_resource::<OwnSounds>()
            .init_resource::<ShotVisuals>()
            .insert_resource(Sfx::new(fx))
            .insert_resource(Player { heading: 0.0, pitch: 0.0, eye_height: 64.0, feet: [0.0, 0.0, 0.0], body_eye: None, body_view: None, placed: None })
            .insert_resource(snap)
            .add_message::<SoundSource>()
            .add_message::<PlaySound>()
            .add_systems(Update, projectiles);
        (app, proj_sup, proj_loop, snd_whiz, snd_loop)
    }

    #[test]
    fn inactive_bullet_snap_preserves_vanilla_whiz_and_active_replaces_it_without_touching_loops() {
        // Inactive BulletSnap (vanilla): supersonic bullet emits vanilla FXBulletWhiz.
        let (mut app, proj_sup, _, snd_whiz, _) = whiz_app(BulletSnap::inactive());
        let eye_z = app.world().resource::<Player>().eye()[2];
        for x in [-300.0f32, -100.0, 200.0] {
            app.world_mut().resource_mut::<ShotVisuals>().flying = vec![
                ShotVisual { id: 1, projectile: proj_sup, owner: FormId(0x100), pos: [x, 100.0, eye_z], dir: [1.0, 0.0, 0.0], beam: None, travelled: x + 300.0, strength: 1.0 },
            ];
            app.update();
        }
        let vanilla: Vec<PlaySound> = app.world_mut().resource_mut::<Messages<PlaySound>>().drain().collect();
        assert_eq!(vanilla.len(), 1);
        assert_eq!(vanilla[0].sound, snd_whiz);

        // Active BulletSnap: vanilla FXBulletWhiz is suppressed while non-supersonic loop still emits.
        let dir = std::env::temp_dir().join(format!("nvf-own-sounds-snap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ini = dir.join("NVSE").join("Plugins").join("BulletSnapConfig").join("BulletSnap.ini");
        std::fs::create_dir_all(ini.parent().unwrap()).unwrap();
        std::fs::write(&ini, "[General]\nDistancedBasedSnap = 1\n").unwrap();
        let wav = dir.join("Sound").join("fx").join("GeneralCloseMid").join("Close").join("a.wav");
        std::fs::create_dir_all(wav.parent().unwrap()).unwrap();
        std::fs::write(&wav, b"RIFF").unwrap();
        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let active = BulletSnap::from_vfs(&vfs);
        assert!(active.active());

        let (mut app, proj_sup, proj_loop, _, snd_loop) = whiz_app(active);
        let eye_z = app.world().resource::<Player>().eye()[2];
        for x in [-300.0f32, -100.0, 200.0] {
            app.world_mut().resource_mut::<ShotVisuals>().flying = vec![
                ShotVisual { id: 1, projectile: proj_sup, owner: FormId(0x100), pos: [x, 100.0, eye_z], dir: [1.0, 0.0, 0.0], beam: None, travelled: x + 300.0, strength: 1.0 },
                ShotVisual { id: 2, projectile: proj_loop, owner: FormId(0x100), pos: [x, 100.0, eye_z], dir: [1.0, 0.0, 0.0], beam: None, travelled: x + 300.0, strength: 1.0 },
            ];
            app.update();
        }
        let suppressed: Vec<PlaySound> = app.world_mut().resource_mut::<Messages<PlaySound>>().drain().collect();
        assert!(suppressed.is_empty(), "active BulletSnap suppresses duplicate vanilla FXBulletWhiz");
        let loops: Vec<SoundSource> = app.world_mut().resource_mut::<Messages<SoundSource>>().drain().collect();
        assert!(loops.iter().any(|s| s.sound == snd_loop), "non-supersonic projectile loops remain active");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
