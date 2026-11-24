//! Settings of mods NVForever ships different from the mods' own (the owner's decisions;
//! the fork). Each is the starting value of a mod's global, written over the value the
//! plugin authored once the forms are read, so the mod's files stay as they are. A value
//! the player sets (the mod's MCM page, its INI, a script) is the world's own and is kept
//! in the save, so it wins over these [design].
//!
//! - JAM's sprint (JVS), "Disable Unarmed Hands" (`JVSAnimsHands`) on: no sprint clip with
//!   empty hands in first person [owner's decision 2026-11-06, after turning it on in MCM
//!   in a playtest; JAM authors it 0].
//!
//! Log line: `mod defaults: JVSAnimsHands 0 -> 1` for each one applied.

use bevy::log::info;
use fnv_sim::FormStore;

/// (global's Editor ID, the value NVForever starts it at).
pub const DEFAULTS: &[(&str, f32)] = &[("JVSAnimsHands", 1.0)];

/// Write [`DEFAULTS`] over the plugins' values for the globals the load order has.
pub fn apply(forms: &mut FormStore) {
    for &(edid, value) in DEFAULTS {
        let Some((id, was)) = forms.global_by_edid(edid) else { continue };
        if was != value {
            forms.globals.insert(id, value);
            info!("mod defaults: {edid} {was} -> {value}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mods_global_starts_at_our_default_and_an_absent_one_is_left_alone() {
        let mut forms = FormStore::default();
        let id = fnv_sim::FormId(0x0400_1234);
        forms.edids.insert("jvsanimshands".into(), id);
        forms.globals.insert(id, 0.0);
        apply(&mut forms);
        assert_eq!(forms.global_by_edid("JVSAnimsHands"), Some((id, 1.0)));
        // Without JAM nothing is added.
        let mut none = FormStore::default();
        apply(&mut none);
        assert!(none.globals.is_empty());
    }
}
