//! Lighting the base game's cold campfires (docs/design/campfires.md) [design: NVForever's
//! own, not in the original].
//!
//! - **A campfire** ([`Game::is_campfire`]): a reference whose base is an activator whose
//!   script names the recipe category [`CATEGORY`] among its references (SCRO). The
//!   category is found by its Editor ID in the load order, so a campfire another plugin
//!   makes from those scripts counts too. In FalloutNV.esm these are `CampfireCrafting01`
//!   to `04` and `VCG03CampfireCrafting03`, with the scripts
//!   `CraftingCampfireRecipesScript` and `VCG03CraftingCampfireRecipesScript` [documented:
//!   FalloutNV.esm records]. Their models are logs, tyres and cinder blocks: no flames,
//!   no particles, no light [documented: the same records].
//! - **Burning** ([`Game::campfire_burns`]): an enabled, not deleted reference (placed or
//!   created) stands in the campfire's cell within [`FIRE_NEAR`] units of it, whose base
//!   is a light (LIGH), or a moveable static (MSTT) whose model path contains "fire". Of
//!   the 110 campfires placed in FalloutNV.esm, 44 have such a fire within 150 units, the
//!   other 66 stand cold [documented: FalloutNV.esm records].
//! - **Lighting** ([`Game::light_campfire`]): a cold campfire gets one [`FIRE`] and one
//!   [`LIGHT`] placed at it, as PlaceAtMe places them ([`Game::create_at`]). They are
//!   created references, saved with the game like any PlaceAtMe's, so a lit campfire stays
//!   lit; nothing puts it out [design]. The original has no way to light a campfire
//!   [documented: FalloutNV.esm records].
//! - The settings row "Light campfires" ([`Game::light_campfires`], off by default) keeps
//!   the original's campfires while it is off: [`Game::light_campfire`] then does nothing.

use crate::game::Game;
use fnv_sim::FormId;

/// The recipe category the campfires' scripts open (FULL "Campfire")
/// [documented: FalloutNV.esm records].
pub const CATEGORY: &str = "CampfireRecipes";
/// The fire placed at a cold campfire: the moveable static whose model
/// (`Effects\Ambient\FXFireSmall01.NIF`) draws its flames from an add-on node
/// [documented: FalloutNV.esm records].
pub const FIRE: &str = "FXFireSmall01";
/// The light placed with it: orange, radius 200, flickering [documented: FalloutNV.esm records].
pub const LIGHT: &str = "FireBrightOrange";
/// A fire this near a campfire (game units, in its cell) is the campfire's own [design: a round figure
/// for "beside it"; not measured against the base game's placements].
pub const FIRE_NEAR: f32 = 150.0;

impl Game<'_> {
    /// Whether reference `r` is a campfire: its base is an activator whose script names
    /// the recipe category [`CATEGORY`] among its references.
    pub fn is_campfire(&self, r: FormId) -> bool {
        crate::functions::base_of(self, r).is_some_and(|base| self.is_campfire_base(base))
    }

    /// Whether `base` is a campfire's base: an activator whose script names the recipe
    /// category [`CATEGORY`] among its references. Reads the load order alone (no
    /// reference state), so the engine may ask it for the base under the crosshair.
    pub fn is_campfire_base(&self, base: FormId) -> bool {
        let Some(category) = self.data.by_edid(CATEGORY).filter(|&c| self.data.sig(c) == Some(*b"RCCT")) else { return false };
        if self.data.sig(base) != Some(*b"ACTI") {
            return false;
        }
        let Some(unit) = self.data.script_of_base(base).and_then(|s| self.data.script(s)) else { return false };
        unit.unit.refs.iter().any(|slot| matches!(slot, fnv_script::unit::RefSlot::Form(x) if FormId(*x) == category))
    }

    /// Whether a fire burns at reference `r`: an enabled, not deleted light, or moveable
    /// static whose model path contains "fire", in its cell within [`FIRE_NEAR`] units
    /// (module notes). False for a reference not in a cell.
    pub fn campfire_burns(&self, r: FormId) -> bool {
        let (Some(cell), Some(at)) = (self.cell_of(r), self.ref_pos(r)) else { return false };
        let placed = self.world.placed.by_cell.get(&cell).into_iter().flatten().copied();
        let created = self.world.created().filter(|(_, s)| s.cell == Some(cell)).map(|(id, _)| id);
        placed.chain(created).any(|other| {
            other != r
                && self.cell_of(other) == Some(cell)
                && self.present(other)
                && self.ref_pos(other).is_some_and(|p| dist(p, at) <= FIRE_NEAR)
                && self.is_fire(other)
        })
    }

    /// Lights campfire `r` when it is cold and the switch is on: one [`FIRE`] and one
    /// [`LIGHT`] placed at its position, in its cell, with its angles, as PlaceAtMe places
    /// them. Returns the new references, the fire first; `None` (and nothing placed) while
    /// the switch is off, when `r` is no campfire, when it burns already, or when either
    /// form is not in the load order.
    pub fn light_campfire(&mut self, r: FormId) -> Option<Vec<FormId>> {
        if !self.light_campfires || !self.is_campfire(r) || self.campfire_burns(r) {
            return None;
        }
        let fire = self.data.by_edid(FIRE).filter(|&f| self.data.sig(f) == Some(*b"MSTT"))?;
        let light = self.data.by_edid(LIGHT).filter(|&f| self.data.sig(f) == Some(*b"LIGH"))?;
        let s = self.world.get(r).ok()?;
        let (cell, pos, rot) = (s.cell, s.pos, s.rot);
        let mut placed = Vec::with_capacity(2);
        // The light first: a fire with no light would count as burning for good, a light
        // with no fire is the lesser fault. If the fire then fails, the light is taken
        // back and nothing is lit, so the campfire stays as it was.
        for base in [light, fire] {
            match self.create_at(base, cell, pos, rot) {
                Ok(made) => placed.push(made),
                Err(e) => {
                    tracing::warn!("campfire {r}: {} not placed: {e}", self.data.name(base));
                    for made in placed {
                        let _ = self.world.set_deleted(made, true);
                    }
                    return None;
                }
            }
        }
        placed.reverse();
        tracing::debug!("campfire {r} lit: {} placed", placed.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", "));
        Some(placed)
    }

    /// Where reference `r` stands now, without materialising it.
    fn ref_pos(&self, r: FormId) -> Option<[f32; 3]> {
        match self.world.peek(r) {
            Some(s) => Some(s.pos),
            None => self.world.placed.get(r).map(|p| p.pos),
        }
    }

    /// Whether reference `r`'s base is a fire by the rule of the module notes.
    fn is_fire(&self, r: FormId) -> bool {
        let Some(base) = crate::functions::base_of(self, r) else { return false };
        match self.data.sig(base) {
            Some(s) if &s == b"LIGH" => true,
            Some(s) if &s == b"MSTT" => crate::nif_blocks::model_of(self, base).is_some_and(|m| m.to_ascii_lowercase().contains("fire")),
            _ => false,
        }
    }
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt()
}
