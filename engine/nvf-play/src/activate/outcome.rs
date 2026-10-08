//! What activating a target will do (the prompt) and what an activation's outcome does
//! to the game (messages, travel, menus) — pure, testable without Bevy's app.
//!
//! Prompt and message wording is NVForever's own; it is not a claim about the original's
//! interface text.

use fnv_sim::world::{Activation, Outcome};
use fnv_sim::FormId;
use fnv_world::WorldIndex;

/// Lock level meaning "requires a key" [hypothesis, simulation.md §5].
pub const KEY_ONLY_LEVEL: u8 = 255;

/// What E would do to the target, for the prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Pick up an item stack.
    Take { count: i32 },
    /// Open a container, or a door that opens in place.
    Open,
    /// Close a door that is open.
    Close,
    /// A door that teleports; the destination's name when known.
    Travel { to: Option<String> },
    /// Locked, and the player carries the key.
    Unlock,
    /// Locked, no key.
    Locked { level: u8 },
    Talk,
    /// Activators and furniture: the base form's script would run (Layer 5).
    Activate,
    /// A placed explosive (PGRE).
    Disarm,
}

pub fn lock_text(level: u8) -> String {
    if level == KEY_ONLY_LEVEL {
        "requires a key".into()
    } else {
        format!("level {level}")
    }
}

impl Action {
    pub fn label(&self) -> String {
        match self {
            Action::Take { .. } => "Take".into(),
            Action::Open => "Open".into(),
            Action::Close => "Close".into(),
            Action::Travel { to: Some(to) } => format!("Open: {to}"),
            Action::Travel { to: None } => "Open".into(),
            Action::Unlock => "Unlock (key)".into(),
            Action::Locked { level } => format!("Locked ({})", lock_text(*level)),
            Action::Talk => "Talk".into(),
            Action::Activate => "Activate".into(),
            Action::Disarm => "Disarm".into(),
        }
    }
}

/// The two prompt lines under the crosshair: the target's name, then the key and action.
pub fn prompt(name: &str, action: &Action) -> String {
    let name = match action {
        Action::Take { count } if *count > 1 => format!("{name} ({count})"),
        _ => name.to_string(),
    };
    format!("{name}\nE) {}", action.label())
}

/// Where a door's destination cell is, from the world index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Interior(u32),
    /// An exterior cell (or a persistent cell) of this worldspace.
    Exterior { world: u32 },
    /// No cell, or a cell the index does not know.
    Unknown,
}

/// Interior cells are those without a worldspace (`fnv_world::Cell::world`), as
/// nvforever's `--cell` decides it.
pub fn destination(index: &WorldIndex, cell: Option<FormId>) -> Destination {
    match cell.and_then(|c| index.cells.get(&c.0)) {
        Some(c) => match c.world {
            None => Destination::Interior(c.form_id),
            Some(world) => Destination::Exterior { world },
        },
        None => Destination::Unknown,
    }
}

/// Compass heading in degrees (0 north, 90 east, in [0, 360)) on arrival through a door,
/// from the XTEL rotation's z angle: references store radians, and a z rotation turns
/// clockwise seen from above (docs/spec-notes/world.md §4 [observed]), so a z of 0 faces
/// north (+Y) and the angle in degrees is the compass heading [hypothesis: that the
/// actor faces the model's +Y; not captured for teleports].
pub fn heading_deg(rot: [f32; 3]) -> f32 {
    rot[2].to_degrees().rem_euclid(360.0)
}

/// What the game must do after an activation.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// The reference left (or came back into) the world: visuals and collision follow
    /// (`crate::RefChanged`).
    RefChanged(u32),
    /// `crate::Travel`: into an interior (`interior`), or out into the worldspace `world`
    /// (the one being played or another).
    Travel { world: Option<u32>, interior: Option<u32>, feet: [f32; 3], heading_deg: f32 },
    OpenContainer(u32),
    /// A short on-screen message.
    Message(String),
}

/// The effects of activating `target` (whose name is `name`). `name_of` names other
/// forms (items picked up, keys).
pub fn effects(a: &Activation, target: u32, name: &str, index: &WorldIndex, name_of: impl Fn(FormId) -> String) -> Vec<Effect> {
    let mut out = Vec::new();
    if let Some(k) = a.unlocked_with {
        out.push(Effect::Message(format!("Unlocked with {}", name_of(k))));
    }
    let nothing = || Effect::Message(format!("{name}: nothing happens"));
    match &a.outcome {
        Outcome::PickedUp { item, count, .. } => {
            out.push(Effect::RefChanged(target));
            let n = name_of(*item);
            out.push(Effect::Message(if *count > 1 { format!("{n} ({count}) added") } else { format!("{n} added") }));
        }
        Outcome::Disarmed { item } => {
            out.push(Effect::RefChanged(target));
            out.push(Effect::Message(format!("{} added", name_of(*item))));
        }
        Outcome::OpenedContainer { container } => out.push(Effect::OpenContainer(container.0)),
        Outcome::Locked { level } => out.push(Effect::Message(format!("{name} is locked ({})", lock_text(*level)))),
        Outcome::Teleported { cell, pos, rot, .. } => {
            // Outdoors the destination always names its worldspace; whether that is a
            // change of worldspace is for the receivers to see (`crate::Travel::destination`).
            let (world, interior) = match destination(index, *cell) {
                Destination::Interior(c) => (None, Some(c)),
                Destination::Exterior { world } => (Some(world), None),
                Destination::Unknown => {
                    out.push(Effect::Message(format!("{name}: the destination cell is unknown")));
                    return out;
                }
            };
            out.push(Effect::Travel { world, interior, feet: *pos, heading_deg: heading_deg(*rot) });
        }
        Outcome::DoorToggled { open } => out.push(Effect::Message(format!("{name} {}", if *open { "opened" } else { "closed" }))),
        Outcome::Talk { .. } => out.push(Effect::Message(format!("{name}: dialogue is not implemented yet"))),
        Outcome::Activated if a.script.is_some() => {
            out.push(Effect::Message(format!("{name}: its script would run (scripts are not implemented yet)")))
        }
        Outcome::Activated | Outcome::Disabled | Outcome::NoDefault { .. } => out.push(nothing()),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::Cell;

    const WORLD: u32 = 0xDA726; // any FormID: the synthetic worldspace
    const OTHER_WORLD: u32 = 0x1_0000;
    const INTERIOR: u32 = 0x100;
    const EXTERIOR: u32 = 0x200;
    const ELSEWHERE: u32 = 0x300;

    fn index() -> WorldIndex {
        let mut ix = WorldIndex::default();
        for (id, world) in [(INTERIOR, None), (EXTERIOR, Some(WORLD)), (ELSEWHERE, Some(OTHER_WORLD))] {
            ix.cells.insert(id, Cell { form_id: id, world, ..Default::default() });
        }
        ix
    }

    fn act(outcome: Outcome) -> Activation {
        Activation { outcome, unlocked_with: None, script: None }
    }

    fn names(id: FormId) -> String {
        format!("form {:X}", id.0)
    }

    fn run(a: &Activation) -> Vec<Effect> {
        effects(a, 0x55, "Thing", &index(), names)
    }

    #[test]
    fn interior_detection() {
        let ix = index();
        assert_eq!(destination(&ix, Some(FormId(INTERIOR))), Destination::Interior(INTERIOR));
        assert_eq!(destination(&ix, Some(FormId(EXTERIOR))), Destination::Exterior { world: WORLD });
        assert_eq!(destination(&ix, Some(FormId(0x999))), Destination::Unknown);
        assert_eq!(destination(&ix, None), Destination::Unknown);
    }

    #[test]
    fn pick_up_hides_the_reference() {
        let e = run(&act(Outcome::PickedUp { item: FormId(0x15169), count: 3, owner: None }));
        assert_eq!(e, vec![Effect::RefChanged(0x55), Effect::Message("form 15169 (3) added".into())]);
        let e = run(&act(Outcome::Disarmed { item: FormId(0x77) }));
        assert_eq!(e[0], Effect::RefChanged(0x55));
    }

    #[test]
    fn container_opens_the_menu() {
        let mut a = act(Outcome::OpenedContainer { container: FormId(0x55) });
        assert_eq!(run(&a), vec![Effect::OpenContainer(0x55)]);
        a.unlocked_with = Some(FormId(0x42));
        assert_eq!(run(&a), vec![Effect::Message("Unlocked with form 42".into()), Effect::OpenContainer(0x55)]);
    }

    #[test]
    fn doors_travel_into_interiors_and_out() {
        let half_turn = [0.0, 0.0, std::f32::consts::PI];
        let a = act(Outcome::Teleported { to_door: FormId(1), cell: Some(FormId(INTERIOR)), pos: [1.0, 2.0, 3.0], rot: half_turn });
        assert_eq!(run(&a), vec![Effect::Travel { world: None, interior: Some(INTERIOR), feet: [1.0, 2.0, 3.0], heading_deg: 180.0 }]);
        let a = act(Outcome::Teleported { to_door: FormId(1), cell: Some(FormId(EXTERIOR)), pos: [4.0, 5.0, 6.0], rot: [0.1, 0.2, -std::f32::consts::FRAC_PI_2] });
        assert_eq!(run(&a), vec![Effect::Travel { world: Some(WORLD), interior: None, feet: [4.0, 5.0, 6.0], heading_deg: 270.0 }]);
        // An unknown cell: no travel, a message.
        for cell in [Some(FormId(0x999)), None] {
            let e = run(&act(Outcome::Teleported { to_door: FormId(1), cell, pos: [0.0; 3], rot: [0.0; 3] }));
            assert!(matches!(e.as_slice(), [Effect::Message(_)]), "{cell:?}: {e:?}");
        }
    }

    #[test]
    fn doors_travel_into_another_worldspace() {
        // A gate whose destination cell belongs to another worldspace (the Mojave's north
        // gate into FreesideNorthWorld is such a door in FalloutNV.esm).
        let a = act(Outcome::Teleported { to_door: FormId(2), cell: Some(FormId(ELSEWHERE)), pos: [8288.0, 6896.0, 1008.0], rot: [0.0; 3] });
        assert_eq!(
            run(&a),
            vec![Effect::Travel { world: Some(OTHER_WORLD), interior: None, feet: [8288.0, 6896.0, 1008.0], heading_deg: 0.0 }]
        );
        let mut unlocked = a.clone();
        unlocked.unlocked_with = Some(FormId(0x42));
        let e = run(&unlocked);
        assert_eq!(e[0], Effect::Message("Unlocked with form 42".into()));
        assert!(matches!(e[1], Effect::Travel { world: Some(OTHER_WORLD), interior: None, .. }));
    }

    #[test]
    fn everything_else_is_a_message() {
        for o in [
            Outcome::Locked { level: 50 },
            Outcome::DoorToggled { open: true },
            Outcome::Talk { actor: FormId(9) },
            Outcome::Activated,
            Outcome::Disabled,
            Outcome::NoDefault { sig: "LVLI".into() },
        ] {
            let e = run(&act(o.clone()));
            assert!(matches!(e.as_slice(), [Effect::Message(_)]), "{o:?}: {e:?}");
        }
        assert_eq!(run(&act(Outcome::Locked { level: 255 })), vec![Effect::Message("Thing is locked (requires a key)".into())]);
    }

    #[test]
    fn prompts() {
        assert_eq!(prompt("Stimpak", &Action::Take { count: 3 }), "Stimpak (3)\nE) Take");
        assert_eq!(prompt("Stimpak", &Action::Take { count: 1 }), "Stimpak\nE) Take");
        assert_eq!(prompt("Door", &Action::Travel { to: Some("Goodsprings".into()) }), "Door\nE) Open: Goodsprings");
        assert_eq!(prompt("Safe", &Action::Locked { level: 50 }), "Safe\nE) Locked (level 50)");
        assert_eq!(Action::Locked { level: 255 }.label(), "Locked (requires a key)");
    }

    #[test]
    fn headings_wrap() {
        assert_eq!(heading_deg([0.0; 3]), 0.0);
        assert!((heading_deg([0.0, 0.0, 7.0]) - (7f32.to_degrees() - 360.0)).abs() < 1e-3);
    }
}
