//! JIP LN `SetOnFastTravelEventHandler` (`fasttravel`, Part of #2272;
//! docs/spec-notes/jip-events.md §2, §5) on a synthetic plugin:
//! firing on player fast travel, plain vs linked arrival marker reference semantics
//! (including the GECK wiki `GetLinkedChildren` pattern), engine `note_fast_travel`
//! destination matching, blocked/refused fast travel not firing, handler removal,
//! `GetPCFastTravelled` interaction, newest-first order, and under-arity skip.

use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const MAP_MARKER_BASE: u32 = 0x10;
const ARRIVAL_STAT: u32 = 0x800;
const WRLD: u32 = 0x900;
const PCELL: u32 = 0x901;
const EXT: u32 = 0x902;
const PLAIN_ROOM: u32 = 0x903;
const INVERTED_ROOM: u32 = 0x904;
const PLAIN_MARKER: u32 = 0x910;
const LINKED_MARKER: u32 = 0x911;
const ARRIVAL_MARKER: u32 = 0x912;
const UNDISCOVERED_MARKER: u32 = 0x913;
const NOT_A_MARKER: u32 = 0x914;
const AUTHORED_VIS_TRAVEL_MARKER: u32 = 0x915;
const HIDDEN_MARKER: u32 = 0x916;
const TRAVEL_HITS: u32 = 0xA00;
const TRAVEL_UDF: u32 = 0xB00;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn pos_rot(pos: [f32; 3], rot: [f32; 3]) -> Vec<u8> {
    [f32b(pos[0]), f32b(pos[1]), f32b(pos[2]), f32b(rot[0]), f32b(rot[1]), f32b(rot[2])].concat()
}

fn glob(id: u32, edid: &str) -> Vec<u8> {
    record(b"GLOB", 0, id, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())])
}

fn udf(id: u32, edid: &str, source: &str) -> Vec<u8> {
    let mut fields = vec![sub(b"EDID", z(edid))];
    fields.extend(unit(0, &script(&[(0, None, Vec::new())]), &[], &[]));
    fields.push(sub(b"SCTX", source.as_bytes().to_vec()));
    record(b"SCPT", 0, id, &fields)
}

fn map_marker(id: u32, edid: &str, name: &str, flags: u8, linked: Option<u32>, pos: [f32; 3]) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"NAME", MAP_MARKER_BASE.to_le_bytes().to_vec()),
        sub(b"XMRK", vec![]),
        sub(b"FNAM", vec![flags]),
        sub(b"FULL", z(name)),
        sub(b"TNAM", vec![5, 0]),
    ];
    if let Some(l) = linked {
        subs.push(sub(b"XLKR", l.to_le_bytes().to_vec()));
    }
    subs.push(sub(b"DATA", pos_rot(pos, [0.0; 3])));
    record(b"REFR", 0x400, id, &subs)
}

fn fixture() -> Fixture {
    let globs = glob(TRAVEL_HITS, "TravelHits");
    let scpts = udf(
        TRAVEL_UDF,
        "FastTravelUdf",
        "scn FastTravelUdf\nref rMarker\nstring_var sMarker\nBegin Function {rMarker}\n  set TravelHits to TravelHits + 1\nEnd\n",
    );
    let stats = [
        record(b"STAT", 0, MAP_MARKER_BASE, &[sub(b"EDID", z("MapMarker"))]),
        record(b"STAT", 0, ARRIVAL_STAT, &[sub(b"EDID", z("TestArrivalHeading"))]),
    ]
    .concat();
    let npcs = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);

    let wrld = record(b"WRLD", 0, WRLD, &[sub(b"EDID", z("TestWorld"))]);
    let pcell = record(b"CELL", 0, PCELL, &[sub(b"EDID", z("")), sub(b"DATA", vec![0])]);
    let persistent_refs = [
        record(
            b"ACHR",
            0x400,
            FormId::PLAYER_REF.0,
            &[sub(b"EDID", z("PlayerREF")), sub(b"NAME", 0x7u32.to_le_bytes().to_vec()), sub(b"DATA", pos_rot([0.0, 0.0, 0.0], [0.0; 3]))],
        ),
        map_marker(PLAIN_MARKER, "PlainMarkerRef", "Plain Camp", 1 | 2, None, [1000.0, 500.0, 0.0]),
        map_marker(LINKED_MARKER, "LinkedMarkerRef", "Linked Town", 1 | 2, Some(ARRIVAL_MARKER), [2000.0, 500.0, 0.0]),
        record(
            b"REFR",
            0x400,
            ARRIVAL_MARKER,
            &[sub(b"EDID", z("ArrivalMarkerRef")), sub(b"NAME", ARRIVAL_STAT.to_le_bytes().to_vec()), sub(b"DATA", pos_rot([2150.0, 620.0, 12.0], [0.0, 0.0, 1.5]))],
        ),
        map_marker(UNDISCOVERED_MARKER, "UndiscoveredMarkerRef", "Hidden Cave", 2, None, [3000.0, 500.0, 0.0]),
        map_marker(AUTHORED_VIS_TRAVEL_MARKER, "AuthoredVisTravelRef", "Unvisited Fort", 1 | 2, None, [3200.0, 500.0, 0.0]),
        map_marker(HIDDEN_MARKER, "HiddenFlagRef", "Secret Bunker", 4, None, [3400.0, 500.0, 0.0]),
        record(
            b"REFR",
            0x400,
            NOT_A_MARKER,
            &[sub(b"EDID", z("NotAMarkerRef")), sub(b"NAME", ARRIVAL_STAT.to_le_bytes().to_vec()), sub(b"DATA", pos_rot([3500.0, 500.0, 0.0], [0.0; 3]))],
        ),
    ]
    .concat();
    let pcell_children = group(PCELL.to_le_bytes(), 6, &group(PCELL.to_le_bytes(), 8, &persistent_refs));
    let ext = record(b"CELL", 0, EXT, &[sub(b"EDID", z("TestExterior")), sub(b"DATA", vec![0]), sub(b"XCLC", u32s(&[0, 0]))]);
    let world_children = group(WRLD.to_le_bytes(), 1, &[pcell, pcell_children, group(0i32.to_le_bytes(), 4, &group(0i32.to_le_bytes(), 5, &ext))].concat());

    // Interiors (DATA bit 0), one with Invert Fast Travel Behavior (bit 2).
    let rooms = [
        record(b"CELL", 0, PLAIN_ROOM, &[sub(b"EDID", z("PlainRoom")), sub(b"DATA", vec![1])]),
        record(b"CELL", 0, INVERTED_ROOM, &[sub(b"EDID", z("InvertedRoom")), sub(b"DATA", vec![1 | (1 << 2)])]),
    ]
    .concat();

    let bytes = [
        tes4(),
        top(b"GLOB", &globs),
        top(b"SCPT", &scpts),
        top(b"STAT", &stats),
        top(b"NPC_", &npcs),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &rooms))),
        top(b"WRLD", &[wrld, world_children].concat()),
    ]
    .concat();
    let plugins = vec![Plugin::parse(&bytes, "JipFastTravel.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    assert!(data.source_units.iter().all(|u| u.error.is_none()), "UDF source compile errors: {:?}", data.source_units);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let mut world = World::new(&f.forms, &f.refs, SimConfig::default());
    if let Ok(p) = world.get(FormId::PLAYER_REF) {
        p.cell = Some(FormId(EXT));
        p.pos = [0.0, 0.0, 0.0];
    }
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx { self_ref: Some(FormId::PLAYER_REF), ..Default::default() })
        .unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    match q(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

fn aux_size(g: &mut Game, key: &str) -> f64 {
    n(g, &format!("PlayerREF.AuxVarSize \"{key}\""))
}

fn aux_flt(g: &mut Game, key: &str, idx: u32) -> f64 {
    n(g, &format!("PlayerREF.AuxVarGetFlt \"{key}\" {idx}"))
}

fn aux_ref(g: &mut Game, key: &str, idx: u32) -> FormId {
    match q(g, &format!("SetFunctionValue (PlayerREF.AuxVarGetRef \"{key}\" {idx})")) {
        XVal::Ref(r) => FormId(r),
        v => panic!("AuxVarGetRef {key}[{idx}]: {v:?}"),
    }
}

fn global(g: &Game, id: u32) -> f32 {
    g.world.global(FormId(id)).unwrap()
}

#[test]
fn fast_travel_event_fires_with_plain_and_linked_markers_and_updates_get_pc_fast_travelled() {
    let f = fixture();
    let mut g = game(&f);
    g.set_found_markers([FormId(PLAIN_MARKER), FormId(LINKED_MARKER)]);

    // Register both a named SCPT UDF (for removal testing) and an inline handler
    // implementing the GECK wiki's MapMarker / GetLinkedChildren resolution pattern.
    q(
        &mut g,
        "SetOnFastTravelEventHandler FastTravelUdf 1\n\
         SetOnFastTravelEventHandler (Begin Function {ref rMarker}\n\
           PlayerREF.AuxVarSetRef \"*ft_raw\" rMarker -1\n\
           ref rReal\n\
           if rMarker.GetBaseObject != MapMarker\n\
             array_var aLinks\n\
             let aLinks := rMarker.GetLinkedChildren\n\
             if (ar_Size aLinks) > 0\n\
               let rReal := aLinks[0]\n\
             endif\n\
           else\n\
             let rReal := rMarker\n\
           endif\n\
           PlayerREF.AuxVarSetRef \"*ft_real\" rReal -1\n\
           PlayerREF.AuxVarSetFlt \"*ft_seen\" (GetPCFastTravelled) -1\n\
         End) 1",
    );

    assert_eq!(n(&mut g, "GetPCFastTravelled"), 0.0, "no fast travel yet");

    // 1) Plain (unlinked) MapMarker: handler receives PlainMarkerRef itself.
    assert!(g.fast_travel(FormId(PLAIN_MARKER)));
    assert_eq!(aux_size(&mut g, "*ft_raw"), 1.0);
    assert_eq!(aux_ref(&mut g, "*ft_raw", 0), FormId(PLAIN_MARKER));
    assert_eq!(aux_ref(&mut g, "*ft_real", 0), FormId(PLAIN_MARKER));
    assert_eq!(aux_flt(&mut g, "*ft_seen", 0), 1.0, "GetPCFastTravelled reads 1 inside handler");
    assert_eq!(global(&g, TRAVEL_HITS), 1.0);
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 1.0, "calling script sees the fast travel once");
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 0.0, "repeated ask resets to 0");

    // 2) Linked MapMarker (XLKR -> ArrivalMarkerRef): handler receives ArrivalMarkerRef,
    //    and GetLinkedChildren resolves back to LinkedMarkerRef (GECK wiki example).
    assert!(g.fast_travel(FormId(LINKED_MARKER)));
    assert_eq!(aux_size(&mut g, "*ft_raw"), 2.0);
    assert_eq!(aux_ref(&mut g, "*ft_raw", 1), FormId(ARRIVAL_MARKER));
    assert_eq!(aux_ref(&mut g, "*ft_real", 1), FormId(LINKED_MARKER));
    assert_eq!(aux_flt(&mut g, "*ft_seen", 1), 1.0);
    assert_eq!(global(&g, TRAVEL_HITS), 2.0);
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 1.0);
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 0.0);

    // 3) Engine depart() boundary: move player to ArrivalMarkerRef in exterior grid cell
    //    (while marker lives in the worldspace persistent cell) and call zero-arg note_fast_travel().
    if let Ok(p) = g.world.get(FormId::PLAYER_REF) {
        (p.cell, p.pos, p.rot) = (Some(FormId(EXT)), [2150.0, 620.0, 12.0], [0.0, 0.0, 1.5]);
    }
    g.note_fast_travel();
    assert_eq!(aux_size(&mut g, "*ft_raw"), 3.0);
    assert_eq!(aux_ref(&mut g, "*ft_raw", 2), FormId(ARRIVAL_MARKER));
    assert_eq!(aux_ref(&mut g, "*ft_real", 2), FormId(LINKED_MARKER));
    assert_eq!(global(&g, TRAVEL_HITS), 3.0);
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 1.0);

    // 4) Removing the named handler stops FastTravelUdf while the inline handler still runs.
    q(&mut g, "SetOnFastTravelEventHandler FastTravelUdf 0");
    assert!(g.fast_travel(FormId(PLAIN_MARKER)));
    assert_eq!(aux_size(&mut g, "*ft_raw"), 4.0);
    assert_eq!(global(&g, TRAVEL_HITS), 3.0, "removed handler no longer runs");
}

#[test]
fn blocked_or_invalid_fast_travel_does_not_fire_event_or_increment_counter() {
    let f = fixture();
    let mut g = game(&f);
    g.set_found_markers([FormId(PLAIN_MARKER)]);

    q(
        &mut g,
        "SetOnFastTravelEventHandler FastTravelUdf 1\n\
         SetOnFastTravelEventHandler (Begin Function {ref rMarker}\n\
           PlayerREF.AuxVarSetRef \"*ft_b\" rMarker -1\n\
         End) 1",
    );

    // Undiscovered markers authored with Can Travel To (FNAM = 2) or Visible | Can Travel To
    // (FNAM = 1 | 2) are refused; ShowMap with canTravel=0 still refuses.
    assert!(!g.fast_travel(FormId(UNDISCOVERED_MARKER)), "authored FNAM=2 undiscovered marker is refused");
    assert!(!g.fast_travel(FormId(AUTHORED_VIS_TRAVEL_MARKER)), "authored FNAM=1|2 undiscovered marker is refused");
    q(&mut g, "ShowMap UndiscoveredMarkerRef 0");
    assert!(!g.fast_travel(FormId(UNDISCOVERED_MARKER)));

    // ShowAllMapMarkers 1 1 1 (skipHidden = 1) must still refuse a MARKER_HIDDEN (FNAM = 4) marker.
    q(&mut g, "ShowAllMapMarkers 1 1 1");
    assert!(!g.fast_travel(FormId(HIDDEN_MARKER)), "MARKER_HIDDEN marker is refused under ShowAllMapMarkers 1 1 1");
    q(&mut g, "ShowAllMapMarkers 0");

    // Non-map-marker and nonexistent FormID: refused.
    assert!(!g.fast_travel(FormId(NOT_A_MARKER)));
    assert!(!g.fast_travel(FormId(0x9999)));

    // EnableFastTravel 0 (script block): refused even for a discovered marker.
    q(&mut g, "EnableFastTravel 0");
    assert!(!g.fast_travel(FormId(PLAIN_MARKER)));
    q(&mut g, "EnableFastTravel 1");

    // An interior, and a worldspace flagged Can't Fast-Travel: refused.
    if let Ok(p) = g.world.get(FormId::PLAYER_REF) {
        p.cell = Some(FormId(PLAIN_ROOM));
    }
    assert!(!g.fast_travel(FormId(PLAIN_MARKER)));
    if let Ok(p) = g.world.get(FormId::PLAYER_REF) {
        p.cell = Some(FormId(EXT));
    }
    q(&mut g, "SetWorldspaceFlag TestWorld 1 1");
    assert!(!g.fast_travel(FormId(PLAIN_MARKER)));
    q(&mut g, "SetWorldspaceFlag TestWorld 1 0");

    // None of the refused attempts fired the event or marked GetPCFastTravelled.
    assert_eq!(aux_size(&mut g, "*ft_b"), 0.0);
    assert_eq!(global(&g, TRAVEL_HITS), 0.0);
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 0.0);

    // Moving back outdoors and enabling travel via ShowMap 1 lets travel succeed and fire once.
    if let Ok(p) = g.world.get(FormId::PLAYER_REF) {
        p.cell = Some(FormId(EXT));
    }
    q(&mut g, "ShowMap UndiscoveredMarkerRef 1");
    assert!(g.fast_travel(FormId(UNDISCOVERED_MARKER)));
    assert_eq!(aux_size(&mut g, "*ft_b"), 1.0);
    assert_eq!(aux_ref(&mut g, "*ft_b", 0), FormId(UNDISCOVERED_MARKER));
    assert_eq!(global(&g, TRAVEL_HITS), 1.0);
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 1.0);

    // ShowAllMapMarkers 1 1 0 (skipHidden = 0) allows travelling to the hidden marker too.
    q(&mut g, "ShowAllMapMarkers 1 1 0");
    assert!(g.fast_travel(FormId(HIDDEN_MARKER)));
    assert_eq!(aux_size(&mut g, "*ft_b"), 2.0);
    assert_eq!(aux_ref(&mut g, "*ft_b", 1), FormId(HIDDEN_MARKER));
    assert_eq!(global(&g, TRAVEL_HITS), 2.0);
}

#[test]
fn fast_travel_event_respects_newest_first_and_under_arity_skip() {
    let f = fixture();
    let mut g = game(&f);
    g.set_found_markers([FormId(PLAIN_MARKER)]);

    q(
        &mut g,
        "SetOnFastTravelEventHandler (Begin Function {ref rMarker}\n\
           PlayerREF.AuxVarSetFlt \"*ord\" 1 -1\n\
         End) 1\n\
         SetOnFastTravelEventHandler (Begin Function {ref rMarker}\n\
           PlayerREF.AuxVarSetFlt \"*ord\" 2 -1\n\
         End) 1\n\
         SetOnFastTravelEventHandler (Begin Function {}\n\
           PlayerREF.AuxVarSetFlt \"*under\" 1 -1\n\
         End) 1",
    );

    assert!(g.fast_travel(FormId(PLAIN_MARKER)));
    assert_eq!(aux_size(&mut g, "*ord"), 2.0);
    assert_eq!((aux_flt(&mut g, "*ord", 0), aux_flt(&mut g, "*ord", 1)), (2.0, 1.0), "newest-first JIP handler order");
    assert_eq!(aux_size(&mut g, "*under"), 0.0, "0-parameter handler is skipped when event passes 1 argument");
}

/// #3434: an interior forbids fast travel, EnableFastTravel 1 or not [documented: GECK wiki
/// EnableFastTravel]; CELL DATA bit 2 turns round what the cell's kind does [documented:
/// GetCellFlag, "Invert Fast Travel Behavior"], as SetCellFlag leaves it too.
#[test]
fn an_interior_forbids_fast_travel_unless_its_cell_inverts_it() {
    let f = fixture();
    let mut g = game(&f);
    let put = |g: &mut Game, cell: u32| g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(cell));
    let can = |g: &mut Game| n(g, "GetPCCanFastTravel");
    assert_eq!(can(&mut g), 1.0, "outdoors");
    put(&mut g, PLAIN_ROOM);
    assert_eq!(can(&mut g), 0.0, "an interior");
    q(&mut g, "EnableFastTravel 1 1 1");
    assert_eq!(can(&mut g), 0.0, "EnableFastTravel 1 does not lift it");
    q(&mut g, "SetCellFlag PlainRoom 2 1");
    assert_eq!(can(&mut g), 1.0, "the flag set by a script");
    q(&mut g, "SetCellFlag PlainRoom 2 0");
    q(&mut g, "SetCellFlag PlainRoom 7 1");
    assert_eq!(can(&mut g), 1.0, "an interior that behaves like an exterior");
    put(&mut g, INVERTED_ROOM);
    assert_eq!(can(&mut g), 1.0, "the flag in the data");
    q(&mut g, "EnableFastTravel 0");
    assert_eq!(can(&mut g), 0.0, "a script's block still holds there");
    q(&mut g, "EnableFastTravel 1");
    put(&mut g, EXT);
    assert_eq!(can(&mut g), 1.0);
    q(&mut g, "SetCellFlag TestExterior 2 1");
    assert_eq!(can(&mut g), 0.0, "the flag on an exterior forbids it");
}

/// The player's Fast Travel From Anywhere (bAllowUnsafeFastTravel) lifts the place's refusal
/// for GetPCCanFastTravel and a scripted fast travel, as on the map; a script's block holds.
#[test]
fn the_unsafe_travel_tweak_lifts_the_place_for_scripts_too() {
    let f = fixture();
    let mut g = game(&f);
    g.set_found_markers([FormId(PLAIN_MARKER)]);
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(PLAIN_ROOM));
    assert_eq!(n(&mut g, "GetPCCanFastTravel"), 0.0);
    assert!(!g.fast_travel(FormId(PLAIN_MARKER)));
    g.tweaks.unsafe_fast_travel = true;
    assert_eq!(n(&mut g, "GetPCCanFastTravel"), 1.0);
    q(&mut g, "EnableFastTravel 0");
    assert!(!g.fast_travel(FormId(PLAIN_MARKER)), "a script's block holds");
    q(&mut g, "EnableFastTravel 1");
    assert!(g.fast_travel(FormId(PLAIN_MARKER)));
    assert_eq!(n(&mut g, "GetPCFastTravelled"), 1.0);
    // A worldspace flagged against it is a place too.
    q(&mut g, "SetWorldspaceFlag TestWorld 1 1");
    assert_eq!(n(&mut g, "GetPCCanFastTravel"), 1.0);
    g.tweaks.unsafe_fast_travel = false;
    assert_eq!(n(&mut g, "GetPCCanFastTravel"), 0.0);
}
