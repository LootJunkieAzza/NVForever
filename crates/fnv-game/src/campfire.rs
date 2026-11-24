//! Campfires lit by Burning Campfire Redone (Nexus 63623, `Burning Campfire.esp`), and
//! the second action NVForever gives them: light the fire without the recipe menu
//! (owner's queue item 54, 2026-11-06).
//!
//! **The plugin** [observed: its records, read 2026-11-06]: it overrides the vanilla
//! campfire scripts `CraftingCampfireRecipesScript` and
//! `VCG03CraftingCampfireRecipesScript`. On activation, a campfire whose base is in the
//! form list `SAGValidCampfiresList` `PlaceAtMe`s one `SAGFireMovableStaticCampfireStage01`
//! and four `SAGFireMovableStaticCampfireStageNoSound` (activators drawing
//! `effects\fxfiremeshsmall.nif`), then opens `ShowRecipeMenu CampfireRecipes` as
//! before. Each fire's script, on load, places a light `SAGCampFire420` into its
//! `MyLight` (the first also `PlaySound3D FXFireBarrelLP`), counts `Timer` up by
//! `GetSecondsPassed` and, past its `Required` (41-60 s for the first, 25-40 s for the
//! others), disables and deletes its light and itself. Every activation places five
//! more fires: activating a lit campfire stacks flames.
//!
//! **NVForever** [design, owner's queue item 54]:
//! - [`Game::light_campfire`] is the second action: it places the same five fires at the
//!   campfire (no menu), or, when the campfire burns already, starts the fires' timers
//!   again instead of placing more. It lights the list's campfires and every activator
//!   whose script names the plugin's fire, such as Goodsprings' `VCG03CampfireCrafting03`,
//!   which the plugin's own activation leaves unlit (off its list).
//! - [`Game::campfire_fires`] before the plugin's own activation (Craft) and
//!   [`Game::keep_first_fires`] after it: when the campfire burned before, the fires that
//!   activation placed are removed with their lights and the burning ones start again.
//! - Co-op (`nvforever::coop::campfires`): a guest keeps the fires of a campfire burning in
//!   the host's game ([`Game::hold_campfire`]) and puts them out with the host's
//!   ([`Game::put_out_campfire`]; [`Game::campfire_full`] says whether the silent ones
//!   still burn).
//! - Without the plugin's forms nothing here applies ([`Game::campfire_forms`] is `None`).

use crate::game::Game;
use crate::trace::EventKind;
use fnv_sim::FormId;

/// The form list of the campfires the plugin lights.
pub const VALID_LIST: &str = "SAGValidCampfiresList";
/// The fire that plays the crackle.
pub const FIRE_SOUND: &str = "SAGFireMovableStaticCampfireStage01";
/// The silent fires, four of them.
pub const FIRE_QUIET: &str = "SAGFireMovableStaticCampfireStageNoSound";
/// The quiet fires placed beside the one with the sound.
pub const QUIET_COUNT: usize = 4;
/// A fire stands within this many units of its campfire (PlaceAtMe puts it exactly at the
/// campfire, `functions::objects::place_at_me`).
const AT_CAMPFIRE: f32 = 1.0;

/// The plugin's forms in the load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CampfireForms {
    pub list: FormId,
    pub fire: FormId,
    pub quiet: FormId,
}

/// What [`Game::light_campfire`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    /// The fires were placed (the new references).
    Placed(Vec<FormId>),
    /// The campfire burned already: these fires start their time again.
    Restarted(Vec<FormId>),
}

impl Game<'_> {
    /// The plugin's forms, when it is loaded.
    pub fn campfire_forms(&self) -> Option<CampfireForms> {
        Some(CampfireForms { list: self.data.by_edid(VALID_LIST)?, fire: self.data.by_edid(FIRE_SOUND)?, quiet: self.data.by_edid(FIRE_QUIET)? })
    }

    /// The campfire bases the second action lights: those in the plugin's list, then the
    /// activators whose script names the plugin's fire ([`Game::script_places_fire`]); in
    /// FormID order after the list's. Empty without the plugin.
    pub fn campfire_bases(&self) -> Vec<FormId> {
        let Some(f) = self.campfire_forms() else { return Vec::new() };
        let mut out = crate::functions::refs::list_members(self, f.list);
        let mut more: Vec<FormId> = self.data.form_script.keys().copied().filter(|&b| !out.contains(&b) && self.script_places_fire(b, f)).collect();
        more.sort();
        out.extend(more);
        out
    }

    /// Whether reference `r` is a campfire the second action lights: its base is in the
    /// list (authored or added at run time), as the plugin's `IsInList` asks, or its base's
    /// script names the plugin's fire.
    pub fn is_campfire(&self, r: FormId) -> bool {
        let Some(f) = self.campfire_forms() else { return false };
        let Some(base) = crate::functions::base_of(self, r) else { return false };
        crate::functions::refs::list_members(self, f.list).contains(&base) || self.script_places_fire(base, f)
    }

    /// Whether activator `base` runs a script that names the plugin's fire among its
    /// references (SCRO): a campfire script the plugin edited to `PlaceAtMe` its fires.
    /// The plugin edits `VCG03CraftingCampfireRecipesScript` that way but leaves its
    /// campfire (`VCG03CampfireCrafting03`, Goodsprings) out of its list, so its own
    /// activation places no fire there [observed: the plugin's records, 2026-11-13]; the
    /// second action, NVForever's own, lights it all the same [design, owner's report
    /// "you broke fires", 2026-11-13].
    fn script_places_fire(&self, base: FormId, f: CampfireForms) -> bool {
        if base == f.fire || base == f.quiet || self.data.sig(base) != Some(*b"ACTI") {
            return false;
        }
        let Some(unit) = self.data.script_of_base(base).and_then(|s| self.data.script(s)) else { return false };
        unit.unit.refs.iter().any(|r| matches!(r, fnv_script::unit::RefSlot::Form(x) if FormId(*x) == f.fire))
    }

    /// The fires burning at campfire `r`: created references of the two fire bases at its
    /// position, in its cell, enabled and not deleted; in FormID order.
    pub fn campfire_fires(&self, r: FormId) -> Vec<FormId> {
        let Some(f) = self.campfire_forms() else { return Vec::new() };
        let Some(at) = self.world.peek(r) else { return Vec::new() };
        let (cell, pos) = (at.cell, at.pos);
        self.world
            .created()
            .filter(|(_, s)| (s.base == f.fire || s.base == f.quiet) && s.enabled() && !s.deleted() && s.cell == cell)
            .filter(|(_, s)| (0..3).map(|k| (s.pos[k] - pos[k]).powi(2)).sum::<f32>().sqrt() <= AT_CAMPFIRE)
            .map(|(id, _)| id)
            .collect()
    }

    /// The second action on campfire `r`: its fires start again when it burns, else the
    /// plugin's five fires are placed at it as its activation places them.
    pub fn light_campfire(&mut self, r: FormId) -> Result<Lit, String> {
        let f = self.campfire_forms().ok_or("Burning Campfire Redone is not loaded")?;
        if !self.is_campfire(r) {
            return Err(format!("{} is not a campfire in {VALID_LIST}", self.data.name(r)));
        }
        let burning = self.campfire_fires(r);
        if !burning.is_empty() {
            self.restart_fires(&burning);
            return Ok(Lit::Restarted(burning));
        }
        let s = self.world.get(r).map_err(|e| e.to_string())?;
        let (cell, pos, rot) = (s.cell, s.pos, s.rot);
        let mut placed = Vec::with_capacity(1 + QUIET_COUNT);
        for base in std::iter::once(f.fire).chain(std::iter::repeat_n(f.quiet, QUIET_COUNT)) {
            placed.push(self.create_at(base, cell, pos, rot)?);
        }
        Ok(Lit::Placed(placed))
    }

    /// After the plugin's own activation of campfire `r`: when it burned `before` it, the
    /// fires the activation added are removed (with any light they placed) and those of
    /// `before` start again. Returns the fires removed.
    pub fn keep_first_fires(&mut self, r: FormId, before: &[FormId]) -> Vec<FormId> {
        if before.is_empty() {
            return Vec::new();
        }
        let added: Vec<FormId> = self.campfire_fires(r).into_iter().filter(|f| !before.contains(f)).collect();
        for &fire in &added {
            self.remove_fire(fire);
        }
        self.restart_fires(before);
        added
    }

    /// Whether campfire `r` still has a silent fire burning beside the one with the sound
    /// (the silent ones burn out first).
    pub fn campfire_full(&self, r: FormId) -> bool {
        let Some(f) = self.campfire_forms() else { return false };
        self.campfire_fires(r).iter().any(|&fire| self.world.peek(fire).is_some_and(|s| s.base == f.quiet))
    }

    /// Co-op: campfire `r` burns in the host's game, so its fires here keep burning (their
    /// timers start again) until the host's go out. Returns the fires held.
    pub fn hold_campfire(&mut self, r: FormId) -> usize {
        let fires = self.campfire_fires(r);
        self.restart_fires(&fires);
        fires.len()
    }

    /// Co-op: the fires at campfire `r` go out here, with the lights they placed, as their
    /// own script ends them: all of them, or (`quiet_only`) the silent ones. Returns how
    /// many went out.
    pub fn put_out_campfire(&mut self, r: FormId, quiet_only: bool) -> usize {
        let Some(f) = self.campfire_forms() else { return 0 };
        let fires: Vec<FormId> = self.campfire_fires(r).into_iter().filter(|&fire| !quiet_only || self.world.peek(fire).is_some_and(|s| s.base == f.quiet)).collect();
        for &fire in &fires {
            self.remove_fire(fire);
        }
        fires.len()
    }

    /// Each fire's `Timer` back to 0: it burns its whole time again.
    fn restart_fires(&mut self, fires: &[FormId]) {
        for &fire in fires {
            let Some((key, script)) = self.instance_of(fire) else { continue };
            let Some(index) = self.data.script(script).and_then(|s| s.var_index("Timer")) else { continue };
            self.store(key, Some(script), index, fnv_script::vm::Value::Num(0.0), "campfire", false);
        }
    }

    /// A fire and the light it placed (its `MyLight`) taken away.
    fn remove_fire(&mut self, fire: FormId) {
        if let Some(light) = self.ref_var_named(fire, "MyLight").map(|v| FormId(v as u32)).filter(|l| l.0 != 0) {
            self.remove_placed(light);
        }
        self.remove_placed(fire);
    }

    /// Disabled and deleted, as the fire's own script ends it.
    fn remove_placed(&mut self, r: FormId) {
        if self.world.peek(r).is_some_and(|s| s.enabled()) && self.world.set_enabled(r, false).is_ok() {
            self.emit(EventKind::RefDisabled { reference: r, fade: false });
        }
        self.extra.entry(r).or_default().marked_for_delete = true;
        let _ = self.world.set_deleted(r, true);
    }
}
