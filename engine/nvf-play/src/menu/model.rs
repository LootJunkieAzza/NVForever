//! The menus as plain data over `fnv_sim::World`, without Bevy: rows, selection,
//! scrolling, and the commands that move items, all unit-tested on a synthetic plugin.
//!
//! Rows are one per base item (a stack), whatever extra data its units carry; the
//! simulation decides which units move (its removal order). Sorting is ours: by display
//! name, case-insensitively, FormID breaking ties. How the game orders and groups rows
//! (category tabs, sort keys) is not captured, so nothing here claims to follow it.
//!
//! Non-playable items (`Item::non_playable`, e.g. the Pip-Boy Glove) are not listed, so
//! they cannot be moved, equipped or dropped from a menu [observed by the owner: the game
//! never shows the glove in the inventory]; Take All leaves them in the container
//! [hypothesis: hidden items stay where they are].

use fnv_sim::forms::{BaseData, FormStore, ItemKind};
use fnv_sim::refs::PlacedRef;
use fnv_sim::world::{Encumbrance, SimError};
use fnv_sim::{FormId, RefTable, World};

/// Rows shown at once per column; the list scrolls to keep the selection visible.
pub const VISIBLE_ROWS: usize = 16;

/// One stack in an inventory.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub item: FormId,
    pub name: String,
    pub count: i32,
    /// Weight of one unit as authored (`None`: not decoded). Ammunition weighs nothing in
    /// the totals outside Hardcore mode (the simulation's rule), but its row shows the
    /// authored weight.
    pub weight: Option<f32>,
    /// Value of one unit.
    pub value: Option<i32>,
    pub kind: Option<ItemKind>,
    /// At least one unit is equipped.
    pub equipped: bool,
}

/// An inventory's rows (sorted) and totals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    pub rows: Vec<Row>,
    /// Total weight as the simulation counts it (`Inventory::totals`), hidden
    /// (non-playable) items included: they are carried.
    pub weight: f64,
    /// Value of the listed stacks.
    pub value: i64,
}

/// Display name of a form: its FULL name, else its Editor ID, else its FormID.
pub fn display_name(forms: &FormStore, id: FormId) -> String {
    match forms.get(id) {
        Some(b) => b.name.clone().filter(|n| !n.trim().is_empty()).or_else(|| b.edid.clone()).unwrap_or_else(|| id.to_string()),
        None => id.to_string(),
    }
}

/// The rows of `holder`'s inventory, sorted by name.
/// A failed command in words for the player: items by name, not FormID. Starts with
/// "Cannot" (the menu draws such lines as warnings).
pub fn plain_error(forms: &FormStore, e: &SimError) -> String {
    let name = |id: &FormId| forms.get(*id).and_then(|b| b.name.clone().or_else(|| b.edid.clone())).unwrap_or_else(|| "this item".into());
    match e {
        SimError::NotEquippable(item) => format!("Cannot equip or use {}", name(item)),
        SimError::NotCarried { item, .. } => format!("Cannot find {} in the inventory", name(item)),
        SimError::NotAnItem(item) => format!("Cannot carry {}", name(item)),
        SimError::Deleted(item) => format!("Cannot reach {}: it is gone", name(item)),
        other => format!("Cannot: {other}"),
    }
}

pub fn listing(world: &mut World<'_>, holder: FormId) -> Result<Listing, SimError> {
    let forms = world.forms;
    let hardcore = world.config().hardcore;
    let s = world.get(holder)?;
    if s.deleted {
        return Err(SimError::Deleted(holder));
    }
    let inv = s.inventory.as_ref().ok_or(SimError::NoInventory(holder))?;
    let totals = inv.totals(forms, hardcore);
    let mut rows: Vec<Row> = inv
        .iter()
        .filter(|&(item, _)| !forms.item(item).is_some_and(|i| i.non_playable))
        .map(|(item, count)| {
            let it = forms.item(item);
            Row {
                item,
                name: display_name(forms, item),
                count,
                weight: it.and_then(|i| i.weight),
                value: it.and_then(|i| i.value),
                kind: it.map(|i| i.kind),
                equipped: inv.is_equipped(item),
            }
        })
        .collect();
    rows.sort_by_cached_key(|r| (r.name.to_lowercase(), r.item));
    let value = rows.iter().map(|r| r.value.unwrap_or(0) as i64 * r.count as i64).sum();
    Ok(Listing { rows, weight: totals.weight, value })
}

/// A column of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The container (left).
    Container = 0,
    /// The player (right; the only column of the inventory menu).
    Player = 1,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Container => Side::Player,
            Side::Player => Side::Container,
        }
    }
}

/// Which menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The container menu on this container reference: container | player.
    Container(FormId),
    /// The player's inventory alone.
    Inventory,
}

/// What the player asks the menu to do (keys, mouse and the test aid all go through
/// this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Up,
    Down,
    PageUp,
    PageDown,
    /// Move the selection by this many rows (mouse wheel).
    Scroll(i32),
    /// Focus a column (container menu).
    Focus(Side),
    /// Select a row (mouse). Selecting the row that is already selected does
    /// [`Command::One`].
    Select(Side, usize),
    /// Container menu: move one unit of the selected stack to the other column (take
    /// from the container, store from the player). Inventory: equip or unequip it (aid
    /// items are used).
    One,
    /// Container menu: move the whole selected stack.
    Stack,
    /// Container menu: take everything from the container.
    TakeAll,
    /// Inventory: drop one unit of the selected stack into the world.
    Drop,
    /// Inventory: drop the whole selected stack.
    DropStack,
    Close,
}

/// The state of an open menu.
#[derive(Debug, Clone)]
pub struct MenuModel {
    pub mode: Mode,
    /// The column that has the keyboard (always `Player` in the inventory menu).
    pub focus: Side,
    /// Selected row per column, indexed by `Side as usize`.
    pub selected: [usize; 2],
    /// First visible row per column.
    pub scroll: [usize; 2],
    /// Rows per column; the container column is empty in the inventory menu.
    pub lists: [Listing; 2],
    /// The player's encumbrance, or why it is unknown.
    pub encumbrance: Result<Encumbrance, String>,
    /// Heading: the container's name, or "Inventory".
    pub title: String,
    /// The last command's result or error, shown in the menu.
    pub status: Option<String>,
    /// References created by drops (the simulation has them; nothing renders them yet).
    pub dropped: Vec<FormId>,
}

/// What a command did besides changing the model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// The menu should close.
    pub close: bool,
    /// The inventories changed.
    pub changed: bool,
    /// References created by this command (drops).
    pub created: Vec<FormId>,
    /// An item the player used up (an aid item "equipped" from the inventory): its
    /// effects (and effect scripts) are the script runtime's (`crate::ItemUsed`).
    pub used: Option<FormId>,
}

const PLAYER: FormId = FormId::PLAYER_REF;

impl MenuModel {
    /// Open the container menu. Fails when the reference has no inventory.
    pub fn open_container(world: &mut World<'_>, container: FormId) -> Result<MenuModel, SimError> {
        let left = listing(world, container)?;
        let base = world.get(container)?.base;
        let title = display_name(world.forms, base);
        let mut m = MenuModel::empty(Mode::Container(container), title);
        m.lists[0] = left;
        m.focus = Side::Container;
        m.refresh(world)?;
        Ok(m)
    }

    /// Open the player's inventory.
    pub fn open_inventory(world: &mut World<'_>) -> Result<MenuModel, SimError> {
        let mut m = MenuModel::empty(Mode::Inventory, "Inventory".into());
        m.focus = Side::Player;
        m.refresh(world)?;
        Ok(m)
    }

    fn empty(mode: Mode, title: String) -> MenuModel {
        MenuModel {
            mode,
            focus: Side::Player,
            selected: [0; 2],
            scroll: [0; 2],
            lists: [Listing::default(), Listing::default()],
            encumbrance: Err(String::new()),
            title,
            status: None,
            dropped: Vec::new(),
        }
    }

    pub fn list(&self, side: Side) -> &Listing {
        &self.lists[side as usize]
    }

    /// The selected row of a column.
    pub fn selected_row(&self, side: Side) -> Option<&Row> {
        self.lists[side as usize].rows.get(self.selected[side as usize])
    }

    /// Indices of the visible rows of a column.
    pub fn visible(&self, side: Side) -> std::ops::Range<usize> {
        let n = self.lists[side as usize].rows.len();
        let start = self.scroll[side as usize].min(n);
        start..(start + VISIBLE_ROWS).min(n)
    }

    fn columns(&self) -> &'static [Side] {
        match self.mode {
            Mode::Container(_) => &[Side::Container, Side::Player],
            Mode::Inventory => &[Side::Player],
        }
    }

    /// Re-read the inventories from the simulation, keeping each column's selection on
    /// the same item when it is still there (else on the same position).
    pub fn refresh(&mut self, world: &mut World<'_>) -> Result<(), SimError> {
        for &side in self.columns() {
            let i = side as usize;
            let keep = self.selected_row(side).map(|r| r.item);
            let holder = match (side, self.mode) {
                (Side::Container, Mode::Container(c)) => c,
                _ => PLAYER,
            };
            self.lists[i] = listing(world, holder)?;
            let rows = &self.lists[i].rows;
            let at = keep.and_then(|k| rows.iter().position(|r| r.item == k));
            self.selected[i] = at.unwrap_or(self.selected[i]).min(rows.len().saturating_sub(1));
            self.ensure_visible(side);
        }
        self.encumbrance = world.encumbrance(PLAYER).map_err(|e| e.to_string());
        Ok(())
    }

    fn ensure_visible(&mut self, side: Side) {
        let i = side as usize;
        let n = self.lists[i].rows.len();
        let (sel, s) = (self.selected[i], &mut self.scroll[i]);
        if sel < *s {
            *s = sel;
        } else if sel >= *s + VISIBLE_ROWS {
            *s = sel + 1 - VISIBLE_ROWS;
        }
        *s = (*s).min(n.saturating_sub(VISIBLE_ROWS));
    }

    fn move_selection(&mut self, delta: i64) {
        let i = self.focus as usize;
        let n = self.lists[i].rows.len() as i64;
        if n == 0 {
            return;
        }
        self.selected[i] = (self.selected[i] as i64 + delta).clamp(0, n - 1) as usize;
        self.ensure_visible(self.focus);
    }

    /// Apply a command. Errors from the simulation become the status line; nothing
    /// panics.
    pub fn apply(&mut self, world: &mut World<'_>, cmd: Command) -> Applied {
        let mut out = Applied::default();
        let page = VISIBLE_ROWS as i64 - 1;
        match cmd {
            Command::Up => self.move_selection(-1),
            Command::Down => self.move_selection(1),
            Command::PageUp => self.move_selection(-page),
            Command::PageDown => self.move_selection(page),
            Command::Scroll(d) => self.move_selection(d as i64),
            Command::Focus(side) => {
                if self.columns().contains(&side) {
                    self.focus = side;
                }
            }
            Command::Select(side, idx) => {
                if !self.columns().contains(&side) || idx >= self.lists[side as usize].rows.len() {
                    return out;
                }
                if self.focus == side && self.selected[side as usize] == idx {
                    return self.apply(world, Command::One);
                }
                self.focus = side;
                self.selected[side as usize] = idx;
                self.ensure_visible(side);
            }
            Command::Close => out.close = true,
            _ => {
                let r = self.act(world, cmd, &mut out);
                self.status = Some(match r {
                    Ok(msg) => msg,
                    Err(e) => plain_error(world.forms, &e),
                });
                if out.changed {
                    if let Err(e) = self.refresh(world) {
                        self.status = Some(format!("Cannot list the inventories: {e}"));
                    }
                }
            }
        }
        out
    }

    /// The commands that change inventories; returns the status message.
    fn act(&mut self, world: &mut World<'_>, cmd: Command, out: &mut Applied) -> Result<String, SimError> {
        let forms = world.forms;
        let sel = self.selected_row(self.focus).cloned();
        let name = |r: &Row| r.name.clone();
        match (self.mode, cmd) {
            (Mode::Container(c), Command::TakeAll) => {
                // The listed stacks only: non-playable items stay hidden in the container.
                let mut moved = Vec::new();
                for r in &self.lists[Side::Container as usize].rows {
                    let n = world.transfer(c, PLAYER, r.item, r.count)?;
                    if n > 0 {
                        moved.push((r.item, n));
                    }
                }
                out.changed = !moved.is_empty();
                let n: i64 = moved.iter().map(|m| m.1 as i64).sum();
                Ok(if moved.is_empty() { "Nothing to take".into() } else { format!("Took everything ({n} items)") })
            }
            (Mode::Container(c), Command::One | Command::Stack) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                let (from, to) = match self.focus {
                    Side::Container => (c, PLAYER),
                    Side::Player => (PLAYER, c),
                };
                let want = if cmd == Command::One { 1 } else { r.count };
                let n = world.transfer(from, to, r.item, want)?;
                out.changed = n > 0;
                let verb = if self.focus == Side::Container { "Took" } else { "Stored" };
                Ok(format!("{verb} {} x{n}", name(&r)))
            }
            (Mode::Inventory, Command::One) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                out.changed = true;
                if r.equipped {
                    world.unequip(PLAYER, r.item)?;
                    return Ok(format!("Unequipped {}", name(&r)));
                }
                let e = world.equip(PLAYER, r.item)?;
                if e.consumed {
                    out.used = Some(r.item);
                }
                let off: Vec<String> = e.unequipped.iter().map(|&i| display_name(forms, i)).collect();
                Ok(match (e.consumed, off.is_empty()) {
                    (true, _) => format!("Used {}", name(&r)),
                    (false, true) => format!("Equipped {}", name(&r)),
                    (false, false) => format!("Equipped {} (took off {})", name(&r), off.join(", ")),
                })
            }
            (Mode::Inventory, Command::Drop | Command::DropStack) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                let want = if cmd == Command::Drop { 1 } else { r.count };
                let created = world.drop_item(PLAYER, r.item, want)?;
                out.changed = !created.is_empty();
                self.dropped.extend(&created);
                out.created = created;
                Ok(format!("Dropped {} x{want}", name(&r)))
            }
            (Mode::Container(_), Command::Drop | Command::DropStack) => Ok("Drop from the inventory menu (Tab)".into()),
            (Mode::Inventory, Command::Stack | Command::TakeAll) => Ok("No container open".into()),
            _ => Ok(String::new()),
        }
    }

    /// Key hints for the footer, for the current mode, one line each.
    pub fn bindings(&self) -> [&'static str; 2] {
        match self.mode {
            Mode::Container(_) => [
                "Up/Down select   Left/Right column   Enter/Space move one   Shift+Enter move stack   R take all",
                "Click a row to select it, again to move one   Wheel scroll   E/Esc/Tab close   * equipped",
            ],
            Mode::Inventory => [
                "Up/Down select   Enter/Space equip/unequip (aid: use)   X drop one   Shift+X drop stack",
                "Click a row to select it, again to equip   Wheel scroll   Tab/Esc/E close   * equipped",
            ],
        }
    }
}

/// Test aid (F9): the container reference nearest to `pos` (game space) that is in the
/// world now, with its distance. `interior`: search that cell; `None`: the worldspace
/// `world`. Containers are references whose base is a CONT.
pub fn nearest_container(
    world: &World<'_>,
    refs: &RefTable,
    interior: Option<FormId>,
    worldspace: FormId,
    pos: [f32; 3],
) -> Option<(FormId, f32)> {
    let forms = world.forms;
    let candidates: Box<dyn Iterator<Item = &PlacedRef>> = match interior {
        Some(cell) => Box::new(refs.by_cell.get(&cell).into_iter().flatten().filter_map(|&id| refs.get(id))),
        None => Box::new(refs.refs.values().filter(|p| p.cell.and_then(|c| refs.cell_world.get(&c)) == Some(&worldspace))),
    };
    candidates
        .filter_map(|p| {
            let (base, at, gone) = match world.peek(p.id) {
                Some(s) => (s.base, s.pos, s.deleted),
                None => (p.base, p.pos, false),
            };
            let is_container = matches!(forms.get(base).map(|b| &b.data), Some(BaseData::Container(_)));
            let present = !gone && world.is_enabled(p.id).unwrap_or(true);
            let d = ((at[0] - pos[0]).powi(2) + (at[1] - pos[1]).powi(2) + (at[2] - pos[2]).powi(2)).sqrt();
            (is_container && present).then_some((p.id, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
}

#[cfg(test)]
mod tests;
