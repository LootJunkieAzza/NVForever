//! The menus as plain data over `fnv_sim::World`, without Bevy: rows, selection,
//! scrolling, and the commands that move items, all unit-tested on a synthetic plugin.
//!
//! Rows are one per base item (a stack), whatever extra data its units carry; the
//! simulation decides which units move (its removal order). Rows are sorted by display
//! name in byte order, capitals before small letters, FormID breaking ties [observed: the
//! owner's barter screenshot lists "LAER+" before "Lad's Life", and "$5 NCR" and ".223
//! Round" before letters; docs/spec-notes/barter-menu.md §3].
//!
//! The container and barter menus list one category at a time, chosen with the arrows
//! beside each list's title: all items, weapons, apparel, aid, misc, ammunition
//! [observed: "ITEMS" and "MISC" titles on the owner's screenshots; the other names and
//! their order are hypotheses, after the Pip-Boy's item pages].
//!
//! Non-playable items (`Item::non_playable`, e.g. the Pip-Boy Glove) are not listed, so
//! they cannot be moved, equipped or dropped from a menu [observed by the owner: the game
//! never shows the glove in the inventory]; Take All leaves them in the container
//! [hypothesis: hidden items stay where they are].
//!
//! Quest items (`World::is_quest_item`: the record's "Quest Item" header flag, or a
//! script's SetQuestObject) are listed and marked; dropping one is refused by the
//! simulation and the status line says so [documented: GECK wiki Category:Items, the
//! player cannot drop one]. Storing one in a container is allowed [hypothesis: the page
//! does not mention containers]. They cannot be sold [documented: the same page], so the
//! barter menu does not list them [hypothesis: hidden rather than refused].
//!
//! Barter ([`Mode::Barter`], [`Trade`]): items are put up from either side and listed on
//! the other side, first, marked, until the trade is accepted [observed: the owner's
//! screenshot shows the sold Gold Bar at the top of the merchant's list with an open
//! square, gone from the player's]; caps are not listed but shown in each list's heading
//! [observed: no Bottle Cap row in the barter screenshots, while the container
//! screenshot lists one]. Accept moves the items and the caps the prices add up to; it
//! is refused when the side that pays has too few caps [hypothesis: the wording is ours].

use super::barter::{self, Prices};
use fnv_sim::forms::{BaseData, FormStore, ItemKind};
use fnv_sim::refs::PlacedRef;
use fnv_sim::world::{Encumbrance, SimError};
use fnv_sim::{FormId, RefTable, World};
use std::collections::BTreeMap;

/// Rows the inventory (the Pip-Boy's ITEMS list) shows at once; the list scrolls to keep
/// the selection visible. Its list is 468 high and a row 51 (`pipboy::items`).
pub const VISIBLE_ROWS: usize = 9;
/// Rows the container and barter lists show at once: the list is 365 high and a row 51
/// (docs/spec-notes/barter-menu.md §2).
pub const LIST_ROWS: usize = 7;

/// The caps item, Bottle Cap [documented: FalloutNV.esm MISC 0x0000000F, the item the
/// vendor caps lists hold].
pub const CAPS: FormId = FormId(0x0000000F);

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
    /// A quest item: the player cannot drop it (`World::is_quest_item`).
    pub quest: bool,
    /// The condition of the unit in use, 0 to 1 (weapons and armour).
    pub condition: Option<f32>,
    /// The item's picture (ICON, a path under `textures\`).
    pub icon: Option<String>,
    /// Barter: what one unit fetches in this trade, in caps (before rounding).
    pub price: Option<f32>,
    /// Barter: units the other side puts up, listed here until the trade is accepted.
    pub bartered: bool,
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

/// What a row says of a worn weapon or piece of armour after its name: the condition
/// of the unit in use (the equipped one, else the one that would be equipped), nothing
/// at full condition (docs/spec-notes/combat-rules.md §17).
pub fn condition_note(kind: Option<ItemKind>, condition: Option<f32>) -> String {
    match (kind, condition) {
        (Some(ItemKind::Weapon | ItemKind::Armor), Some(c)) if c <= 0.0 => " (broken)".into(),
        (Some(ItemKind::Weapon | ItemKind::Armor), Some(c)) if c < 1.0 => format!(" (CND {}%)", fnv_combat::condition::percent(c)),
        _ => String::new(),
    }
}

/// Display name of a form: its FULL name, else its Editor ID, else its FormID.
pub fn display_name(forms: &FormStore, id: FormId) -> String {
    match forms.get(id) {
        Some(b) => b.name.clone().filter(|n| !n.trim().is_empty()).or_else(|| b.edid.clone()).unwrap_or_else(|| id.to_string()),
        None => id.to_string(),
    }
}

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

/// The order rows are listed in: put-up rows first, then by name in byte order.
fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| (!a.bartered, a.name.as_bytes(), a.item).cmp(&(!b.bartered, b.name.as_bytes(), b.item)));
}

/// The rows of `holder`'s inventory, sorted by name.
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
            let kind = it.map(|i| i.kind);
            Row {
                item,
                name: display_name(forms, item),
                count,
                weight: it.and_then(|i| i.weight),
                value: it.and_then(|i| i.value),
                kind,
                equipped: inv.is_equipped(item),
                quest: false, // below, once the inventory is no longer borrowed
                condition: inv.condition_in_use(item).filter(|_| matches!(kind, Some(ItemKind::Weapon | ItemKind::Armor))),
                icon: it.and_then(|i| i.icon.clone()),
                price: None,
                bartered: false,
            }
        })
        .collect();
    for r in &mut rows {
        r.quest = world.is_quest_item(r.item);
    }
    sort_rows(&mut rows);
    let value = rows.iter().map(|r| r.value.unwrap_or(0) as i64 * r.count as i64).sum();
    Ok(Listing { rows, weight: totals.weight, value })
}

/// A column of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The container, or the merchant (drawn on the right).
    Container = 0,
    /// The player (drawn on the left; the only column of the inventory menu).
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

/// The items a list shows, chosen with the arrows beside its title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    All,
    Weapons,
    Apparel,
    Aid,
    Misc,
    Ammo,
}

impl Category {
    pub const ORDER: [Category; 6] = [Category::All, Category::Weapons, Category::Apparel, Category::Aid, Category::Misc, Category::Ammo];

    /// Whether an item of this kind is listed.
    pub fn holds(self, kind: Option<ItemKind>) -> bool {
        let of = match kind {
            Some(ItemKind::Weapon) => Category::Weapons,
            Some(ItemKind::Armor) => Category::Apparel,
            Some(ItemKind::Aid) => Category::Aid,
            Some(ItemKind::Ammo) => Category::Ammo,
            _ => Category::Misc,
        };
        self == Category::All || self == of
    }

    /// The next category (`by` 1) or the one before (-1), round the ends.
    pub fn step(self, by: i32) -> Category {
        let n = Self::ORDER.len() as i32;
        let i = Self::ORDER.iter().position(|&c| c == self).unwrap_or(0) as i32;
        Self::ORDER[(i + by).rem_euclid(n) as usize]
    }

    /// The player's list title [observed: "ITEMS", "MISC"; the others hypotheses].
    pub fn title(self) -> &'static str {
        match self {
            Category::All => "ITEMS",
            Category::Weapons => "WEAPONS",
            Category::Apparel => "APPAREL",
            Category::Aid => "AID",
            Category::Misc => "MISC",
            Category::Ammo => "AMMO",
        }
    }
}

/// Which menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The container menu on this container reference: player | container.
    Container(FormId),
    /// The player's inventory alone.
    Inventory,
    /// The barter menu with this merchant (an actor reference): player | merchant.
    Barter(FormId),
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
    /// Focus a column (container and barter menus).
    Focus(Side),
    /// Select a row (mouse). Selecting the row that is already selected does
    /// [`Command::One`].
    Select(Side, usize),
    /// Select a row without acting on it (the pointer came onto it).
    Hover(Side, usize),
    /// Show the next (1) or previous (-1) category of a column.
    Category(Side, i32),
    /// Show this category in a column (the Pip-Boy's ITEMS tabs).
    SetCategory(Side, Category),
    /// Container menu: move one unit of the selected stack to the other column (take
    /// from the container, store from the player). Barter: put one unit up, or take one
    /// back. Inventory: equip or unequip it (aid items are used).
    One,
    /// Container and barter menus: the same for the whole selected stack.
    Stack,
    /// Container menu: take everything from the container.
    TakeAll,
    /// Barter: make the trade.
    Accept,
    /// Inventory: drop one unit of the selected stack into the world.
    Drop,
    /// Inventory: drop the whole selected stack.
    DropStack,
    Close,
}

/// A trade being put together in the barter menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub merchant: FormId,
    /// What holds the merchant's goods and caps: its merchant container (XMRC), else the
    /// merchant itself [observed: Chet's container VendorContainerChet holds his stock
    /// and the vendor caps lists, his own inventory only pocket change; hypothesis: the
    /// merchant's own inventory is not offered].
    pub stock: FormId,
    /// The item that counts as money.
    pub caps: FormId,
    /// `ShowBarterMenu`'s percentage.
    pub discount: i32,
    /// The player's Barter skill.
    pub skill: f32,
    pub prices: Prices,
    /// Units put up, by the side they come from (`Side as usize`).
    pub offered: [BTreeMap<FormId, i32>; 2],
    /// The caps each side holds (`Side as usize`).
    pub purse: [i32; 2],
    /// The caps the trade moves: positive when the player is paid.
    pub balance: i64,
}

impl Trade {
    /// What one unit of value is worth when it comes from `from`.
    fn mult(&self, from: Side) -> f32 {
        match from {
            Side::Player => self.prices.selling(self.skill, self.discount),
            Side::Container => self.prices.buying(self.skill, self.discount),
        }
    }

    /// What holds a side's items.
    fn holder(&self, side: Side) -> FormId {
        match side {
            Side::Container => self.stock,
            Side::Player => PLAYER,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.offered.iter().all(BTreeMap::is_empty)
    }
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
    /// The category each column shows.
    pub category: [Category; 2],
    /// The player's encumbrance, or why it is unknown.
    pub encumbrance: Result<Encumbrance, String>,
    /// Heading: the container's or merchant's name, or "Inventory".
    pub title: String,
    /// The last command's result or error, shown in the menu.
    pub status: Option<String>,
    /// References created by drops (the simulation has them; nothing renders them yet).
    pub dropped: Vec<FormId>,
    /// The barter menu's trade.
    pub trade: Option<Trade>,
}

/// What a command did besides changing the model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// The menu should close.
    pub close: bool,
    /// The inventories (or the trade) changed.
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
        let base = world.get(container)?.base;
        let title = display_name(world.forms, base);
        let mut m = MenuModel::empty(Mode::Container(container), title);
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

    /// Open the barter menu with `merchant`, whose goods and caps `stock` holds. Fails
    /// when either has no inventory.
    pub fn open_barter(world: &mut World<'_>, merchant: FormId, stock: FormId, caps: FormId, skill: f32, discount: i32) -> Result<MenuModel, SimError> {
        let base = world.get(merchant)?.base;
        let title = display_name(world.forms, base);
        let mut m = MenuModel::empty(Mode::Barter(merchant), title);
        m.trade = Some(Trade {
            merchant,
            stock,
            caps,
            discount,
            skill,
            prices: Prices::from_settings(world.forms),
            offered: [BTreeMap::new(), BTreeMap::new()],
            purse: [0, 0],
            balance: 0,
        });
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
            category: [Category::All; 2],
            encumbrance: Err(String::new()),
            title,
            status: None,
            dropped: Vec::new(),
            trade: None,
        }
    }

    pub fn list(&self, side: Side) -> &Listing {
        &self.lists[side as usize]
    }

    /// The selected row of a column.
    pub fn selected_row(&self, side: Side) -> Option<&Row> {
        self.lists[side as usize].rows.get(self.selected[side as usize])
    }

    /// Rows shown at once.
    pub fn page(&self) -> usize {
        match self.mode {
            Mode::Inventory => VISIBLE_ROWS,
            _ => LIST_ROWS,
        }
    }

    /// Indices of the visible rows of a column.
    pub fn visible(&self, side: Side) -> std::ops::Range<usize> {
        let n = self.lists[side as usize].rows.len();
        let start = self.scroll[side as usize].min(n);
        start..(start + self.page()).min(n)
    }

    fn columns(&self) -> &'static [Side] {
        match self.mode {
            Mode::Container(_) | Mode::Barter(_) => &[Side::Container, Side::Player],
            Mode::Inventory => &[Side::Player],
        }
    }

    /// Re-read the inventories from the simulation, keeping each column's selection on
    /// the same row when it is still there (else on the same position).
    pub fn refresh(&mut self, world: &mut World<'_>) -> Result<(), SimError> {
        let keep = [Side::Container, Side::Player].map(|s| self.selected_row(s).map(|r| (r.item, r.bartered)));
        if self.trade.is_some() {
            self.refresh_trade(world)?;
        } else {
            for &side in self.columns() {
                let holder = match (side, self.mode) {
                    (Side::Container, Mode::Container(c)) => c,
                    _ => PLAYER,
                };
                let mut l = listing(world, holder)?;
                l.rows.retain(|r| self.category[side as usize].holds(r.kind));
                self.lists[side as usize] = l;
            }
        }
        for &side in self.columns() {
            let i = side as usize;
            let rows = &self.lists[i].rows;
            let at = keep[i].and_then(|k| rows.iter().position(|r| (r.item, r.bartered) == k));
            self.selected[i] = at.unwrap_or(self.selected[i]).min(rows.len().saturating_sub(1));
            self.ensure_visible(side);
        }
        self.encumbrance = world.encumbrance(PLAYER).map_err(|e| e.to_string());
        Ok(())
    }

    /// The barter lists: each side's own rows less what it put up, and what the other
    /// side put up; prices; the caps on each side and the trade's balance.
    fn refresh_trade(&mut self, world: &mut World<'_>) -> Result<(), SimError> {
        let Some(t) = self.trade.as_mut() else { return Ok(()) };
        let mut own: [Vec<Row>; 2] = [Vec::new(), Vec::new()];
        let mut weights = [0.0; 2];
        for side in [Side::Container, Side::Player] {
            let i = side as usize;
            let l = listing(world, t.holder(side))?;
            weights[i] = l.weight;
            t.purse[i] = world.item_count(t.holder(side), t.caps)?;
            let mult = t.mult(side);
            let prices = t.prices;
            own[i] = l
                .rows
                .into_iter()
                .filter(|r| r.item != t.caps && !r.quest)
                .map(|mut r| {
                    r.price = r.value.map(|v| prices.value_at(v, r.condition) * mult);
                    r
                })
                .collect();
            // What is no longer there cannot stay put up.
            let rows = &own[i];
            t.offered[i].retain(|item, n| match rows.iter().find(|r| r.item == *item) {
                Some(r) => {
                    *n = (*n).min(r.count);
                    *n > 0
                }
                None => false,
            });
        }
        let mut shown: [Vec<Row>; 2] = [Vec::new(), Vec::new()];
        let mut lines: [Vec<(f32, i32)>; 2] = [Vec::new(), Vec::new()];
        for side in [Side::Container, Side::Player] {
            let i = side as usize;
            for r in &own[i] {
                let up = t.offered[i].get(&r.item).copied().unwrap_or(0);
                if r.count > up {
                    shown[i].push(Row { count: r.count - up, ..r.clone() });
                }
                if up > 0 {
                    lines[i].push((r.price.unwrap_or(0.0), up));
                    // Shown on the other side without the equipped mark [hypothesis].
                    shown[side.other() as usize].push(Row { count: up, bartered: true, equipped: false, ..r.clone() });
                }
            }
        }
        t.balance = barter::balance(lines[Side::Player as usize].iter().copied(), lines[Side::Container as usize].iter().copied());
        for side in [Side::Container, Side::Player] {
            let i = side as usize;
            let mut rows = std::mem::take(&mut shown[i]);
            rows.retain(|r| self.category[i].holds(r.kind));
            sort_rows(&mut rows);
            let value = rows.iter().map(|r| r.value.unwrap_or(0) as i64 * r.count as i64).sum();
            self.lists[i] = Listing { rows, weight: weights[i], value };
        }
        Ok(())
    }

    fn ensure_visible(&mut self, side: Side) {
        let page = self.page();
        let i = side as usize;
        let n = self.lists[i].rows.len();
        let (sel, s) = (self.selected[i], &mut self.scroll[i]);
        if sel < *s {
            *s = sel;
        } else if sel >= *s + page {
            *s = sel + 1 - page;
        }
        *s = (*s).min(n.saturating_sub(page));
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
        let page = self.page() as i64 - 1;
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
            Command::Select(side, idx) | Command::Hover(side, idx) => {
                if !self.columns().contains(&side) || idx >= self.lists[side as usize].rows.len() {
                    return out;
                }
                if matches!(cmd, Command::Select(..)) && self.focus == side && self.selected[side as usize] == idx {
                    return self.apply(world, Command::One);
                }
                self.focus = side;
                self.selected[side as usize] = idx;
                self.ensure_visible(side);
            }
            Command::SetCategory(side, cat) => {
                let i = side as usize;
                if !self.columns().contains(&side) || self.category[i] == cat {
                    return out;
                }
                self.category[i] = cat;
                self.selected[i] = 0;
                self.scroll[i] = 0;
                if let Err(e) = self.refresh(world) {
                    self.status = Some(format!("Cannot list the inventories: {e}"));
                }
            }
            Command::Category(side, by) => {
                if self.mode == Mode::Inventory || !self.columns().contains(&side) {
                    return out;
                }
                let i = side as usize;
                self.category[i] = self.category[i].step(by);
                self.selected[i] = 0;
                self.scroll[i] = 0;
                if let Err(e) = self.refresh(world) {
                    self.status = Some(format!("Cannot list the inventories: {e}"));
                }
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
            (Mode::Barter(_), Command::One | Command::Stack) => {
                let Some(r) = sel else { return Ok("Nothing selected".into()) };
                let Some(t) = self.trade.as_mut() else { return Ok(String::new()) };
                let want = if cmd == Command::One { 1 } else { r.count };
                out.changed = true;
                if r.bartered {
                    // Taken back by the side it came from.
                    let from = self.focus.other() as usize;
                    let left = t.offered[from].get(&r.item).copied().unwrap_or(0) - want;
                    if left > 0 {
                        t.offered[from].insert(r.item, left);
                    } else {
                        t.offered[from].remove(&r.item);
                    }
                    return Ok(format!("Took back {} x{want}", name(&r)));
                }
                *t.offered[self.focus as usize].entry(r.item).or_insert(0) += want;
                let verb = if self.focus == Side::Player { "Selling" } else { "Buying" };
                Ok(format!("{verb} {} x{want}", name(&r)))
            }
            (Mode::Barter(_), Command::Accept) => self.accept(world, out),
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
                let created = match world.player_drop_item(r.item, want) {
                    Err(SimError::QuestItem(_)) => return Ok(format!("{} is a quest item and can't be dropped", name(&r))),
                    other => other?,
                };
                out.changed = !created.is_empty();
                self.dropped.extend(&created);
                out.created = created;
                Ok(format!("Dropped {} x{want}", name(&r)))
            }
            (Mode::Container(_) | Mode::Barter(_), Command::Drop | Command::DropStack) => Ok("Drop from the inventory menu (Tab)".into()),
            (Mode::Inventory, Command::Stack | Command::TakeAll) => Ok("No container open".into()),
            _ => Ok(String::new()),
        }
    }

    /// Barter: move what was put up and the caps it comes to, if the paying side can pay.
    fn accept(&mut self, world: &mut World<'_>, out: &mut Applied) -> Result<String, SimError> {
        let Some(t) = self.trade.as_mut() else { return Ok(String::new()) };
        if t.is_empty() {
            return Ok("Nothing to trade".into());
        }
        let (player, merchant) = (Side::Player as usize, Side::Container as usize);
        if t.balance < 0 && (t.purse[player] as i64) < -t.balance {
            return Ok("You don't have enough caps.".into());
        }
        if t.balance > 0 && (t.purse[merchant] as i64) < t.balance {
            return Ok(format!("{} doesn't have enough caps.", self.title));
        }
        for (&item, &n) in &t.offered[player] {
            world.transfer(PLAYER, t.stock, item, n)?;
        }
        for (&item, &n) in &t.offered[merchant] {
            world.transfer(t.stock, PLAYER, item, n)?;
        }
        let paid = t.balance;
        match paid {
            p if p > 0 => world.transfer(t.stock, PLAYER, t.caps, p as i32)?,
            p if p < 0 => world.transfer(PLAYER, t.stock, t.caps, (-p) as i32)?,
            _ => 0,
        };
        t.offered = [BTreeMap::new(), BTreeMap::new()];
        out.changed = true;
        Ok(match paid {
            p if p > 0 => format!("Traded: received {p} caps"),
            p if p < 0 => format!("Traded: paid {} caps", -p),
            _ => "Traded".into(),
        })
    }

    /// Key hints for the footer, for the current mode, one line each.
    pub fn bindings(&self) -> [&'static str; 2] {
        match self.mode {
            Mode::Container(_) => [
                "Up/Down select   Left/Right column   Enter/Space move one   Shift+Enter move stack   R take all",
                "Click a row to select it, again to move one   Wheel scroll   E/Esc/Tab close   * equipped   Q quest item",
            ],
            Mode::Barter(_) => [
                "Up/Down select   Left/Right column   Enter/Space put up or take back one   Shift+Enter the stack   A accept",
                "Click a row to put it up or take it back   [ ] categories   Wheel scroll   X/Esc/Tab exit",
            ],
            Mode::Inventory => [
                "Up/Down select   Enter/Space equip/unequip (aid: use)   X drop one   Shift+X drop stack",
                "Click a row to select it, again to equip   Wheel scroll   Tab/Esc/E close   * equipped   Q quest item",
            ],
        }
    }
}

/// Test aid (F10): the container reference nearest to `pos` (game space) that is in the
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
