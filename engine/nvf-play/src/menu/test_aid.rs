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

use super::model::{Command, MenuModel, Side};
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
    time: Res<Time>,
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
