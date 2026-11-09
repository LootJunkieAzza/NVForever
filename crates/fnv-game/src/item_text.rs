//! The text an item card shows of what an item does (docs/spec-notes/pipboy-items.md):
//! the actor values' short names and the changes' line, for the Pip-Boy (`nvf-play`
//! pipboy) and GetItemEffectString (`crate::nvse_jg9`).

use crate::data::{MagicEffectDef, ARCHETYPE_VALUE_MODIFIER};
use crate::game::Game;
use fnv_sim::FormId;

/// The AVIF record of an actor value code in `FalloutNV.esm` [observed: the masters'
/// AVIF FormIDs follow the codes in five runs: 0–4 from 0x514, 5–11 from 0x3E8, 12–31
/// from 0x44C, 32–45 from 0x4B0, 46–76 from 0x5DC (`AVStrength` 0x3E8, `AVBarter`
/// 0x4B0, `AVDamageThreshold` 0x5FA)].
pub fn avif_form(code: u16) -> Option<u32> {
    let c = code as u32;
    Some(match code {
        0..=4 => 0x514 + c,
        5..=11 => 0x3E8 + c - 5,
        12..=31 => 0x44C + c - 12,
        32..=45 => 0x4B0 + c - 32,
        46..=76 => 0x5DC + c - 46,
        _ => return None,
    })
}

/// A whole number as the page writes it.
pub fn whole(v: f64) -> String {
    format!("{}", v.round() as i64)
}

/// One change an effect makes: the value's short name, the amount, and whether the base
/// effect is detrimental (MGEF flag "Detrimental").
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub value: String,
    pub amount: f64,
    pub detrimental: bool,
}

/// The changes of one source, as the EFF list writes them: "Speech +2, Melee Weap. +2"
/// [observed: the owner's EFF screenshot of the Vault Jumpsuit, whose enchantment adds 2
/// to Speech and 2 to Melee Weapons; the short names are the AVIF records' ANAM;
/// hypothesis: a detrimental effect is written with a minus].
pub fn changes_text(changes: &[Change]) -> String {
    changes
        .iter()
        .map(|c| {
            let sign = if c.detrimental { '-' } else { '+' };
            format!("{} {sign}{}", c.value, whole(c.amount.abs()))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl Game<'_> {
    /// An actor value's short name (its AVIF's ANAM), else its name (FULL), else the
    /// rules' name for it.
    pub fn av_short_name(&self, code: u16) -> String {
        let text = |sub: &str| {
            let id = FormId(avif_form(code)?);
            let v = self.data.record_field(id, sub, "")?;
            v.as_str().or_else(|| v.get("value").and_then(|x| x.as_str())).map(str::to_string).filter(|t| !t.is_empty())
        };
        text("ANAM").or_else(|| text("FULL")).unwrap_or_else(|| crate::functions::AV_NAMES.get(code as usize).map_or_else(String::new, |n| n.to_string()))
    }

    /// The changes of an enchantment or other magic item: each effect whose base effect
    /// is a Value Modifier, its magnitude and actor value, and whether it is detrimental
    /// [hypothesis, as the Pip-Boy's EFF list reads them].
    pub fn magic_changes(&self, item: FormId) -> Vec<Change> {
        let Some(list) = self.data.effect_items.get(&item) else { return Vec::new() };
        list.iter()
            .filter_map(|e| {
                let m = self.data.magic_effects.get(&e.effect)?;
                let code = (m.archetype == ARCHETYPE_VALUE_MODIFIER).then(|| e.actor_value.or(m.actor_value)).flatten()?;
                Some(Change { value: self.av_short_name(code), amount: f64::from(e.magnitude), detrimental: m.has(MagicEffectDef::DETRIMENTAL) })
            })
            .collect()
    }

    /// The effect line of an item's card for `actor`: an aid item's changes as taking it
    /// would make them ([`Game::aid_preview`]), else its enchantment's (apparel's EITM)
    /// [hypothesis: the Pip-Boy's EFFECTS line]; "" for an item that changes nothing.
    pub fn item_effect_text(&mut self, actor: FormId, item: FormId) -> String {
        if self.data.effect_items.contains_key(&item) {
            let lines = self.aid_preview(actor, item);
            let changes: Vec<Change> = lines.into_iter().filter(|(_, a)| a.abs() >= 0.5).map(|(code, a)| Change { value: self.av_short_name(code), amount: a.abs(), detrimental: a < 0.0 }).collect();
            if !changes.is_empty() {
                return changes_text(&changes);
            }
        }
        let ench = self.data.armor_enchantments.get(&item).copied();
        ench.map(|e| changes_text(&self.magic_changes(e))).unwrap_or_default()
    }
}
