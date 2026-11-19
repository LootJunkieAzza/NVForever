//! Opt-in physical-consumer smoke aid; never used by an ordinary run.
//! See projectile-launch-session-traits.md. Setup uses real compiled script calls;
//! acceptance observes only actual Flying insertions, never setter readback.
use super::*;
use crate::save::SaveConfig;
use crate::PlayState;
use fnv_game::{Game, game::RunCtx, nvse_jip7::ProjectileLaunchFieldLease};
use fnv_script::vm::XVal;
use std::path::{Path, PathBuf};

const HOME: &str = "b42-launch-probe-home";
const MAX_WAIT_FRAMES: u32 = 7200;
const MAX_ACTIVE_FRAMES: u32 = 300;
const WAIT_SECONDS: f64 = 120.0;
const ACTIVE_SECONDS: f64 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage { Waiting, FirstPending, Restore, SecondPending, Cleanup, Done }

#[derive(Debug)]
struct Backup {
    lease: ProjectileLaunchFieldLease,
    weapon: FormId,
    ammo: Option<FormId>,
    projectile: FormId,
    weapon_before: i32,
    ammo_before: i32,
    original: [f64; 2],
    restored_physical: [f32; 2],
    sentinel: [f32; 2],
    fields_restored: bool,
}

#[derive(Resource, Debug)]
pub(super) struct Probe {
    edid: String,
    home: PathBuf,
    stage: Stage,
    backup: Option<Backup>,
    frames: u32,
    armed_frame: u32,
    started: Option<f64>,
    armed_at: f64,
    requests: u32,
    accepted: u32,
    last_visual: Option<u64>,
    failed: Option<String>,
    cleaned: bool,
    passed: bool,
    cleanup_attempts: u8,
}

fn query(game: &mut Game<'_>, body: &str) -> Result<XVal, String> {
    game.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default())
        .map_err(|why| format!("{why}; generated source: {body:?}"))
}
/// Bare vanilla statements and extended expressions parse form arguments differently.
/// StringToRef resolves the exact loaded ID and rejects missing forms.
fn source_form(id: FormId) -> String {
    format!("(StringToRef \"{:08X}\")", id.0)
}
fn count(world: &fnv_sim::World<'_>, form: FormId) -> i32 {
    world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map_or(0, |i| i.count(form))
}
fn form_result(value: XVal) -> Result<Option<FormId>, String> {
    match value {
        XVal::Ref(0) | XVal::Num(0.0) => Ok(None),
        XVal::Ref(id) => Ok(Some(FormId(id))),
        _ => Err("projectile getter did not return an actual form".into()),
    }
}
fn empty_saves(home: &Path) -> Result<(), String> {
    match std::fs::read_dir(home.join("saves")) {
        Ok(mut entries) => if entries.next().is_none() { Ok(()) } else { Err("probe home has preexisting saves or other save-directory files".into()) },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("probe save directory unreadable: {e}")),
    }
}

impl Probe {
    fn configured(edid: String, home: PathBuf, cwd: &Path) -> Result<Self, String> {
        if edid.is_empty() || edid.len() > 128 || !edid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("expected one bounded loaded weapon EditorID".into());
        }
        let cwd = cwd.canonicalize().map_err(|e| e.to_string())?;
        let home = home.canonicalize().map_err(|e| e.to_string())?;
        if home != cwd.join(HOME) { return Err("probe requires its isolated smoke-output home".into()); }
        empty_saves(&home)?;
        Ok(Self { edid, home, stage: Stage::Waiting, backup: None, frames: 0, armed_frame: 0, started: None, armed_at: 0.0,
            requests: 0, accepted: 0, last_visual: None, failed: None, cleaned: false, passed: false, cleanup_attempts: 0 })
    }
    fn fail(&mut self, why: impl Into<String>) {
        if self.failed.is_none() {
            let why = why.into();
            error!("projectile launch probe: FAIL {why}; stage={:?} requests={} accepted={}", self.stage, self.requests, self.accepted);
            self.failed = Some(why);
        }
    }
    fn prepare(&mut self, game: &mut Game<'_>, combat: &Combat, now: f64, cfg: &SaveConfig) -> Result<(), String> {
        let dir = cfg.dir.as_ref().ok_or("probe needs an explicit save directory")?;
        let parent = dir.parent().ok_or("probe save path has no home")?.canonicalize().map_err(|e| e.to_string())?;
        if parent != self.home || dir.file_name().is_none_or(|n| n != "saves") { return Err("actual SaveConfig is not the isolated probe home".into()); }
        empty_saves(&self.home)?;
        if !game.combat_requests.is_empty() || !combat.script_shots.is_empty() { return Err("unowned combat requests interfere with probe setup".into()); }
        let mut matches = combat.forms.weapons.iter().filter(|(_, w)| w.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&self.edid)));
        let (&weapon, _) = matches.next().ok_or("loaded weapon EditorID is absent")?;
        if matches.next().is_some() { return Err("loaded weapon EditorID is ambiguous".into()); }
        if game.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).is_some_and(|i| i.mods_in_use(weapon) != 0) {
            return Err("fixture requires an unmodified weapon unit".into());
        }
        let ammo = combat.forms.ammo_for(weapon, |a| count(&game.world, a) > 0);
        let chain = combat.forms.chain(weapon, ammo).ok_or("typed weapon has no shot chain")?;
        if chain.projectiles_per_shot != 1 { return Err("fixture requires one projectile per shot".into()); }
        let ammo_projectile = match ammo {
            Some(a) => form_result(query(game, &format!("SetFunctionValue (GetAmmoProjectile {})", source_form(a)))?)?,
            None => None,
        };
        let projectile = match ammo_projectile {
            Some(p) => p,
            None => form_result(query(game, &format!("SetFunctionValue (GetWeaponProjectile {})", source_form(weapon)))?)?.ok_or("no runtime projectile is selected")?,
        };
        let typed = combat.forms.projectiles.get(&projectile).ok_or("selected projectile is not a static typed PROJ")?;
        if !matches!(typed.kind.as_deref(), Some("Missile" | "Lobber")) || !typed.speed.is_finite() || typed.speed <= 0.0 || !typed.impact_force.is_finite() {
            return Err("fixture requires a finite typed nonbeam projectile".into());
        }
        let lease = game.preserve_projectile_launch_fields(projectile).ok_or("selected form cannot be leased")?;
        let original = lease.effective_values();
        // A nonnumeric prior entry reads zero in num_field, but launch ignores it
        // and retains CombatForms. Resolve that policy independently of resolve()
        // and refuse any prior getter representation this bounded aid cannot name.
        let traits = game.projectile_launch_traits(projectile).ok_or("original launch traits unavailable")?;
        if traits.original != projectile { return Err("temporary projectile aliases are outside this aid".into()); }
        let restored_physical = [traits.speed.unwrap_or(typed.speed), traits.impact_force.unwrap_or(typed.impact_force)];
        if original.iter().any(|n| !n.is_finite() || !(*n as f32).is_finite()) || restored_physical.iter().any(|n| !n.is_finite()) || restored_physical[0] <= 0.0
            || original.map(|n| (n as f32).to_bits()) != restored_physical.map(f32::to_bits) {
            return Err("original field representation is unsupported by this physical restoration aid".into());
        }
        let choose = |values: [f32; 3], typed: f32, effective: f64| values.into_iter().find(|v| *v != typed && f64::from(*v) != effective).expect("three sentinels differ from two originals");
        let sentinel = [choose([500.0, 750.0, 1000.5], typed.speed, original[0]), choose([7.0, 9.0, 13.0], typed.impact_force, original[1])];
        let weapon_before = count(&game.world, weapon);
        let ammo_before = ammo.map_or(0, |a| count(&game.world, a));
        if !(0..=i32::MAX - 2).contains(&weapon_before) || !(0..=i32::MAX - 2).contains(&ammo_before) { return Err("inventory cannot stage two bounded fixture items".into()); }
        self.backup = Some(Backup { lease, weapon, ammo, projectile, weapon_before, ammo_before, original, restored_physical, sentinel, fields_restored: false });
        self.armed_frame = self.frames; self.armed_at = now;
        let mut source = format!("Player.AddItem {} 2 1\n", source_form(weapon));
        if let Some(a) = ammo { source.push_str(&format!("Player.AddItem {} 2 1\n", source_form(a))); }
        source.push_str(&format!("SetProjectileTraitNumeric {} 2 {}\nSetProjectileTraitNumeric {} 9 {}\nPlayer.FireWeapon {}\nPlayer.RemoveItem {} 1 1", source_form(projectile), sentinel[0], source_form(projectile), sentinel[1], source_form(weapon), source_form(weapon)));
        query(game, &source)?;
        self.stage = Stage::FirstPending;
        self.one_queued_request(game)?;
        info!("projectile launch probe: armed loaded={} weapon={weapon} ammo={ammo:?} projectile={projectile} sentinel={sentinel:?} original={original:?}", self.edid);
        Ok(())
    }
    fn one_queued_request(&self, game: &Game<'_>) -> Result<(), String> {
        let b = self.backup.as_ref().ok_or("probe has no field/inventory backup")?;
        if !matches!(game.combat_requests.as_slice(), [fnv_game::combat::CombatRequest::Fire { from, weapon, muzzle: None }] if *from == FormId::PLAYER_REF && *weapon == b.weapon) {
            return Err("compiled source did not queue exactly its one real FireWeapon request".into());
        }
        Ok(())
    }
    fn restored_request(&mut self, game: &mut Game<'_>, combat: &Combat) -> Result<(), String> {
        if !game.combat_requests.is_empty() || !combat.script_shots.is_empty() { return Err("unowned requests interfere with restored stage".into()); }
        let b = self.backup.as_mut().ok_or("missing restore capsule")?;
        if !b.lease.belongs_to(game) { return Err("runtime owner changed before restored stage".into()); }
        b.lease.restore(game)?;
        b.fields_restored = true;
        query(game, &format!("Player.FireWeapon {}\nPlayer.RemoveItem {} 1 1", source_form(b.weapon), source_form(b.weapon)))?;
        self.stage = Stage::SecondPending; self.one_queued_request(game)
    }
    pub(super) fn request(&mut self, shot: &ActorFire, scripted: bool) {
        let Some(b) = self.backup.as_ref() else { return };
        if shot.shooter != FormId::PLAYER_REF || shot.weapon != b.weapon { return; }
        if !scripted || !matches!(self.stage, Stage::FirstPending | Stage::SecondPending) || self.requests != self.accepted {
            self.fail("unmatched/extra producer request"); return;
        }
        self.requests += 1;
    }
    pub(super) fn inserted(&mut self, shot: &ActorFire, scripted: bool, flying: &Flying, world: &fnv_sim::World<'_>) {
        let Some(b) = self.backup.as_ref() else { return };
        if shot.shooter != FormId::PLAYER_REF || shot.weapon != b.weapon { return; }
        let expected = match self.stage { Stage::FirstPending => b.sentinel, Stage::SecondPending => b.restored_physical, _ => { self.fail("extra physical insertion"); return; } };
        let expected_weapon_count = b.weapon_before + if self.stage == Stage::FirstPending { 1 } else { 0 };
        let speed = vec::len(flying.p.vel);
        let tolerance = (expected[0].abs() * 1.0e-5).max(0.001);
        let valid = scripted && self.requests == self.accepted + 1 && self.accepted < 2 && flying.p.vel.iter().all(|v| v.is_finite())
            && (speed - expected[0]).abs() <= tolerance && (flying.p.speed - expected[0]).abs() <= tolerance
            && flying.force.to_bits() == expected[1].to_bits() && flying.proj == b.projectile && flying.by.weapon == b.weapon
            && flying.by.ammo == b.ammo && flying.by.actor == FormId::PLAYER_REF && flying.p.shooter == Some(PLAYER)
            && self.last_visual != Some(flying.id) && count(world, b.weapon) == expected_weapon_count
            && b.ammo.is_none_or(|a| count(world, a) == b.ammo_before + 2);
        info!("projectile launch probe: physical stage={:?} visual={} velocity={:?} speed={} force={} projectile={} weapon={} ammo={:?} weapon_count={} ammo_count={:?}", self.stage, flying.id, flying.p.vel, speed, flying.force, flying.proj, flying.by.weapon, flying.by.ammo, count(world, b.weapon), b.ammo.map(|a| count(world, a)));
        if !valid { self.fail("actual physical insertion/identity/inventory does not match the independent stage oracle"); return; }
        self.last_visual = Some(flying.id); self.accepted += 1;
        self.stage = if self.accepted == 1 { Stage::Restore } else { Stage::Cleanup };
    }
    fn cleanup(&mut self, game: &mut Game<'_>) -> Result<(), String> {
        let Some(b) = self.backup.as_mut() else { self.cleaned = true; return Ok(()) };
        if !b.lease.belongs_to(game) { return Err("refusing stale cleanup against another Game".into()); }
        if !b.fields_restored { b.lease.restore(game)?; b.fields_restored = true; }
        for (form, wanted) in [(Some(b.weapon), b.weapon_before), (b.ammo, b.ammo_before)] {
            let Some(form) = form else { continue };
            let delta = i64::from(wanted) - i64::from(count(&game.world, form));
            if delta != 0 {
                let quantity = i32::try_from(delta.abs()).map_err(|_| "cleanup count exceeds bounded inventory API")?;
                query(game, &format!("Player.{} {} {quantity} 1", if delta > 0 { "AddItem" } else { "RemoveItem" }, source_form(form)))?;
            }
            if count(&game.world, form) != wanted { return Err("inventory cleanup did not restore the actual count".into()); }
        }
        let actual = [game.num_field(b.projectile, "DATA", "speed"), game.num_field(b.projectile, "DATA", "impact_force")];
        if actual.map(f64::to_bits) != b.original.map(f64::to_bits) { return Err("effective values differ after exact entry cleanup".into()); }
        self.backup = None; self.cleaned = true;
        Ok(())
    }
}

pub(super) fn install(app: &mut App) {
    let Some(edid) = std::env::var_os("NVF_PLAY_TEST_PROJECTILE_LAUNCH") else { return };
    let configured = (|| {
        let home = std::env::var_os("NVFOREVER_HOME").ok_or("explicit isolated NVFOREVER_HOME required")?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Probe::configured(edid.to_string_lossy().into_owned(), PathBuf::from(home), &cwd)
    })();
    match configured {
        Ok(probe) => {
            app.insert_resource(probe)
                .add_systems(Update, drive.before(actor_attacks).in_set(PlaySet::Activation).run_if(in_state(PlayState::Playing)))
                .add_systems(Update, teardown.before(leave_space_shots))
                .add_systems(OnExit(PlayState::Playing), cancel)
                .add_systems(Last, finish.before(report));
        }
        Err(why) => error!("projectile launch probe: FAIL configuration: {why}"),
    }
}

fn drive(mut probe: ResMut<Probe>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, combat: Res<Combat>, cfg: Res<SaveConfig>, time: Res<Time<Real>>) {
    if probe.stage == Stage::Done || probe.failed.is_some() { return; }
    probe.frames = probe.frames.saturating_add(1);
    let now = time.elapsed_secs_f64(); let started = *probe.started.get_or_insert(now);
    if !now.is_finite() || probe.frames > MAX_WAIT_FRAMES || now - started > WAIT_SECONDS || probe.backup.is_some() && (probe.frames - probe.armed_frame > MAX_ACTIVE_FRAMES || now - probe.armed_at > ACTIVE_SECONDS) {
        probe.fail("bounded wait/stage deadline"); return;
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let result = with_world(rt, &mut sim, |rt| match probe.stage {
        Stage::Waiting if now - started >= 20.0 => probe.prepare(&mut rt.game, &combat, now, &cfg),
        Stage::Restore => probe.restored_request(&mut rt.game, &combat),
        Stage::FirstPending | Stage::SecondPending => {
            let b = probe.backup.as_ref().ok_or("pending diagnostic has no backup")?;
            if !matches!(combat.script_shots.as_slice(), [shot] if shot.shooter == FormId::PLAYER_REF && shot.weapon == b.weapon) { Err("real script shot scheduling did not preserve its one request".into()) } else { Ok(()) }
        },
        _ => Ok(()),
    });
    if let Err(why) = result { probe.fail(why); }
}
fn cleanup(probe: &mut Probe, script: Option<&mut ScriptState>, sim: &mut SimState) {
    if probe.backup.is_none() { probe.cleaned = true; return; }
    probe.cleanup_attempts = probe.cleanup_attempts.saturating_add(1);
    let result = script.and_then(|s| s.rt.as_mut()).ok_or_else(|| "runtime retired before cleanup; no replacement Game is touched".to_string())
        .and_then(|rt| with_world(rt, sim, |rt| probe.cleanup(&mut rt.game)));
    if let Err(why) = result {
        error!("projectile launch probe: FAIL cleanup attempt={} {why}", probe.cleanup_attempts);
        probe.fail(format!("cleanup: {why}"));
        if probe.cleanup_attempts >= 3 { probe.stage = Stage::Done; }
    }
}
fn finish(mut probe: ResMut<Probe>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, combat: Res<Combat>, mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_some() && probe.stage != Stage::Done { probe.fail("application exit before diagnostic completion"); }
    if probe.stage == Stage::Done { return; }
    if probe.stage == Stage::Cleanup {
        if let Some(b) = probe.backup.as_ref() {
            let foreign = combat.script_shots.iter().any(|s| s.shooter == FormId::PLAYER_REF && s.weapon == b.weapon)
                || script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.combat_requests.iter().any(|r| matches!(r, fnv_game::combat::CombatRequest::Fire { from, weapon, .. } if *from == FormId::PLAYER_REF && *weapon == b.weapon)));
            if foreign { probe.fail("extra matching script request at completion"); }
        }
    }
    if probe.failed.is_some() || probe.stage == Stage::Cleanup {
        cleanup(&mut probe, script.as_deref_mut(), &mut sim);
        if probe.failed.is_none() && probe.cleaned && probe.requests == 2 && probe.accepted == 2 {
            probe.passed = true;
            info!("projectile launch probe: PASS stages=2 requests=2 physical_insertions=2 exact_fields_restored=true inventory_restored=true");
        }
        if probe.cleaned { probe.stage = Stage::Done; }
    }
}
fn cancel(mut probe: ResMut<Probe>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>) {
    if probe.backup.is_some() { probe.fail("active diagnostic interrupted by state teardown"); cleanup(&mut probe, script.as_deref_mut(), &mut sim); }
}
fn teardown(mut probe: ResMut<Probe>, mut travel: MessageReader<crate::Travel>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>) {
    if travel.read().next().is_some() && probe.backup.is_some() { probe.fail("active diagnostic interrupted by travel"); cleanup(&mut probe, script.as_deref_mut(), &mut sim); }
}

#[cfg(test)]
#[path = "projectile_launch_probe_tests.rs"]
mod tests;
