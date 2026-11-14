//! Typed session CSTY views; raw clone reads stay separate from AI defaults.
//! See docs/spec-notes/combat-style-session.md.
use crate::{data::CombatStyleDef, game::Game};
use fnv_sim::FormId;
use std::{borrow::Cow, collections::HashMap};

pub(crate) struct RawCombatStyle {
    pub traits: [f64; 64],
    pub flags: u64,
}

#[derive(Default)]
pub(crate) struct CombatStyleEdits {
    pub typed: HashMap<FormId, CombatStyleDef>,
    pub raw_clones: HashMap<FormId, RawCombatStyle>,
}

impl<'a> Game<'a> {
    /// Keep main's session-only save degradation; never discard saved carriers.
    /// The caller has already checked independent scene-reference save safety.
    pub(crate) fn log_combat_style_snapshot_loss(&self) {
        if !self.combat_style_edits.typed.is_empty()
            || !self.combat_style_edits.raw_clones.is_empty()
            || self.clones.keys().any(|style| self.combat_style_origin(*style).is_some())
        {
            tracing::warn!("save: combat-style edits and temporary copies are session-only; loading resets them and uses authored fallback styles");
        }
    }

    /// A declared CSTY origin, including complete finite clone chains.
    pub(crate) fn combat_style_origin(&self, mut style: FormId) -> Option<FormId> {
        for _ in 0..=self.clones.len() {
            match self.clones.get(&style) {
                Some(&original) => style = original,
                None => return self.data.combat_styles.contains_key(&style).then_some(style),
            }
        }
        None
    }

    /// The effective session view of a style form, without selecting an actor's style.
    pub(crate) fn combat_style_form(&self, style: FormId) -> Option<Cow<'a, CombatStyleDef>> {
        let original = self.combat_style_origin(style)?;
        match self.combat_style_edits.typed.get(&style) {
            Some(edited) => Some(Cow::Owned(edited.clone())),
            None => self.data.combat_styles.get(&original).map(Cow::Borrowed),
        }
    }

    /// Copy on mutation, preserving defaults of typed authored fields not written.
    pub(crate) fn edit_combat_style(&mut self, style: FormId, edit: impl FnOnce(&mut CombatStyleDef)) -> bool {
        let Some(mut value) = self.combat_style_form(style).map(Cow::into_owned) else { return false };
        edit(&mut value);
        self.combat_style_edits.typed.insert(style, value);
        true
    }

    /// Called before a new alias is installed: copy effective source values once.
    pub(crate) fn snapshot_combat_style(&mut self, source: FormId, clone: FormId) {
        let Some(value) = self.combat_style_form(source).map(Cow::into_owned) else { return };
        let raw = crate::nvse_jip4::combat_style_raw_snapshot(self, source);
        self.combat_style_edits.typed.insert(clone, value);
        self.combat_style_edits.raw_clones.insert(clone, raw);
    }
}
