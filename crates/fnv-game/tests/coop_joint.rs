//! Joint progression in co-op (`fnv_game::coop_joint`; shared-progression plan §3): the
//! host's stage hook and presence, the guest's per-stage check, the claim ledger and its
//! replay, on a synthetic plugin built in memory (no game data).

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::coop_joint::{check, ClaimLedger, JointGuest, StageOffer, TurnedDown};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload]
        .concat()
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

fn catalogue() -> (FunctionTable, serde_json::Value) {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    (FunctionTable::load(&p).unwrap(), j)
}

const SIDE: u32 = 0x900;
const MAIN: u32 = 0x904;
/// Old World Blues' implant quest (the Auto-Doc's Implant GRX): each player's own.
const IMPLANT: u32 = 0x905;
const GUEST: u32 = 0xFF00_0801;

fn quest(fid: u32, edid: &str, stages: &[i16]) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"DATA", [vec![0u8, 0, 0, 0], 5.0f32.to_le_bytes().to_vec()].concat())];
    for s in stages {
        subs.push(sub(b"INDX", s.to_le_bytes().to_vec()));
        subs.push(sub(b"QSDT", vec![0]));
    }
    record(b"QUST", 0, fid, &subs)
}

fn plugin() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x1000u32.to_le_bytes()].concat();
    let tes4 = record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)]);
    // A side quest and a base-game main-story quest (its editor ID is on the list, and
    // this plugin is load-order index 0, as FalloutNV.esm is).
    [tes4, top(b"QUST", &[quest(SIDE, "VMS24", &[1, 10, 20, 50, 100]), quest(MAIN, "VMQ01", &[10, 20]), quest(IMPLANT, "NVDLC03GenericPerkQUEST", &[10, 11])].concat())].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let (table, j) = catalogue();
    let bytes = plugin();
    let plugins = vec![Plugin::parse(&bytes, "Story.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.new_game();
    g
}

fn offer(stage: i32, prior: &[i32]) -> StageOffer {
    StageOffer { quest: FormId(SIDE), stage, prior: prior.to_vec(), line: None, to: vec![FormId(GUEST)] }
}

#[test]
fn the_host_offers_side_stages_to_guests_with_joint_on_and_keeps_presence() {
    let f = fixture();
    let mut host = game(&f);
    // Nobody joined: nothing offered.
    host.set_stage(FormId(SIDE), 1);
    assert!(host.take_joint_offers().is_empty());
    host.coop_joint_guests.insert(FormId(GUEST), JointGuest { character: 77, on: true });
    host.set_stage(FormId(SIDE), 10);
    host.set_stage(FormId(MAIN), 10);
    host.set_stage(FormId(IMPLANT), 10);
    let offers = host.take_joint_offers();
    assert_eq!(offers.len(), 1, "main story and a player's own implant are never offered: {offers:?}");
    assert!(!host.coop_presence.stages.contains_key(&FormId(IMPLANT)));
    assert_eq!((offers[0].quest, offers[0].stage, offers[0].prior.clone(), offers[0].to.clone()), (FormId(SIDE), 10, vec![1], vec![FormId(GUEST)]));
    assert!(host.coop_presence.was_there(FormId(SIDE), 10, 77));
    assert!(!host.coop_presence.was_there(FormId(SIDE), 1, 77));
    assert!(!host.coop_presence.stages.contains_key(&FormId(MAIN)));
    // Joint off: no offer, no presence.
    host.coop_joint_guests.insert(FormId(GUEST), JointGuest { character: 77, on: false });
    host.set_stage(FormId(SIDE), 20);
    assert!(host.take_joint_offers().is_empty());
    assert!(!host.coop_presence.was_there(FormId(SIDE), 20, 77));
    // Presence survives a save and load.
    let json = serde_json::to_string(&host.snapshot()).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&json).unwrap();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let again = Game::restore(&f.data, world, GameConfig { strict: true, ..GameConfig::default() }, &back).unwrap();
    assert!(again.coop_presence.was_there(FormId(SIDE), 10, 77));
    // The debug console's view (`coop.presence`).
    let lines = again.coop_presence_lines(FormId(SIDE));
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].contains("stage 10") && lines[1].contains("000000000000004d"), "{lines:?}");
    assert_eq!(again.coop_presence_lines(FormId(MAIN)).len(), 1);
}

#[test]
fn continuity_prerequisites_and_own_progress_decide_each_claim() {
    let own = std::collections::BTreeMap::new();
    let mut ledger = ClaimLedger::default();
    // First stage of a quest its own game never started: prerequisites decide.
    assert_eq!(check(&own, &ledger, &offer(1, &[]), || false), Err(TurnedDown::Prerequisite));
    assert_eq!(check(&own, &ledger, &offer(1, &[]), || true), Ok(()));
    ledger.stages.entry(FormId(SIDE)).or_default().push(fnv_game::coop_joint::ClaimedStage { stage: 1, line: None });
    // Later stages: continuity only (prerequisites are not asked again).
    assert_eq!(check(&own, &ledger, &offer(10, &[1]), || false), Ok(()));
    // A stage missed (joined late, dropped) stops the later ones.
    assert_eq!(check(&own, &ledger, &offer(50, &[1, 10, 20]), || true), Err(TurnedDown::Missed(10)));
    // Its own game did the missed stage: continuity holds through it.
    let mut own2 = std::collections::BTreeMap::new();
    let mut q = fnv_game::quest::QuestState { running: true, ..Default::default() };
    q.stages_done.extend([1, 10]);
    own2.insert(FormId(SIDE), q.clone());
    assert_eq!(check(&own2, &ClaimLedger::default(), &offer(20, &[1, 10]), || false), Ok(()));
    assert_eq!(check(&own2, &ClaimLedger::default(), &offer(10, &[1]), || true), Err(TurnedDown::AlreadyDone));
    // Its own game ended the quest: nothing more.
    q.completed = true;
    own2.insert(FormId(SIDE), q);
    assert_eq!(check(&own2, &ClaimLedger::default(), &offer(20, &[1, 10]), || true), Err(TurnedDown::QuestEnded));
}

#[test]
fn a_guest_claims_into_its_character_and_its_own_world_replays_once() {
    let f = fixture();
    let mut guest = game(&f);
    let id = guest.ensure_character_id();
    assert_ne!(id, 0);
    assert_eq!(guest.ensure_character_id(), id, "the ID is stable");
    let own = guest.quests.clone();
    // No claim open yet: a guest's infamy in the host's world stays there.
    const REP: FormId = FormId(0x0000_0900);
    guest.coop_note_reputation(&[(REP, 30.0)]);
    assert!(guest.player.claims.reputation.is_empty());
    for (s, prior) in [(1, vec![]), (10, vec![1]), (20, vec![1, 10])] {
        guest.take_joint_offer(&own, &offer(s, &prior)).unwrap();
    }
    assert_eq!(guest.take_joint_offer(&own, &offer(100, &[1, 10, 20, 50])), Err(TurnedDown::Missed(50)));
    // With a claim open, its own acts' infamy goes into the ledger (plan §3.6).
    guest.coop_note_reputation(&[(REP, 30.0)]);
    assert_eq!(guest.player.claims.reputation.get(&REP), Some(&[30.0, 0.0]));
    // The debug console's view (`coop.ledger`).
    let ledger = guest.coop_ledger_lines();
    assert!(ledger[0].contains("1 quest(s) waiting") && ledger[1].contains("1, 10, 20"), "{ledger:?}");
    // A map marker found in the host's world (plan P7).
    guest.player.claims.markers.insert(FormId(0x0000_0A00));
    // The ledger travels with the character into its own save.
    let c = fnv_game::coop::Character::of(&guest.player);
    assert_eq!(c.character_id, id);
    let mut own_world = game(&f);
    c.put_on(&mut own_world.player);
    let xp = own_world.player.xp;
    assert!(own_world.player.claims.markers.contains(&FormId(0x0000_0A00)), "markers carry over for the engine's load");
    let lines = own_world.replay_claims();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let q = own_world.quests.get(&FormId(SIDE)).unwrap();
    assert!([1, 10, 20].iter().all(|s| q.stages_done.contains(s)), "{:?}", q.stages_done);
    assert!(!q.stages_done.contains(&50));
    assert_eq!(own_world.player.xp, xp);
    assert_eq!(own_world.player.reputation.get(&REP).map(|v| v[0]), Some(30.0), "the infamy is replayed");
    assert!(own_world.player.claims.reputation.is_empty());
    assert!(!own_world.coop_replaying);
    // Replayed once: a second load replays nothing, and the claims stay known.
    assert!(own_world.replay_claims().is_empty());
    assert!(own_world.player.claims.claimed(FormId(SIDE), 20));
    // A replay never offers stages on (it is not the host's world).
    assert!(own_world.take_joint_offers().is_empty());
}
