//! Test aid `NVF_PLAY_TEST_START_FOS=<save.fos>`: the run starts where an original game's
//! save stands (#1024): its worldspace or interior cell, the player's position and
//! heading (`fnv_save::fos`, docs/spec-notes/save-files.md §6.4). The save is only read,
//! and only its place is used: the rest of a new run is NVForever's own start. Not the
//! pitch: the place's x rotation reads 0 in nvfairport [observed], where the original's
//! probes read the player's GetAngle X as 22.7 [observed: projectile proj.aim], so the
//! camera's pitch is kept elsewhere [hypothesis]; the run keeps `--pitch`. The
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
}

/// The save's place, its FormID moved from the save's plugin list to `plugins` (this
/// run's load order, by name, ignoring case).
pub fn read(path: &Path, plugins: &[String]) -> Result<FosStart, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let fos = fnv_save::fos::Fos::parse(&bytes).map_err(|e| e.0)?;
    let player = fos.player().map_err(|e| e.0)?;
    let (at, pos, rot) = player.place.ok_or("the save's player record has no place")?;
    let at = remap(at, &fos.plugins, plugins)?;
    Ok(FosStart { at, feet: pos, heading_deg: rot[2].to_degrees().rem_euclid(360.0) })
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
}
