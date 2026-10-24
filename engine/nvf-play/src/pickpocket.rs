//! Pickpocketing in play (docs/spec-notes/pickpocket.md; the rules are
//! `fnv_game::pickpocket`).
//!
//! - E on a living actor while the player sneaks shows **Steal** (`crate::activate`) and
//!   opens [`OpenPickpocket`]: the container menu on the actor's inventory in its steal
//!   mode (`crate::menu::model::Steal`), equipped items not listed.
//! - Each take or placement rolls against its chance, shown under the menu. A failed
//!   roll closes the menu: the victim says Pickpocket and turns on the player, and will
//!   not be pickpocketed again ("<name> has already caught you.", `sNoPickPocketAgain`).
//! - A take that succeeds is theft (karma, Pockets Picked). A grenade or mine placed in
//!   the pocket (a weapon whose projectile explodes and has Alt. Trigger [documented: GECK
//!   wiki Projectile notes]) goes live: after `fProjectileInventoryGrenadeTimer` its
//!   explosion goes off where the victim stands, killing it (Pants Exploded); its
//!   blast hurts others as a thrown one's does [hypothesis].
//!
//! Logged: `pickpocket: open on …`, `pickpocket: took …`, `pickpocket: caught by …`,
//! `pickpocket: planted …`, `pickpocket: … explodes in …'s pocket`, and at exit
//! `pickpocket summary: …`.

use crate::activate::HudMessage;
use crate::combat::{Combat, DESTRUCTION_BLAST_HEIGHT};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;

/// Open the container menu on `victim`'s pocket (steal mode).
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenPickpocket {
    pub victim: u32,
}

/// What a command of the steal menu did (`crate::menu`): items taken and placed, or the
/// item whose roll failed.
#[derive(Message, Clone, Debug)]
pub struct Pickpocketed {
    pub victim: FormId,
    pub taken: Vec<(FormId, i32)>,
    pub given: Vec<(FormId, i32)>,
    pub caught: Option<FormId>,
}

#[derive(Resource, Default)]
struct Stats {
    opened: u32,
    taken: u32,
    caught: u32,
    planted: u32,
    exploded: u32,
}

pub struct PickpocketPlugin;

impl Plugin for PickpocketPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stats>()
            .add_message::<OpenPickpocket>()
            .add_message::<Pickpocketed>()
            .add_systems(Update, (results, blasts.run_if(resource_exists::<Combat>)).chain().in_set(PlaySet::Activation))
            .add_systems(Update, count_opened)
            .add_systems(Last, summary);
    }
}

/// The text after a victim's name when it caught the player before: `sNoPickPocketAgain`
/// ("has already caught you." [observed: FalloutNV.esm]).
pub fn caught_before_line(sim: &SimState, name: &str) -> String {
    let tail = match sim.forms.settings.get("snopickpocketagain") {
        Some(fnv_sim::settings::Setting::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
        _ => "has already caught you.".into(),
    };
    format!("{name} {tail}")
}

/// Whether `item` is a live explosive once placed in a pocket: a weapon whose projectile
/// explodes and has the Alt. Trigger flag [documented: GECK wiki Projectile notes].
fn live_explosive(c: &Combat, item: FormId) -> Option<FormId> {
    let w = c.forms.weapons.get(&item)?;
    let p = c.forms.projectiles.get(&w.projectile?)?;
    p.explodes().filter(|_| p.has("Alt. Trigger"))
}

fn results(
    mut done: MessageReader<Pickpocketed>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    combat: Option<Res<Combat>>,
    mut stats: ResMut<Stats>,
    mut hud: Option<ResMut<HudMessage>>,
) {
    for p in done.read() {
        let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { continue };
        let victim = p.victim;
        let name = sim.ref_name(Some(&rt.game), victim.0).map_or_else(|| victim.to_string(), str::to_string);
        for &(item, n) in &p.taken {
            stats.taken += 1;
            crate::script::with_world(rt, &mut sim, |rt| rt.game.pickpocket_took(victim, item, n));
            info!("pickpocket: took {} x{n} from {name} {victim} unseen", item_name(&sim, item));
        }
        for &(item, n) in &p.given {
            match combat.as_deref().and_then(|c| live_explosive(c, item)) {
                Some(explosion) => {
                    for _ in 0..n {
                        rt.game.plant_explosive(victim, item);
                    }
                    stats.planted += n.max(0) as u32;
                    info!("pickpocket: planted {} x{n} (live, explosion {explosion}) on {name} {victim}", item_name(&sim, item));
                }
                None => info!("pickpocket: placed {} x{n} on {name} {victim}", item_name(&sim, item)),
            }
        }
        if let Some(item) = p.caught {
            stats.caught += 1;
            let turned = crate::script::with_world(rt, &mut sim, |rt| rt.game.pickpocket_caught(victim));
            info!("pickpocket: caught by {name} {victim} with {}{}", item_name(&sim, item), if turned { ": it turns on the player" } else { "" });
            if let Some(h) = hud.as_deref_mut() {
                h.show(format!("{name} caught you"));
            }
        }
    }
}

/// Planted explosives whose time is up go off where their victims stand.
fn blasts(mut sim: ResMut<SimState>, mut script: Option<ResMut<ScriptState>>, mut c: ResMut<Combat>, mut stats: ResMut<Stats>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    if rt.game.pickpocket.planted.is_empty() {
        return;
    }
    let due = crate::script::with_world(rt, &mut sim, |rt| rt.game.tick_planted());
    for b in due {
        stats.exploded += 1;
        let at = sim.world.peek(b.victim).map(|s| s.pos);
        match (live_explosive(&c, b.item), at) {
            (Some(e), Some(p)) => {
                c.mine_explodes(e, [p[0], p[1], p[2] + DESTRUCTION_BLAST_HEIGHT], Some((FormId::PLAYER_REF, b.item)));
                info!("pickpocket: {} explodes in {}'s pocket: it dies", item_name(&sim, b.item), b.victim);
            }
            _ => info!("pickpocket: {} explodes in {}'s pocket: it dies (no explosion drawn)", item_name(&sim, b.item), b.victim),
        }
    }
}

fn summary(mut exit: MessageReader<AppExit>, stats: Res<Stats>) {
    if exit.read().next().is_some() && stats.opened + stats.caught + stats.taken > 0 {
        info!(
            "pickpocket summary: {} pockets opened, {} takes unseen, caught {} times, {} explosives planted, {} went off",
            stats.opened, stats.taken, stats.caught, stats.planted, stats.exploded
        );
    }
}

fn count_opened(mut open: MessageReader<OpenPickpocket>, mut stats: ResMut<Stats>) {
    stats.opened += open.read().count() as u32;
}

fn item_name(sim: &SimState, id: FormId) -> String {
    sim.name(id).map_or_else(|| id.to_string(), str::to_string)
}
