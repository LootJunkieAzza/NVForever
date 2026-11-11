//! Continuous collision detection (CCD) for ragdoll bones and gore pieces (#1798).
//!
//! What rapier 0.36 does [documented: rapier CHANGELOG v0.35.0-beta.0; rapier3d 0.36.1
//! `dynamics/ccd/sweeps.rs` `tier_allows`, `dynamics/ccd/ccd_solver.rs` `solve_continuous`,
//! `geometry/narrow_phase/pair_update.rs`]:
//! - **Plain**: every dynamic body that moves more than half its thinnest extent in a step
//!   is swept against the fixed colliders, and its pose is clamped at the first impact. That
//!   covers terrain and every static or keyframed reference, which are fixed bodies here.
//!   It is on for every dynamic body; there is nothing to set.
//! - **Bullet** (`ccd_enabled`): the body is also swept against kinematic and dynamic
//!   bodies (characters, clutter, other actors' hit volumes), but not against other
//!   bullets. It searches the whole broad phase.
//! - **Soft** (`soft_ccd_prediction`): plain, plus the body's broad-phase box is stretched
//!   along its velocity and its contacts are predicted out to that distance, so the solver
//!   sees a contact before the body reaches it. This works against any body.
//!
//! The play world's choice can be switched with [`CCD_VAR`] for comparing.

use rapier3d::prelude::RigidBodyBuilder;

/// For comparing (not a test aid): `<bones>[,<gore>]`, each `plain`, `soft`, `soft:<units>`
/// or `bullet`, read by nvf-play when it makes the play world. One value sets both.
pub const CCD_VAR: &str = "NVF_PHYS_CCD";

/// How far soft CCD looks ahead when no distance is given, game units: about a step's
/// travel at 1,800 units a second at 60 steps a second [design].
pub const SOFT_UNITS: f32 = 30.0;

/// The CCD a kind of body is built with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BodyCcd {
    /// Swept against fixed colliders only (rapier's automatic CCD).
    Plain,
    /// Plain, plus soft CCD's prediction out to this many game units.
    Soft(f32),
    /// Swept against every body but other bullets.
    Bullet,
}

/// The CCD of ragdoll bones and of gore pieces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CcdChoice {
    pub bones: BodyCcd,
    pub gore: BodyCcd,
}

/// Plain for both (until 2026-11-11 both were bullets) [design, #1798]:
/// - Safe where it matters: fast bones, severed heads and gore pieces stop at 2-unit floors
///   and walls, triangle floors and terrain at 300 to 3,000 units a second, at 60 and 152
///   steps a second, as with bullets; with CCD off they fall through [observed: tests
///   `tests/ccd.rs`].
/// - What plain gives up: sweeps against dynamic clutter and kinematic bodies. Ragdolls and
///   gore never touch characters or living actors (`RagdollHooks`), so that is a fast bone
///   or piece passing through a crate.
/// - The cost did not settle it: in a headless battle (`ccd_cost_in_a_battle`) CCD was about
///   a tenth of rapier's step whatever the choice, and the choices differed by less than the
///   run-to-run spread. Soft was a little dearer than plain in three of four setups. Plain
///   is the cheaper of the owner's two candidates; the game's battle-50 says whether it pays.
impl Default for CcdChoice {
    fn default() -> Self {
        CcdChoice { bones: BodyCcd::Plain, gore: BodyCcd::Plain }
    }
}

impl BodyCcd {
    /// `plain`, `soft`, `soft:<units>` or `bullet`.
    pub fn parse(s: &str) -> Option<BodyCcd> {
        let s = s.trim();
        match s {
            "plain" => Some(BodyCcd::Plain),
            "bullet" => Some(BodyCcd::Bullet),
            "soft" => Some(BodyCcd::Soft(SOFT_UNITS)),
            _ => {
                let units: f32 = s.strip_prefix("soft:")?.trim().parse().ok()?;
                (units > 0.0 && units.is_finite()).then_some(BodyCcd::Soft(units))
            }
        }
    }

    /// `b` with this CCD; `k` is metres per game unit.
    pub(crate) fn build(self, b: RigidBodyBuilder, k: f32) -> RigidBodyBuilder {
        match self {
            BodyCcd::Plain => b,
            BodyCcd::Soft(units) => b.soft_ccd_prediction(units * k),
            BodyCcd::Bullet => b.ccd_enabled(true),
        }
    }
}

impl std::fmt::Display for BodyCcd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyCcd::Plain => write!(f, "plain"),
            BodyCcd::Soft(units) => write!(f, "soft:{units}"),
            BodyCcd::Bullet => write!(f, "bullet"),
        }
    }
}

impl CcdChoice {
    /// [`CCD_VAR`]'s value: `<bones>[,<gore>]`.
    pub fn parse(s: &str) -> Option<CcdChoice> {
        let mut parts = s.split(',');
        let bones = BodyCcd::parse(parts.next()?)?;
        let gore = match parts.next() {
            Some(g) => BodyCcd::parse(g)?,
            None => bones,
        };
        parts.next().is_none().then_some(CcdChoice { bones, gore })
    }
}

impl crate::world::PhysWorld {
    /// The CCD ragdoll bones and gore pieces added from now on are built with.
    pub fn set_ccd(&mut self, choice: CcdChoice) {
        self.ccd = choice;
    }

    pub fn ccd(&self) -> CcdChoice {
        self.ccd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_parse() {
        assert_eq!(BodyCcd::parse("plain"), Some(BodyCcd::Plain));
        assert_eq!(BodyCcd::parse(" bullet "), Some(BodyCcd::Bullet));
        assert_eq!(BodyCcd::parse("soft"), Some(BodyCcd::Soft(SOFT_UNITS)));
        assert_eq!(BodyCcd::parse("soft:12.5"), Some(BodyCcd::Soft(12.5)));
        assert_eq!(BodyCcd::parse("soft:0"), None);
        assert_eq!(BodyCcd::parse("soft:-3"), None);
        assert_eq!(BodyCcd::parse("fast"), None);
        let both = CcdChoice::parse("plain").unwrap();
        assert_eq!(both, CcdChoice { bones: BodyCcd::Plain, gore: BodyCcd::Plain });
        let split = CcdChoice::parse("plain,soft:20").unwrap();
        assert_eq!(split, CcdChoice { bones: BodyCcd::Plain, gore: BodyCcd::Soft(20.0) });
        assert_eq!(CcdChoice::parse("plain,soft,bullet"), None);
        assert_eq!(CcdChoice::parse(""), None);
        for c in [BodyCcd::Plain, BodyCcd::Soft(7.5), BodyCcd::Bullet] {
            assert_eq!(BodyCcd::parse(&c.to_string()), Some(c));
        }
    }
}
