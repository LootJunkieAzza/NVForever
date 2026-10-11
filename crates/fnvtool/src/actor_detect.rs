//! `fnvtool actor detect`: the detection numbers of one detector and one target in the
//! authored game (docs/design/detection.md §4): the settings the load order gives, what
//! the two actors' records and actor values put into the formula, and for a set of
//! situations the value at several distances and the farthest distance at which the
//! target is detected and noticed (fnv.actor.detect/1).

use fnv_esm::{LoadOrder, Schema};
use fnv_game::detection::{Band, Settings, Situation};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::{json, Value};

/// The distances the table lists, game units.
const DISTANCES: [f64; 9] = [100.0, 250.0, 500.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 5000.0];
/// The step of the search for the farthest distance in a band.
const STEP: f64 = 10.0;

pub struct Options {
    pub detector: String,
    pub target: String,
    pub interior: bool,
    /// The light level of a lit and of a dark place, 0–100.
    pub light: f64,
    pub dark: f64,
}

pub fn run(lo: &LoadOrder, schema: &Schema, data: &GameData, refs: &RefTable, o: &Options) -> anyhow::Result<Value> {
    let forms = FormStore::build(lo, schema);
    let mut g = Game::new(data, World::new(&forms, refs, SimConfig::default()), GameConfig::default());
    let detector = crate::actor_factions::resolve(&g, refs, &o.detector)?;
    let target = crate::actor_factions::resolve(&g, refs, &o.target)?;
    let settings = Settings::read(&g);
    let pair = g.detection_pair(detector, target);
    let exterior = !o.interior;
    let open = Situation { exterior, line_of_sight: true, in_view: true, light: o.light, ..Default::default() };
    let cases: Vec<(&str, Situation)> = vec![
        ("standing, lit", open),
        ("walking, lit", Situation { moving: true, ..open }),
        ("running, lit", Situation { running: true, ..open }),
        ("standing, dark", Situation { light: o.dark, ..open }),
        ("walking, dark", Situation { moving: true, light: o.dark, ..open }),
        ("sneaking, lit", Situation { sneaking: true, ..open }),
        ("sneaking and walking, lit", Situation { sneaking: true, moving: true, ..open }),
        ("sneaking, dark", Situation { sneaking: true, light: o.dark, ..open }),
        ("sneaking and walking, dark", Situation { sneaking: true, moving: true, light: o.dark, ..open }),
        ("behind the detector, walking", Situation { in_view: false, moving: true, ..open }),
        ("behind a wall, walking", Situation { line_of_sight: false, moving: true, ..open }),
        ("sneaking, firing a normal gun, lit", Situation { sneaking: true, action_sound: settings.sound_normal, ..open }),
        ("sneaking behind a wall, firing a silenced gun", Situation { sneaking: true, line_of_sight: false, action_sound: settings.sound_silent, ..open }),
    ];
    let reach = settings.max_distance(exterior);
    let farthest = |s: &Situation, band: Band| -> Option<f64> {
        let mut best = None;
        let mut d = 0.0;
        while d <= reach {
            if settings.evaluate(&pair, &Situation { distance: d, ..*s }).band >= band {
                best = Some(d);
            }
            d += STEP;
        }
        best
    };
    let rows: Vec<Value> = cases
        .iter()
        .map(|(name, s)| {
            let at: serde_json::Map<String, Value> = DISTANCES
                .iter()
                .map(|&d| {
                    let r = settings.evaluate(&pair, &Situation { distance: d, ..*s });
                    (format!("{d:.0}"), json!(format!("{:.1} {}", r.value, r.band.name())))
                })
                .collect();
            json!({
                "situation": name,
                "detected_out_to": farthest(s, Band::Detected),
                "noticed_out_to": farthest(s, Band::Noticed),
                "at_distance": at,
            })
        })
        .collect();
    Ok(json!({
        "schema": "fnv.actor.detect/1",
        "state": "the authored game: no quest started, no script run, nothing equipped by the player",
        "detector": data.name(detector),
        "target": data.name(target),
        "space": if exterior { "exterior" } else { "interior" },
        "reach": reach,
        "light": { "lit": o.light, "dark": o.dark },
        "thresholds": { "noticed_above": settings.noticed_min, "detected_above": 0.0 },
        "settings": format!("{settings:?}"),
        "inputs": format!("{pair:?}"),
        "situations": rows,
    }))
}
