//! Native `BulletSnap.ini` pass-by snap audio (docs/spec-notes/audio.md §19).
//!
//! Reads `Data\NVSE\Plugins\BulletSnapConfig\BulletSnap.ini` from the VFS at startup.
//! When the INI is absent (unmodded vanilla), this module stays completely inactive: no
//! folders are scanned, no synthetic sounds are registered, and zero snap audio is
//! emitted.

use bevy::prelude::*;
use fnv_combat::bullet_snap::{
    pick_folder, BulletSnapConfig, SnapBand, SnapKind, SnapTracker, SoundFolder, BULLET_SNAP_INI_PATH,
};
use fnv_combat::vec::V3;
use fnv_sim::FormId;

use crate::player::Player;
use crate::sfx::{ScaledSound, Sfx, SfxSet};
use crate::PlaySet;

/// Reserved synthetic `SOUN` FormID range (`[BULLET_SNAP_SOUND_BASE,
/// BULLET_SNAP_SOUND_END)`) for BulletSnap's 10 sound folders [design: placed in
/// `0xFE7E_0000..0xFE7F_0000`, recorded in `docs/spec-notes/callable-identity.md` and
/// bounded in `fnv_game::game` strictly above `TEMP_REF_BASE..BULLET_SNAP_SOUND_BASE`
/// (`0xFE40_0000..0xFE7E_0000`), below `CAMERA_OVERLAY_BASE` (`0xFE7F_0000`),
/// `LAMBDA_BASE` / `MADE_PACKAGE_BASE` (`0xFE80_0000`), and `FormId::FIRST_DYNAMIC`
/// (`0xFF00_0000`), and disjoint from normal plugin indices `0x00..=0xFD`, `CLONE_BASE`
/// (`0xFE00_0000..0xFE20_0000`), and `COMPILED_BASE` (`0xFE20_0000..0xFE40_0000`).
/// Registration never overwrites an existing loaded `SOUN` record: any loaded record at
/// the same ID wins and BulletSnap skips to the first free slot in
/// `[BULLET_SNAP_SOUND_BASE + 10, BULLET_SNAP_SOUND_END)`].
pub const BULLET_SNAP_SOUND_BASE: u32 = fnv_game::game::BULLET_SNAP_SOUND_BASE;
/// Exclusive upper bound of BulletSnap's reserved synthetic `SOUN` FormID range [design].
pub const BULLET_SNAP_SOUND_END: u32 = fnv_game::game::CAMERA_OVERLAY_BASE;

/// Attenuation curve for BulletSnap synthetic sounds [design: keeps distant snaps
/// audible out to the configured `Far` threshold while still attenuating with distance].
pub const SNAP_ATTENUATION_CURVE: [f32; 5] = [100.0, 80.0, 60.0, 40.0, 15.0];

/// Per-frame projectile movement/impact fed from `crate::combat::fly` when BulletSnap is
/// active.
#[derive(Debug, Clone, Copy)]
pub enum SnapInput {
    Segment {
        id: u64,
        from: V3,
        to: V3,
        dir: V3,
        speed: f32,
        damage: f32,
        is_player_owned: bool,
        eligible: bool,
    },
    PlayerImpact {
        id: u64,
        at: V3,
        speed: f32,
        damage: f32,
        is_player_owned: bool,
        eligible: bool,
    },
}

impl SnapInput {
    pub fn id(&self) -> u64 {
        match *self {
            SnapInput::Segment { id, .. } | SnapInput::PlayerImpact { id, .. } => id,
        }
    }
}

/// Summary counters (logged at exit only when `BulletSnap.ini` was loaded).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BulletSnapStats {
    pub triggered: u32,
    pub supersonic: u32,
    pub subsonic: u32,
    pub player_impacts: u32,
    pub very_close: u32,
    pub close: u32,
    pub distance: u32,
    pub far: u32,
}

/// Runtime state for `BulletSnap.ini` pass-by audio.
#[derive(Resource, Default)]
pub struct BulletSnap {
    /// Parsed config when `BulletSnap.ini` was present in the VFS; `None` in vanilla.
    config: Option<BulletSnapConfig>,
    /// Pre-listed `.wav`/`.ogg` VFS paths for each of the 10 [`SoundFolder`] slots.
    folder_files: [Vec<String>; 10],
    /// Assigned synthetic SOUN FormID for each non-empty folder slot.
    folder_sounds: [Option<FormId>; 10],
    /// True once non-empty folders have been registered on [`Sfx`].
    registered_in_sfx: bool,
    tracker: SnapTracker,
    inputs: Vec<SnapInput>,
    /// Finished projectile IDs still referenced by queued `inputs`: kept through this
    /// frame's [`tick`] so `snapped` state is preserved, then pruned immediately after.
    prune_after_tick: Vec<u64>,
    pub stats: BulletSnapStats,
}

impl BulletSnap {
    /// Inactive resource (used when `BulletSnap.ini` is absent).
    pub fn inactive() -> BulletSnap {
        BulletSnap::default()
    }

    /// Load `BulletSnap.ini` and index available BulletSnap audio folders from `vfs`.
    /// Returns [`BulletSnap::inactive`] when the INI is absent.
    pub fn from_vfs(vfs: &fnv_vfs::Vfs) -> BulletSnap {
        let Some(read) = vfs.read(BULLET_SNAP_INI_PATH) else {
            return BulletSnap::inactive();
        };
        let bytes = match read {
            Ok(b) => b,
            Err(e) => {
                warn!("bullet snap: could not read {BULLET_SNAP_INI_PATH}: {e}");
                return BulletSnap::inactive();
            }
        };
        let config = BulletSnapConfig::parse_bytes(&bytes);
        let mut snap = BulletSnap { config: Some(config), ..Default::default() };
        let mut found_files = 0usize;
        for folder in SoundFolder::ALL {
            let prefix = folder.vfs_folder();
            let files = vfs.paths(|p| {
                p.starts_with(prefix)
                    && (p.ends_with(".wav") || p.ends_with(".ogg"))
                    && !p[prefix.len()..].contains('\\')
            });
            if !files.is_empty() {
                found_files += files.len();
                let idx = folder.index();
                snap.folder_files[idx] = files;
                snap.folder_sounds[idx] = Some(FormId(BULLET_SNAP_SOUND_BASE + idx as u32));
            }
        }
        let folders = snap.folder_sounds.iter().filter(|s| s.is_some()).count();
        info!(
            "bullet snap: loaded {BULLET_SNAP_INI_PATH} ({found_files} audio file(s) in {folders}/{} folders; distance_based={}, subsonic={}, subsonic_speed={:.0}, angle={:.0}°, impact_player={})",
            SoundFolder::ALL.len(),
            config.distanced_based_snap,
            config.subsonic,
            config.subsonic_threshold_speed,
            config.snap_angle_deg,
            config.impacted_player_snap,
        );
        snap
    }

    /// Whether `BulletSnap.ini` was found in the VFS.
    pub fn loaded(&self) -> bool {
        self.config.is_some()
    }

    /// Whether BulletSnap is actively handling pass-by audio (INI loaded AND at least one
    /// snap sound folder contains audio files).
    pub fn active(&self) -> bool {
        self.config.is_some() && self.folder_sounds.iter().any(Option::is_some)
    }

    pub fn config(&self) -> Option<&BulletSnapConfig> {
        self.config.as_ref()
    }

    /// Synthetic SOUN FormID registered for `folder`, if that folder has audio files.
    pub fn sound_id(&self, folder: SoundFolder) -> Option<FormId> {
        self.folder_sounds[folder.index()]
    }

    /// Queue a projectile segment or player impact for evaluation this frame.
    pub fn push(&mut self, input: SnapInput) {
        if self.active() {
            self.inputs.push(input);
        }
    }

    /// Drop tracked state for projectiles no longer in flight.
    ///
    /// Any finished ID still referenced by queued `inputs` is retained through the
    /// current frame's [`tick`] (preventing double-snapping on a projectile's final
    /// segment or player impact) and pruned immediately once [`tick`] drains `inputs`.
    pub fn retain_active(&mut self, active: impl Fn(u64) -> bool) {
        if self.active() {
            let inputs = &self.inputs;
            for input in inputs {
                let id = input.id();
                if !active(id) && !self.prune_after_tick.contains(&id) {
                    self.prune_after_tick.push(id);
                }
            }
            self.tracker.retain_active(|id| active(id) || inputs.iter().any(|i| i.id() == id));
        }
    }

    /// Clear tracked projectiles (e.g. on cell/worldspace travel).
    pub fn clear_tracking(&mut self) {
        self.tracker.clear();
        self.inputs.clear();
        self.prune_after_tick.clear();
    }

    fn ensure_registered(&mut self, sfx: &mut Sfx) {
        if self.registered_in_sfx {
            return;
        }
        let Some(cfg) = self.config else { return };
        let min_distance = cfg.very_close.max(128.0);
        let max_distance = cfg.max_reach();
        let fallback_start = BULLET_SNAP_SOUND_BASE + SoundFolder::ALL.len() as u32;
        for folder in SoundFolder::ALL {
            let idx = folder.index();
            let Some(preferred) = self.folder_sounds[idx] else { continue };
            self.folder_sounds[idx] = sfx.register_folder_sound(
                preferred,
                fallback_start,
                BULLET_SNAP_SOUND_END,
                folder.label(),
                folder.sfx_fnam(),
                min_distance,
                max_distance,
                Some(SNAP_ATTENUATION_CURVE),
                self.folder_files[idx].clone(),
            );
        }
        self.registered_in_sfx = true;
    }
}

fn init(mut commands: Commands, data: Option<Res<nvf_assets::GameData>>) {
    let snap = data.as_deref().map_or_else(BulletSnap::inactive, |d| BulletSnap::from_vfs(&d.vfs));
    commands.insert_resource(snap);
}

pub(crate) fn tick(
    mut snap: ResMut<BulletSnap>,
    player: Option<Res<Player>>,
    sfx: Option<ResMut<Sfx>>,
    mut scaled: MessageWriter<ScaledSound>,
) {
    let snap = &mut *snap;
    if !snap.active() {
        snap.inputs.clear();
        snap.prune_after_tick.clear();
        return;
    }
    if let Some(mut sfx) = sfx {
        snap.ensure_registered(&mut sfx);
    }
    let Some(player) = player else {
        snap.inputs.clear();
        let prune = std::mem::take(&mut snap.prune_after_tick);
        if !prune.is_empty() {
            snap.tracker.retain_active(|id| !prune.contains(&id));
        }
        return;
    };
    let Some(cfg) = snap.config else {
        snap.inputs.clear();
        snap.prune_after_tick.clear();
        return;
    };
    let eye = player.eye();
    let inputs = std::mem::take(&mut snap.inputs);
    for input in inputs {
        let trigger = match input {
            SnapInput::Segment { id, from, to, dir, speed, damage, is_player_owned, eligible } => {
                snap.tracker.observe_segment(id, from, to, dir, speed, damage, is_player_owned, eligible, eye, &cfg)
            }
            SnapInput::PlayerImpact { id, at, speed, damage, is_player_owned, eligible } => {
                snap.tracker.observe_player_impact(id, at, speed, damage, is_player_owned, eligible, eye, &cfg)
            }
        };
        let Some(trig) = trigger else { continue };
        let Some(folder) = pick_folder(trig.kind, trig.band, |f| snap.sound_id(f).is_some()) else {
            continue;
        };
        let Some(sound) = snap.sound_id(folder) else { continue };
        snap.stats.triggered += 1;
        match trig.kind {
            SnapKind::Subsonic => snap.stats.subsonic += 1,
            SnapKind::General | SnapKind::HighCaliber | SnapKind::VeryHighCaliber => snap.stats.supersonic += 1,
        }
        if trig.on_player_impact {
            snap.stats.player_impacts += 1;
        }
        match trig.band {
            SnapBand::VeryClose => snap.stats.very_close += 1,
            SnapBand::Close => snap.stats.close += 1,
            SnapBand::Distance => snap.stats.distance += 1,
            SnapBand::Far => snap.stats.far += 1,
        }
        debug!(
            "bullet snap: projectile {} {:?} {:?} at {:.0} units (gain {:.2}, folder {}, impact={})",
            trig.id,
            trig.kind,
            trig.band,
            trig.distance,
            trig.gain,
            folder.label(),
            trig.on_player_impact,
        );
        scaled.write(ScaledSound { sound, at: Some(trig.at), gain: trig.gain });
    }
    if !snap.prune_after_tick.is_empty() {
        let prune = std::mem::take(&mut snap.prune_after_tick);
        snap.tracker.retain_active(|id| !prune.contains(&id));
    }
}

fn report(mut exit: MessageReader<AppExit>, snap: Option<Res<BulletSnap>>) {
    if let (Some(_), Some(s)) = (exit.read().last(), snap) {
        if s.loaded() {
            let st = s.stats;
            info!(
                "bullet snap summary: {} snap(s) ({} supersonic, {} subsonic, {} on player impact; bands very_close={}, close={}, distance={}, far={})",
                st.triggered, st.supersonic, st.subsonic, st.player_impacts, st.very_close, st.close, st.distance, st.far,
            );
        }
    }
}

pub struct BulletSnapPlugin;

impl Plugin for BulletSnapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BulletSnap>()
            .add_systems(Startup, init)
            .add_systems(Update, tick.after(PlaySet::Activation).before(SfxSet))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_combat::fx::FxForms;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_data_dir(tag: &str) -> PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("nvf-bullet-snap-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_wav(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Minimal valid 16-bit mono PCM WAV header + 2 samples.
        let mut v = b"RIFF".to_vec();
        v.extend(40u32.to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes()); // PCM
        v.extend(1u16.to_le_bytes()); // mono
        v.extend(22_050u32.to_le_bytes());
        v.extend(44_100u32.to_le_bytes());
        v.extend(2u16.to_le_bytes());
        v.extend(16u16.to_le_bytes());
        v.extend(b"data");
        v.extend(4u32.to_le_bytes());
        v.extend([0u8, 0, 0x40, 0]);
        std::fs::write(path, v).unwrap();
    }

    fn populate_all_folders(data: &std::path::Path) {
        for folder in SoundFolder::ALL {
            let rel = folder.vfs_folder().replace('\\', "/");
            write_wav(&data.join(rel).join("snap_01.wav"));
        }
    }

    fn test_player() -> Player {
        Player { heading: 0.0, pitch: 0.0, eye_height: 64.0, feet: [0.0, 0.0, 0.0], body_eye: None, body_view: None, placed: None }
    }

    #[test]
    fn absent_ini_is_complete_noop() {
        let dir = temp_data_dir("absent");
        populate_all_folders(&dir);
        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let snap = BulletSnap::from_vfs(&vfs);
        assert!(!snap.loaded());
        assert!(!snap.active());

        let mut app = App::new();
        app.insert_resource(snap)
            .insert_resource(test_player())
            .insert_resource(Sfx::new(FxForms::default()))
            .add_message::<ScaledSound>()
            .add_systems(Update, tick);

        app.world_mut().resource_mut::<BulletSnap>().push(SnapInput::Segment {
            id: 1,
            from: [-500.0, 200.0, 0.0],
            to: [500.0, 200.0, 0.0],
            dir: [1.0, 0.0, 0.0],
            speed: 25_000.0,
            damage: 50.0,
            is_player_owned: false,
            eligible: true,
        });
        app.update();

        let emitted: Vec<ScaledSound> = app.world_mut().resource_mut::<Messages<ScaledSound>>().drain().collect();
        assert!(emitted.is_empty());
        assert!(app.world().resource::<Sfx>().forms.sounds.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_ini_registers_folders_and_emits_band_and_tier_sounds() {
        let dir = temp_data_dir("active");
        let ini_path = dir.join("NVSE").join("Plugins").join("BulletSnapConfig").join("BulletSnap.ini");
        std::fs::create_dir_all(ini_path.parent().unwrap()).unwrap();
        std::fs::write(
            &ini_path,
            "[General]\r\nImpactedPlayerSnap = 0\r\nDistancedBasedSnap = 1\r\nSubsonic = 1\r\nSubsonicThresholdSpeed = 21000\r\nSnapAngle = 75\r\n\
             [DistanceBased]\r\nVeryClose = 768\r\nClose = 1536\r\nDistance = 4092\r\nFar = 8192\r\n\
             [DamageBased]\r\nmiddledmg = 32\r\nhighdmg = 46\r\n",
        )
        .unwrap();
        populate_all_folders(&dir);

        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let snap = BulletSnap::from_vfs(&vfs);
        assert!(snap.loaded());
        assert!(snap.active());

        let player = test_player();
        let eye_z = player.eye()[2];

        let mut app = App::new();
        app.insert_resource(snap)
            .insert_resource(player)
            .insert_resource(Sfx::new(FxForms::default()))
            .add_message::<ScaledSound>()
            .add_systems(Update, tick);

        {
            let mut b = app.world_mut().resource_mut::<BulletSnap>();
            // 1. VeryClose HighCaliber supersonic pass-by (y = 300 <= 768, speed 25000 >= 21000, dmg 35 in [32,46)).
            b.push(SnapInput::Segment {
                id: 1,
                from: [-500.0, 300.0, eye_z],
                to: [500.0, 300.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 25_000.0,
                damage: 35.0,
                is_player_owned: false,
                eligible: true,
            });
            // 2. Far VeryHighCaliber supersonic pass-by (y = 5000 in (4092, 8192], dmg 60 >= 46).
            b.push(SnapInput::Segment {
                id: 2,
                from: [-500.0, 5000.0, eye_z],
                to: [500.0, 5000.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 30_000.0,
                damage: 60.0,
                is_player_owned: false,
                eligible: true,
            });
            // 3. Distance Subsonic pass-by (y = 2500 in (1536, 4092], speed 15000 < 21000).
            b.push(SnapInput::Segment {
                id: 3,
                from: [-500.0, 2500.0, eye_z],
                to: [500.0, 2500.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 15_000.0,
                damage: 100.0,
                is_player_owned: false,
                eligible: true,
            });
            // 4. Beyond Far (y = 9000 > 8192) -> silent.
            b.push(SnapInput::Segment {
                id: 4,
                from: [-500.0, 9000.0, eye_z],
                to: [500.0, 9000.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 25_000.0,
                damage: 50.0,
                is_player_owned: false,
                eligible: true,
            });
            // 5. Player impact with ImpactedPlayerSnap = 0 -> silent.
            b.push(SnapInput::PlayerImpact {
                id: 5,
                at: [0.0, 0.0, eye_z],
                speed: 25_000.0,
                damage: 50.0,
                is_player_owned: false,
                eligible: true,
            });
        }

        app.update();

        let emitted: Vec<ScaledSound> = app.world_mut().resource_mut::<Messages<ScaledSound>>().drain().collect();
        assert_eq!(emitted.len(), 3);
        let b = app.world().resource::<BulletSnap>();
        assert_eq!(emitted[0].sound, b.sound_id(SoundFolder::HighCaliberClose).unwrap());
        assert_eq!(emitted[0].at, Some([0.0, 300.0, eye_z]));
        assert_eq!(emitted[0].gain, 1.0);

        assert_eq!(emitted[1].sound, b.sound_id(SoundFolder::VeryHighCaliberFar).unwrap());
        assert_eq!(emitted[1].at, Some([0.0, 5000.0, eye_z]));
        assert_eq!(emitted[1].gain, 0.45);

        assert_eq!(emitted[2].sound, b.sound_id(SoundFolder::SubsonicMid).unwrap());
        assert_eq!(emitted[2].at, Some([0.0, 2500.0, eye_z]));
        assert_eq!(emitted[2].gain, 0.65);

        assert_eq!(b.stats.triggered, 3);
        assert_eq!(b.stats.supersonic, 2);
        assert_eq!(b.stats.subsonic, 1);
        assert_eq!(b.stats.very_close, 1);
        assert_eq!(b.stats.distance, 1);
        assert_eq!(b.stats.far, 1);

        // Synthetic SOUN records were registered in Sfx for all 10 folders.
        let sfx = app.world().resource::<Sfx>();
        assert_eq!(sfx.forms.sounds.len(), SoundFolder::ALL.len());
        let far_soun = sfx.forms.sounds.get(&b.sound_id(SoundFolder::VeryHighCaliberFar).unwrap()).unwrap();
        assert!(far_soun.max_distance > 8_192.0, "Far snap must stay within Sfx attenuation reach");

        // Second segment for id=1 must not fire again.
        app.world_mut().resource_mut::<BulletSnap>().push(SnapInput::Segment {
            id: 1,
            from: [500.0, 300.0, eye_z],
            to: [1000.0, 300.0, eye_z],
            dir: [1.0, 0.0, 0.0],
            speed: 25_000.0,
            damage: 35.0,
            is_player_owned: false,
            eligible: true,
        });
        app.update();
        let again: Vec<ScaledSound> = app.world_mut().resource_mut::<Messages<ScaledSound>>().drain().collect();
        assert!(again.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retain_active_before_tick_does_not_double_snap_and_prunes_finished_ids() {
        let dir = temp_data_dir("retain-before-tick");
        let ini_path = dir.join("NVSE").join("Plugins").join("BulletSnapConfig").join("BulletSnap.ini");
        std::fs::create_dir_all(ini_path.parent().unwrap()).unwrap();
        std::fs::write(
            &ini_path,
            "[General]\r\nImpactedPlayerSnap = 1\r\nDistancedBasedSnap = 1\r\nSubsonic = 1\r\nSubsonicThresholdSpeed = 21000\r\nSnapAngle = 75\r\n\
             [DistanceBased]\r\nVeryClose = 768\r\nClose = 1536\r\nDistance = 4092\r\nFar = 8192\r\n\
             [DamageBased]\r\nmiddledmg = 32\r\nhighdmg = 46\r\n",
        )
        .unwrap();
        populate_all_folders(&dir);

        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let snap = BulletSnap::from_vfs(&vfs);
        assert!(snap.active());

        let player = test_player();
        let eye_z = player.eye()[2];

        let mut app = App::new();
        app.insert_resource(snap)
            .insert_resource(player)
            .insert_resource(Sfx::new(FxForms::default()))
            .add_message::<ScaledSound>()
            .add_systems(Update, tick);

        // Frame 1: both projectiles snap while angle to eye is in (75°, 90°)
        // (at to = (-20, 200, eye_z), angle = atan2(200, 20) ≈ 84.3° > 75°).
        {
            let mut b = app.world_mut().resource_mut::<BulletSnap>();
            b.push(SnapInput::Segment {
                id: 10,
                from: [-200.0, 200.0, eye_z],
                to: [-20.0, 200.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 25_000.0,
                damage: 40.0,
                is_player_owned: false,
                eligible: true,
            });
            b.push(SnapInput::Segment {
                id: 11,
                from: [-200.0, 200.0, eye_z],
                to: [-20.0, 200.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 25_000.0,
                damage: 50.0,
                is_player_owned: false,
                eligible: true,
            });
        }
        app.update();
        let frame1: Vec<ScaledSound> = app.world_mut().resource_mut::<Messages<ScaledSound>>().drain().collect();
        assert_eq!(frame1.len(), 2);
        assert_eq!(app.world().resource::<BulletSnap>().tracker.tracked_len(), 2);

        // Frame 2 (exact `combat::fly` order):
        // - id=10 finishes on final segment starting still before 90° (`from.x = -20`, so
        //   erasing `snapped` before `tick` would falsely fire a second pass-by snap).
        // - id=11 finishes with `PlayerImpact` while `ImpactedPlayerSnap = 1` (erasing
        //   `snapped` before `tick` would falsely fire a second snap on impact).
        // Both finish this frame, so `combat::fly` calls `retain_active(|_| false)` before `tick`.
        {
            let mut b = app.world_mut().resource_mut::<BulletSnap>();
            b.push(SnapInput::Segment {
                id: 10,
                from: [-20.0, 200.0, eye_z],
                to: [200.0, 200.0, eye_z],
                dir: [1.0, 0.0, 0.0],
                speed: 25_000.0,
                damage: 40.0,
                is_player_owned: false,
                eligible: true,
            });
            b.push(SnapInput::PlayerImpact {
                id: 11,
                at: [0.0, 0.0, eye_z],
                speed: 25_000.0,
                damage: 50.0,
                is_player_owned: false,
                eligible: true,
            });
            b.retain_active(|_| false);
            assert_eq!(b.tracker.tracked_len(), 2, "pending inputs must keep tracked state alive until tick runs");
        }
        app.update();
        let frame2: Vec<ScaledSound> = app.world_mut().resource_mut::<Messages<ScaledSound>>().drain().collect();
        assert!(frame2.is_empty(), "already-snapped projectiles must not double-snap on their final frame: {frame2:?}");
        assert_eq!(app.world().resource::<BulletSnap>().stats.triggered, 2);

        // Finished IDs are pruned cleanly after tick (and remain empty on a follow-up retain_active).
        assert_eq!(app.world().resource::<BulletSnap>().tracker.tracked_len(), 0);
        app.world_mut().resource_mut::<BulletSnap>().retain_active(|_| false);
        assert_eq!(app.world().resource::<BulletSnap>().tracker.tracked_len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_sound_ids_avoid_dynamic_and_callable_ranges_and_loaded_records_win() {
        let dir = temp_data_dir("formid-guard");
        let ini_path = dir.join("NVSE").join("Plugins").join("BulletSnapConfig").join("BulletSnap.ini");
        std::fs::create_dir_all(ini_path.parent().unwrap()).unwrap();
        std::fs::write(&ini_path, "[General]\nDistancedBasedSnap = 1\nSnapAngle = 75\n").unwrap();
        populate_all_folders(&dir);

        let vfs = fnv_vfs::Vfs::open(&dir, &[]).unwrap();
        let snap = BulletSnap::from_vfs(&vfs);
        assert!(snap.active());

        // Every preferred ID sits strictly inside [BULLET_SNAP_SOUND_BASE, BULLET_SNAP_SOUND_END),
        // completely outside 0x00..=0xFD plugin slots, CLONE_BASE, COMPILED_BASE, low TEMP_REF_BASE,
        // CAMERA_OVERLAY_BASE (0xFE7F_0000), LAMBDA_BASE (0xFE80_0000), and FIRST_DYNAMIC (0xFF00_0000).
        const {
            assert!(BULLET_SNAP_SOUND_BASE > fnv_game::game::TEMP_REF_BASE);
            assert!(BULLET_SNAP_SOUND_END <= 0xFE7F_0000);
            assert!(BULLET_SNAP_SOUND_END < fnv_game::game::LAMBDA_BASE);
            assert!(BULLET_SNAP_SOUND_END < FormId::FIRST_DYNAMIC.0);
        }
        let first_dynamic = FormId::FIRST_DYNAMIC.0;
        let clone_base = fnv_game::nvse_data::CLONE_BASE;
        let compiled_base = fnv_game::game::COMPILED_BASE;
        let temp_ref_base = fnv_game::game::TEMP_REF_BASE;
        let lambda_base = fnv_game::game::LAMBDA_BASE;
        for folder in SoundFolder::ALL {
            let id = snap.sound_id(folder).unwrap().0;
            assert!((BULLET_SNAP_SOUND_BASE..BULLET_SNAP_SOUND_END).contains(&id));
            assert_eq!(id >> 24, 0xFE, "must not collide with normal plugin indices 0x00..=0xFD");
            assert!(!(clone_base..compiled_base).contains(&id));
            assert!(!(compiled_base..temp_ref_base).contains(&id));
            assert!(id < lambda_base);
            assert!(id < first_dynamic);
        }

        // Pre-populate a loaded SOUN record at HighCaliberClose's preferred FormID:
        // the loaded record must win untouched and BulletSnap must move to a free fallback ID.
        let colliding_id = snap.sound_id(SoundFolder::HighCaliberClose).unwrap();
        let mut fx = FxForms::default();
        fx.sounds.insert(
            colliding_id,
            fnv_combat::fx::Sound {
                edid: Some("LoadedModSound".into()),
                file: Some("fx\\loaded\\keep.wav".into()),
                min_distance: 50.0,
                max_distance: 500.0,
                ..Default::default()
            },
        );

        let player = test_player();
        let eye_z = player.eye()[2];
        let mut app = App::new();
        app.insert_resource(snap)
            .insert_resource(player)
            .insert_resource(Sfx::new(fx))
            .add_message::<ScaledSound>()
            .add_systems(Update, tick);

        app.world_mut().resource_mut::<BulletSnap>().push(SnapInput::Segment {
            id: 42,
            from: [-500.0, 300.0, eye_z],
            to: [500.0, 300.0, eye_z],
            dir: [1.0, 0.0, 0.0],
            speed: 25_000.0,
            damage: 38.0, // HighCaliber tier
            is_player_owned: false,
            eligible: true,
        });
        app.update();

        let b = app.world().resource::<BulletSnap>();
        let assigned = b.sound_id(SoundFolder::HighCaliberClose).unwrap();
        assert_ne!(assigned, colliding_id, "must skip colliding loaded SOUN FormID");
        assert!(
            (BULLET_SNAP_SOUND_BASE..BULLET_SNAP_SOUND_END).contains(&assigned.0),
            "fallback ID {assigned:?} must stay within reserved BulletSnap range"
        );
        assert!(assigned.0 < first_dynamic && assigned.0 < lambda_base);

        let sfx = app.world().resource::<Sfx>();
        let loaded = sfx.forms.sounds.get(&colliding_id).unwrap();
        assert_eq!(loaded.edid.as_deref(), Some("LoadedModSound"), "loaded SOUN record must win");
        assert_eq!(loaded.file.as_deref(), Some("fx\\loaded\\keep.wav"));
        assert_eq!(loaded.max_distance, 500.0);

        let snap_soun = sfx.forms.sounds.get(&assigned).unwrap();
        assert_eq!(snap_soun.edid.as_deref(), Some(SoundFolder::HighCaliberClose.label()));

        let emitted: Vec<ScaledSound> = app.world_mut().resource_mut::<Messages<ScaledSound>>().drain().collect();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].sound, assigned);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
