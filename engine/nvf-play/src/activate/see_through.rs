//! Talking through bars and fences (#3233): when the crosshair ray stops on collision on
//! a see-through layer and nothing else is offered, a living actor the player may talk to
//! standing just behind it, within reach, is the target, and E only opens the
//! conversation (`crate::script::talk_by_hand`; `Targeted::through`).
//!
//! The rule [design, docs/spec-notes/simulation.md §5 "Talking through bars"]: actors
//! see through these layers [documented: GECK wiki "Collision layer": TRANSPARENT,
//! TRANSPARENT SMALL, "Actors can see through this layer type"], and so may talk through
//! them; the original's crosshair on them is not known (observer-checks.md #249). Only
//! Talk passes: items, containers, doors, bodies and pockets behind bars or a window stay
//! out of reach, as render-bounds picks still count only before the first collision hit
//! (`pick::choose`). A closed door hides what is behind it on any layer.

use super::pick::{Pick, Source};
use fnv_phys::{layers, Filter, LayerSet, Owner, PhysWorld};

/// The layers the crosshair may talk through: Transparent (3), Transparent Small (26),
/// Transparent Small Anim (28), the ones sight passes (nvforever `detection.rs`
/// `see_through`, less the Invisible Wall, which the activation ray already passes).
pub fn talk_layers() -> LayerSet {
    LayerSet::from_layers(&[layers::TRANSPARENT, layers::TRANSPARENT_SMALL, layers::TRANSPARENT_SMALL_ANIM])
}

/// Whether a collision hit is a reference's collider on one of [`talk_layers`].
pub fn is_see_through(owner: Owner) -> bool {
    matches!(owner, Owner::Ref { layer, .. } if talk_layers().contains(layer))
}

/// An actor to talk to behind see-through collision ([`actor_behind`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Behind {
    pub actor: u32,
    /// Distance from the eye to the actor's hit volume or character, game units.
    pub distance: f32,
    /// The reference whose see-through collider the view ray hit first (the bars).
    pub through: u32,
}

/// The actor the view ray meets behind see-through collision: when `first` (the
/// activation ray's first hit, cast with `filter`) is a reference's collider on one of
/// [`talk_layers`], every hit along the ray within `reach` is read in order
/// ([`first_actor_past`]). `closed_door` says whether a reference is a closed door, which
/// hides what is behind it whatever its layer.
pub fn actor_behind(world: &mut PhysWorld, eye: [f32; 3], dir: [f32; 3], reach: f32, first: &fnv_phys::RayHit, filter: &Filter, closed_door: impl Fn(u32) -> bool) -> Option<Behind> {
    let Owner::Ref { form_id: through, .. } = first.owner else { return None };
    if !is_see_through(first.owner) || closed_door(through) {
        return None;
    }
    let hits = world.queries().ray_all(eye, dir, reach, filter);
    let (actor, distance) = first_actor_past(hits.iter().map(|h| (h.owner, h.distance)), closed_door)?;
    Some(Behind { actor, distance, through })
}

/// The first actor along the hits (owner, distance; nearest first) past colliders on
/// [`talk_layers`], if that is what comes first: an actor's hit volume, or an NPC's
/// character (added under its reference's id, `PhysState::add_npc`; the player's is 0
/// and never behind bars). A closed door (`closed_door`), a wall, terrain or any other
/// collider before the actor hides it.
pub fn first_actor_past(hits: impl IntoIterator<Item = (Owner, f32)>, closed_door: impl Fn(u32) -> bool) -> Option<(u32, f32)> {
    for (owner, distance) in hits {
        match owner {
            Owner::Ref { form_id, .. } if is_see_through(owner) && !closed_door(form_id) => continue,
            Owner::Actor { form_id, .. } => return Some((form_id, distance)),
            Owner::Character { id } if id != 0 => return Some((id, distance)),
            _ => return None,
        }
    }
    None
}

/// The crosshair's pick with [`actor_behind`]'s actor: `picked` when there is one (an
/// item or anything before the bars wins); else the actor behind, within `reach`, when
/// the player may talk to it now (`talks`: its prompt is Talk), with the reference it is
/// talked to through.
pub fn talk_through(picked: Option<Pick>, behind: Option<Behind>, reach: f32, talks: impl FnOnce(u32) -> bool) -> (Option<Pick>, Option<u32>) {
    if picked.is_some() {
        return (picked, None);
    }
    match behind {
        Some(b) if b.distance <= reach && talks(b.actor) => (Some(Pick { form_id: b.actor, distance: b.distance, source: Source::Collision }), Some(b.through)),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_see_through_layers_pass() {
        let on = |layer| Owner::Ref { form_id: 1, object: 0, layer };
        assert!(is_see_through(on(layers::TRANSPARENT)));
        assert!(is_see_through(on(layers::TRANSPARENT_SMALL)));
        assert!(is_see_through(on(layers::TRANSPARENT_SMALL_ANIM)));
        assert!(!is_see_through(on(layers::STATIC)));
        assert!(!is_see_through(on(layers::ANIM_STATIC)));
        assert!(!is_see_through(Owner::Terrain { x: 0, y: 0 }));
        assert!(!is_see_through(Owner::Actor { form_id: 1, layer: layers::BIPED, part: 0 }));
    }

    #[test]
    fn an_actor_behind_is_picked_only_when_nothing_else_is_and_it_talks() {
        let behind = Some(Behind { actor: 9, distance: 120.0, through: 3 });
        let (p, via) = talk_through(None, behind, 150.0, |_| true);
        assert_eq!(p, Some(Pick { form_id: 9, distance: 120.0, source: Source::Collision }));
        assert_eq!(via, Some(3));
        // Not one to talk to (a body, a pocket, a hostile actor): nothing.
        assert_eq!(talk_through(None, behind, 150.0, |_| false), (None, None));
        // Out of reach: nothing.
        assert_eq!(talk_through(None, Some(Behind { distance: 151.0, ..behind.unwrap() }), 150.0, |_| true), (None, None));
        // Something offered before the bars wins.
        let item = Pick { form_id: 4, distance: 20.0, source: Source::Bounds };
        assert_eq!(talk_through(Some(item), behind, 150.0, |_| true), (Some(item), None));
    }

    #[test]
    fn a_closed_door_on_a_see_through_layer_hides_the_actor() {
        let bars = Owner::Ref { form_id: 1, object: 0, layer: layers::TRANSPARENT };
        let door = Owner::Ref { form_id: 2, object: 0, layer: layers::TRANSPARENT_SMALL_ANIM };
        let actor = Owner::Actor { form_id: 9, layer: layers::BIPED, part: 0 };
        let wall = Owner::Ref { form_id: 5, object: 0, layer: layers::STATIC };
        let doors = |id: u32| id == 2;
        let none = |_: u32| false;
        // Bars, then a second set of bars, then the actor.
        assert_eq!(first_actor_past([(bars, 30.0), (bars, 32.0), (actor, 100.0)], none), Some((9, 100.0)));
        // A closed door on layer 28 between: hidden; open (not a closed door): passed.
        assert_eq!(first_actor_past([(bars, 30.0), (door, 60.0), (actor, 100.0)], doors), None);
        assert_eq!(first_actor_past([(bars, 30.0), (door, 60.0), (actor, 100.0)], none), Some((9, 100.0)));
        // A wall between, or nothing at all: none.
        assert_eq!(first_actor_past([(bars, 30.0), (wall, 60.0), (actor, 100.0)], none), None);
        assert_eq!(first_actor_past([(bars, 30.0)], none), None);
        // An NPC's character counts, the player's does not.
        assert_eq!(first_actor_past([(bars, 30.0), (Owner::Character { id: 9 }, 95.0)], none), Some((9, 95.0)));
        assert_eq!(first_actor_past([(bars, 30.0), (Owner::Character { id: 0 }, 95.0)], none), None);
    }
}
