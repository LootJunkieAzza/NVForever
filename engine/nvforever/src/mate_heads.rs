//! Where the other players' heads are, for their name tags (`nvf_play::world_marks`): the
//! head bone of each other player's puppet in this player's place, every frame, whatever
//! the body does: standing, in an emote, seated on furniture (a chair, a slot machine's
//! stool), at an arm-wrestling table. The tag stands over the drawn head, so it stays on
//! the body when the body is not where the player's reported feet are (a seated puppet is
//! moved onto the furniture's marker) [design: the parity session's ship6 check, a slot
//! player's tag left over the floor].
//!
//! A player whose puppet has no head bone yet (not drawn, still loading) is left out: its
//! tag stands over its feet as before (`world_marks::HEAD_HEIGHT`).

use bevy::prelude::*;
use nvf_play::coop_social::SocialMates;
use nvf_play::emotes::MateHeads;
use std::collections::HashMap;

use crate::limbs::ActorRig;

pub struct MateHeadsPlugin;

impl Plugin for MateHeadsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, track);
        if std::env::var_os("NVF_PLAY_TEST_MARKS_LOG").is_some() {
            app.add_systems(Update, trace_bodies);
        }
    }
}

/// Test aid (`NVF_PLAY_TEST_MARKS_LOG`): every body drawn within 600 units of this
/// player's camera, each 5 seconds: its reference, base, place, and whether it is a
/// player's puppet or a host's NPC, to tell what a body with no name tag is.
#[allow(clippy::type_complexity)]
fn trace_bodies(time: Res<Time>, mut last: Local<f32>, cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>, rigs: Query<(&ActorRig, &GlobalTransform, &InheritedVisibility, Has<crate::coop::Puppet>, Has<crate::coop::Remote>)>) {
    let now = time.elapsed_secs();
    if now - *last < 5.0 {
        return;
    }
    *last = now;
    let Some((_, eye)) = cams.iter().find(|(c, _)| c.is_active) else { return };
    let eye = eye.translation();
    let mut line = String::new();
    for (rig, t, seen, puppet, remote) in &rigs {
        let at = t.translation();
        if at.distance(eye) > 600.0 {
            continue;
        }
        let g = nvf_play::space::to_game(at);
        let kind = if puppet { "puppet" } else if remote { "remote" } else { "own" };
        line += &format!(" [{:08X} base {:08X} {kind}{} at ({:.0}, {:.0}, {:.0})]", rig.reference, rig.base, if seen.get() { "" } else { " hidden" }, g[0], g[1], g[2]);
    }
    let e = nvf_play::space::to_game(eye);
    info!("bodies trace: eye ({:.0}, {:.0}, {:.0}):{line}", e[0], e[1], e[2]);
}

/// The heads of the mates in this place, by reference, from `head` (game space).
pub fn heads_of(mates: &SocialMates, head: impl Fn(u32) -> Option<[f32; 3]>) -> HashMap<u32, [f32; 3]> {
    mates.mates.iter().filter(|m| m.here && !m.lost).filter_map(|m| head(m.reference).map(|h| (m.reference, h))).collect()
}

fn track(mates: Option<Res<SocialMates>>, rigs: Query<&ActorRig>, globals: Query<&GlobalTransform>, heads: Option<ResMut<MateHeads>>, mut doubled: Local<Vec<(u32, usize)>>) {
    let (Some(mates), Some(mut heads)) = (mates, heads) else { return };
    // A player drawn twice here (two bodies under its reference): said once each time it
    // changes, as its tag can stand over only one of them.
    let twice: Vec<(u32, usize)> = mates.mates.iter().filter(|m| m.here && !m.lost).map(|m| (m.reference, rigs.iter().filter(|r| r.reference == m.reference).count())).filter(|(_, n)| *n > 1).collect();
    if *doubled != twice {
        for (r, n) in &twice {
            warn!("mate heads: {n} bodies drawn for player reference {r:08X}");
        }
        *doubled = twice;
    }
    let want = heads_of(&mates, |reference| {
        let rig = rigs.iter().find(|r| r.reference == reference)?;
        let g = nvf_play::space::to_game(crate::knockouts::bone(rig, &globals, "Bip01 Head")?);
        Some([g[0], g[1], g[2]])
    });
    if heads.0 != want {
        heads.0 = want;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvf_play::coop_social::Mate;

    fn mate(reference: u32, here: bool, lost: bool) -> Mate {
        Mate { id: 1, name: "Host".into(), reference, here, feet: [100.0, 200.0, 0.0], yaw: 0.0, down: false, body: None, lost, weapon: 0, attacks: 0, sights: false, pitch: 0.0 }
    }

    #[test]
    fn a_seated_mate_s_tag_follows_its_head_not_its_feet() {
        // Seated on a stool: the puppet's head is 50 units from its reported feet.
        let mates = SocialMates { mates: vec![mate(0xFE00_C000, true, false)], ..Default::default() };
        let heads = heads_of(&mates, |_| Some([150.0, 200.0, 95.0]));
        assert_eq!(heads.get(&0xFE00_C000), Some(&[150.0, 200.0, 95.0]));
    }

    #[test]
    fn every_mate_here_gets_its_head_not_only_one_in_an_emote() {
        let mates = SocialMates { mates: vec![mate(1, true, false), mate(2, true, false), mate(3, false, false), mate(4, true, true)], ..Default::default() };
        let heads = heads_of(&mates, |r| Some([r as f32, 0.0, 120.0]));
        let mut keys: Vec<u32> = heads.keys().copied().collect();
        keys.sort();
        // Elsewhere (3) and lost (4) have no tag to place.
        assert_eq!(keys, vec![1, 2]);
    }

    #[test]
    fn a_puppet_not_drawn_yet_is_left_to_its_feet() {
        let mates = SocialMates { mates: vec![mate(1, true, false)], ..Default::default() };
        assert!(heads_of(&mates, |_| None).is_empty());
    }
}
