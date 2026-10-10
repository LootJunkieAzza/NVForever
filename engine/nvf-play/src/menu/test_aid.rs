//! Test aid for in-game checks of the menus without pressing keys.
//!
//! `NVF_PLAY_TEST_MENU=<container reference FormID, hex>` (or `nearest`: the container
//! nearest to the camera, as F9 picks it) runs a script through the same
//! paths as the keys (`OpenContainer`, `OpenInventory`, [`MenuInput`]): after
//! `NVF_PLAY_TEST_DELAY` seconds (default 6), one step per second, it opens the
//! container and logs both columns, takes all, logs, stores one unit of the player's
//! selected (first) row back, logs, closes, opens the inventory, equips (or unequips)
//! the first weapon or armour, logs, drops one unit of it, logs, and closes. With
//! `NVF_PLAY_TEST_SHOT=<path.png>` it saves screenshots of the container menu (`<path>`)
//! and of the inventory (`<path stem>-inventory.png`).
//!
//! `NVF_PLAY_TEST_DROP=<seconds>:<item FormID hex>[:stack|:use][,...]` drops (or uses) a
//! carried item through the inventory menu: at that time (once no menu is open) it opens
//! the inventory, half a second later selects the item's row and drops one unit (`Drop`,
//! the X key), the whole stack (`DropStack`, Shift+X) or uses it (`One`, Enter: equips, or
//! uses up an aid item) and logs the list, and half a second later closes the menu.
//! Dropping a quest item is refused: the logged status says so and the item stays (give
//! one first with `NVF_PLAY_TEST_GIVE`, e.g. The Platinum Chip, 001164FB).

use super::model::{Command, MenuModel, Mode, Side};
use super::{ActiveMenu, MenuInput, OpenInventory, OpenNearestContainer};
use crate::OpenContainer;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;
use std::path::PathBuf;

#[derive(Resource)]
pub(super) struct Script {
    /// `None`: the nearest container.
    container: Option<FormId>,
    shot: Option<PathBuf>,
    delay: f32,
    step: usize,
    next_at: Option<f32>,
}

pub(super) fn setup(app: &mut App) {
    setup_drops(app);
    let Ok(v) = std::env::var("NVF_PLAY_TEST_MENU") else { return };
    let container = match v.trim() {
        "nearest" => None,
        v => match v.parse::<FormId>() {
            Ok(id) => Some(id),
            Err(e) => {
                warn!("NVF_PLAY_TEST_MENU: {e}");
                return;
            }
        },
    };
    let delay = std::env::var("NVF_PLAY_TEST_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(6.0);
    let shot = std::env::var_os("NVF_PLAY_TEST_SHOT").map(PathBuf::from);
    app.insert_resource(Script { container, shot, delay, step: 0, next_at: None });
}

/// One scheduled drop (`NVF_PLAY_TEST_DROP`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DropJob {
    at: f32,
    item: FormId,
    /// `Drop`, `DropStack` or `One` (use / equip).
    cmd: Command,
}

pub(super) fn parse_drops(v: &str) -> Result<Vec<DropJob>, String> {
    crate::test_aid::timed(v)?
        .into_iter()
        .map(|(at, rest)| {
            let (item, cmd) = match rest.split_once(':') {
                Some((i, "stack")) => (i, Command::DropStack),
                Some((i, "use")) => (i, Command::One),
                Some((_, x)) => return Err(format!("{rest:?}: expected <item>[:stack|:use], not {x:?}")),
                None => (rest, Command::Drop),
            };
            let item = item.parse::<FormId>().map_err(|e| format!("{rest:?}: {e}"))?;
            Ok(DropJob { at, item, cmd })
        })
        .collect()
}

/// Scheduled drops, soonest last, and the one in progress (its stage and when the next
/// stage is due).
#[derive(Resource)]
pub(super) struct Drops {
    jobs: Vec<DropJob>,
    current: Option<(DropJob, u8, f32)>,
}

fn setup_drops(app: &mut App) {
    let Ok(v) = std::env::var("NVF_PLAY_TEST_DROP") else { return };
    match parse_drops(&v) {
        Ok(mut jobs) => {
            jobs.sort_by(|a, b| b.at.total_cmp(&a.at));
            app.insert_resource(Drops { jobs, current: None });
        }
        Err(e) => warn!("NVF_PLAY_TEST_DROP: {e}"),
    }
}

pub(super) fn run_drops(
    time: crate::pause::AidTime,
    drops: Option<ResMut<Drops>>,
    active: Res<ActiveMenu>,
    mut commands: Commands,
    mut open_inventory: MessageWriter<OpenInventory>,
    mut input: MessageWriter<MenuInput>,
) {
    let Some(mut d) = drops else { return };
    let now = time.elapsed_secs();
    let m = active.0.as_ref();
    match d.current {
        None => {
            if !d.jobs.last().is_some_and(|j| j.at <= now) {
                if d.jobs.is_empty() {
                    commands.remove_resource::<Drops>();
                }
                return;
            }
            if m.is_some() {
                return; // wait until no menu is open
            }
            let job = d.jobs.pop().expect("checked");
            info!("test aid: {:?} {}: open the inventory", job.cmd, job.item);
            open_inventory.write(OpenInventory);
            d.current = Some((job, 1, now + 0.5));
        }
        Some((job, stage, due)) => {
            if now < due {
                return;
            }
            match (stage, m) {
                (1, Some(m)) if m.mode == Mode::Inventory => {
                    match m.list(Side::Player).rows.iter().position(|r| r.item == job.item) {
                        Some(i) => {
                            // Selecting the selected row would use (equip) it: only when needed.
                            if m.selected[Side::Player as usize] != i {
                                input.write(MenuInput(Command::Select(Side::Player, i)));
                            }
                            input.write(MenuInput(job.cmd));
                        }
                        None => info!("test aid: {} is not carried", job.item),
                    }
                    d.current = Some((job, 2, now + 0.5));
                }
                (2, m) => {
                    log_menu("after test drop", m);
                    input.write(MenuInput(Command::Close));
                    d.current = None;
                }
                _ => {
                    info!("test aid: drop {}: the inventory did not open", job.item);
                    d.current = None;
                }
            }
        }
    }
}

fn log_menu(label: &str, m: Option<&MenuModel>) {
    let Some(m) = m else {
        info!("test aid [{label}]: no menu open");
        return;
    };
    info!("test aid [{label}]: {:?} \"{}\", status {:?}", m.mode, m.title, m.status);
    let sides: &[Side] = match m.mode {
        super::model::Mode::Container(_) => &[Side::Container, Side::Player],
        super::model::Mode::Inventory => &[Side::Player],
    };
    for &side in sides {
        let l = m.list(side);
        let rows: Vec<String> = l
            .rows
            .iter()
            .map(|r| format!("{}{} x{}", if r.equipped { "*" } else { "" }, r.name, r.count))
            .collect();
        info!("test aid [{label}]   {side:?}: {} stacks, weight {:.1}, value {}: {}", l.rows.len(), l.weight, l.value, rows.join(", "));
    }
    match &m.encumbrance {
        Ok(e) => info!("test aid [{label}]   encumbrance {:.1} / {:.0} over {}", e.weight, e.capacity, e.over),
        Err(e) => info!("test aid [{label}]   encumbrance unknown: {e}"),
    }
}

fn screenshot(commands: &mut Commands, path: PathBuf) {
    info!("test aid: screenshot {}", path.display());
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    time: crate::pause::AidTime,
    script: Option<ResMut<Script>>,
    active: Res<ActiveMenu>,
    mut commands: Commands,
    mut open_container: MessageWriter<OpenContainer>,
    mut open_nearest: MessageWriter<OpenNearestContainer>,
    mut open_inventory: MessageWriter<OpenInventory>,
    mut input: MessageWriter<MenuInput>,
) {
    let Some(mut s) = script else { return };
    let now = time.elapsed_secs();
    let due = s.next_at.unwrap_or(s.delay);
    if now < due {
        return;
    }
    s.next_at = Some(now + 1.0);
    let m = active.0.as_ref();
    let mut send = |c| {
        input.write(MenuInput(c));
    };
    match s.step {
        0 => {
            match s.container {
                Some(c) => {
                    info!("test aid: open container {c}");
                    open_container.write(OpenContainer { container: c.0 });
                }
                None => {
                    info!("test aid: open the nearest container");
                    open_nearest.write(OpenNearestContainer);
                }
            }
        }
        1 => {
            log_menu("opened", m);
            if let Some(p) = s.shot.clone() {
                screenshot(&mut commands, p);
            }
        }
        2 => send(Command::TakeAll),
        3 => log_menu("after take all", m),
        4 => {
            send(Command::Focus(Side::Player));
            send(Command::One);
        }
        5 => log_menu("after storing one back", m),
        6 => send(Command::Close),
        7 => {
            log_menu("closed", m);
            open_inventory.write(OpenInventory);
        }
        8 => log_menu("inventory", m),
        9 => {
            let wearable = m.and_then(|m| {
                m.list(Side::Player).rows.iter().position(|r| matches!(r.kind, Some(ItemKind::Weapon | ItemKind::Armor)))
            });
            match (wearable, m) {
                (Some(i), Some(m)) if m.selected[Side::Player as usize] != i => {
                    send(Command::Select(Side::Player, i));
                    send(Command::One);
                }
                (Some(_), Some(_)) => send(Command::One),
                _ => info!("test aid: no weapon or armour to equip"),
            }
        }
        10 => {
            log_menu("after equip", m);
            send(Command::Drop);
        }
        11 => {
            log_menu("after drop", m);
            if let Some(p) = s.shot.clone() {
                let stem = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
                screenshot(&mut commands, p.with_file_name(format!("{stem}-inventory.png")));
            }
        }
        12 => send(Command::Close),
        13 => {
            log_menu("closed", m);
            info!("test aid: done");
        }
        _ => {
            commands.remove_resource::<Script>();
            return;
        }
    }
    s.step += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_jobs() {
        let j = parse_drops("12:00015169, 20.5:0x00015169:stack").unwrap();
        assert_eq!(j, vec![
            DropJob { at: 12.0, item: FormId(0x15169), cmd: Command::Drop },
            DropJob { at: 20.5, item: FormId(0x15169), cmd: Command::DropStack },
        ]);
        assert_eq!(parse_drops("3:00015169:use").unwrap()[0].cmd, Command::One);
        assert!(parse_drops("12:00015169:all").is_err());
        assert!(parse_drops("12:zz").is_err());
    }
}
