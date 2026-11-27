//! Test aid `NVF_PLAY_TEST_START_FOS=<save.fos>`: the run starts where an original game's
//! save stands (#1024): its worldspace or interior cell, the player's position and
//! heading (`fnv_save::fos`, docs/spec-notes/save-files.md §6.4). The save is only read,
//! and only its place is used: the rest of a new run is NVForever's own start. The pitch
//! is the place's x rotation too, unless `--pitch` is given: it reads 0 in nvfairport,
//! and the original's player.GetAngle X reads 0.0 right after loading that save and 60
//! frames later, its GetReticleRange 9384.3 level along heading 75 [observed: save
//! format; observer check 124, 2026-11-27, #2135]. Before, the run kept `--pitch`'s
//! default of 10° down, and the reticle met the runway 470 units off. That the x
//! rotation is the pitch, positive looking down as GetAngle X is, is [hypothesis: the
//! save gives 0, so the sign is not seen]. The
//! extender lab's engine sets start from the owner's save the original's probes ran in
//! (`nvfairport`), as `fnvtool ext queries --save` does headless.

use std::path::Path;

/// Where the save stands, in this run's load order.
#[derive(Debug, Clone, PartialEq)]
pub struct FosStart {
    /// The worldspace (outdoors) or interior cell.
    pub at: u32,
    /// The player's feet.
    pub feet: [f32; 3],
    /// Degrees, as `--heading` takes them (the rotation's z, clockwise from north).
    pub heading_deg: f32,
    /// Degrees, as `--pitch` takes them (negative looks down): the rotation's x, negated
    /// (module notes).
    pub pitch_deg: f32,
}

/// The save's place, its FormID moved from the save's plugin list to `plugins` (this
/// run's load order, by name, ignoring case).
pub fn read(path: &Path, plugins: &[String]) -> Result<FosStart, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let fos = fnv_save::fos::Fos::parse(&bytes).map_err(|e| e.0)?;
    let player = fos.player().map_err(|e| e.0)?;
    let (at, pos, rot) = player.place.ok_or("the save's player record has no place")?;
    let at = remap(at, &fos.plugins, plugins)?;
    Ok(FosStart { at, feet: pos, heading_deg: rot[2].to_degrees().rem_euclid(360.0), pitch_deg: pitch_of(rot[0]) })
}

/// `--pitch`'s degrees for a saved x rotation (radians, positive looking down).
fn pitch_of(x: f32) -> f32 {
    let d = -x.to_degrees();
    if d == 0.0 { 0.0 } else { d }
}

/// A FormID of the save's load order (`saved`) in `run`'s: the same plugin by name.
fn remap(id: u32, saved: &[String], run: &[String]) -> Result<u32, String> {
    let index = (id >> 24) as usize;
    if index == 0xFF {
        return Err(format!("{id:08X}: a form the save created"));
    }
    let name = saved.get(index).ok_or_else(|| format!("{id:08X}: plugin {index} is not in the save's list"))?;
    let to = run.iter().position(|p| p.eq_ignore_ascii_case(name)).ok_or_else(|| format!("{id:08X}: {name} is not loaded"))?;
    Ok(((to as u32) << 24) | (id & 0x00FF_FFFF))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_move_to_this_runs_load_order_by_name() {
        let saved = ["FalloutNV.esm", "DeadMoney.esm", "Mod.esp"].map(String::from);
        let run = ["FalloutNV.esm", "mod.esp"].map(String::from);
        assert_eq!(remap(0x000D_A726, &saved, &run), Ok(0x000D_A726));
        assert_eq!(remap(0x0200_1234, &saved, &run), Ok(0x0100_1234));
        assert!(remap(0x0100_1234, &saved, &run).is_err(), "DeadMoney is not loaded");
        assert!(remap(0xFF00_0001, &saved, &run).is_err());
        assert!(remap(0x0500_0001, &saved, &run).is_err());
    }

    #[test]
    fn the_saved_x_rotation_is_the_pitch_looking_down_when_positive() {
        assert_eq!(pitch_of(0.0), 0.0, "nvfairport: level, as the original reads");
        assert!((pitch_of(10f32.to_radians()) + 10.0).abs() < 1e-4);
    }
}
