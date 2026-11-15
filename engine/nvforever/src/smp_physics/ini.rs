//! `NVSMPPhysics.ini` (docs/spec-notes/smp-physics.md §5): NV SMP Physics' settings,
//! read from the load order's `nvse\plugins\NVSMPPhysics.ini` when there is one. The
//! defaults are the values the plugin's own INI ships with [observed: the owner's NV SMP
//! 1.0 download, 2026-11-13]; what each setting does is its INI comment's.

/// The settings the engine uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmpSettings {
    /// `bEnabled`: off, every item follows its animated pose.
    pub enabled: bool,
    /// `fCollisionDistance`: actors farther than this keep their physics but no collision.
    pub collision_distance: f32,
    /// `iMaxActors`: most actors with collision at once, nearest first.
    pub max_actors: usize,
    /// `iMinFps`: physics steps per second at most.
    pub min_fps: f32,
    /// `iMaxSubSteps`: a frame longer than this many steps drops the rest.
    pub max_substeps: u32,
    /// `iSolverIterations`: constraint solver iterations.
    pub iterations: u32,
    /// `fGravityScale`: gravity, 1 = Earth's.
    pub gravity_scale: f32,
    /// `fResetAngle`: turning faster than this (degrees per second) starts the physics
    /// over in its rest pose, as a teleport does.
    pub reset_angle: f32,
    /// `bWind`, `fWindStrength` (the windiest weather's wind against 9.8 for gravity),
    /// `bWindInInteriors`.
    pub wind: bool,
    pub wind_strength: f32,
    pub wind_in_interiors: bool,
    /// `bCollisions`, `bGround`.
    pub collisions: bool,
    pub ground: bool,
}

impl Default for SmpSettings {
    fn default() -> Self {
        SmpSettings {
            enabled: true,
            collision_distance: 1500.0,
            max_actors: 10,
            min_fps: 100.0,
            max_substeps: 4,
            iterations: 10,
            gravity_scale: 1.0,
            reset_angle: 120.0,
            wind: true,
            wind_strength: 0.08,
            wind_in_interiors: false,
            collisions: true,
            ground: true,
        }
    }
}

/// Where the plugin keeps its INI, as a load-order path.
pub const INI_PATH: &str = "nvse\\plugins\\nvsmpphysics.ini";

impl SmpSettings {
    /// The settings an INI's text gives over the defaults. Keys match case-insensitively
    /// in any section; unknown keys and unreadable values keep the default.
    pub fn parse(text: &str) -> SmpSettings {
        let mut s = SmpSettings::default();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            let f = v.parse::<f32>().ok().filter(|x| x.is_finite());
            let b = f.map(|x| x != 0.0);
            match k.as_str() {
                "benabled" => s.enabled = b.unwrap_or(s.enabled),
                "fcollisiondistance" => s.collision_distance = f.map(|x| x.max(0.0)).unwrap_or(s.collision_distance),
                "imaxactors" => s.max_actors = f.map(|x| x.max(0.0) as usize).unwrap_or(s.max_actors),
                "iminfps" => s.min_fps = f.map(|x| x.clamp(10.0, 1000.0)).unwrap_or(s.min_fps),
                "imaxsubsteps" => s.max_substeps = f.map(|x| x.clamp(1.0, 64.0) as u32).unwrap_or(s.max_substeps),
                "isolveriterations" => s.iterations = f.map(|x| x.clamp(1.0, 100.0) as u32).unwrap_or(s.iterations),
                "fgravityscale" => s.gravity_scale = f.unwrap_or(s.gravity_scale),
                "fresetangle" => s.reset_angle = f.map(|x| x.max(1.0)).unwrap_or(s.reset_angle),
                "bwind" => s.wind = b.unwrap_or(s.wind),
                "fwindstrength" => s.wind_strength = f.unwrap_or(s.wind_strength),
                "bwindininteriors" => s.wind_in_interiors = b.unwrap_or(s.wind_in_interiors),
                "bcollisions" => s.collisions = b.unwrap_or(s.collisions),
                "bground" => s.ground = b.unwrap_or(s.ground),
                _ => {}
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_ini_reads_as_the_defaults_and_values_override() {
        let shipped = "[Main]\n; comment\nbEnabled=1\nfCollisionDistance=1500\niMaxActors=10\niMinFps=100\niMaxSubSteps=4\niSolverIterations=10\nfGravityScale=1\nfResetAngle=120\nbWind=1\nfWindStrength=0.08\nbWindInInteriors=0\n";
        assert_eq!(SmpSettings::parse(shipped), SmpSettings::default());
        let s = SmpSettings::parse("ISOLVERITERATIONS = 4 ; fewer\nfGravityScale=0.5\nbGround=0\niMinFps=junk\n");
        assert_eq!((s.iterations, s.gravity_scale, s.ground, s.min_fps), (4, 0.5, false, 100.0));
    }
}
