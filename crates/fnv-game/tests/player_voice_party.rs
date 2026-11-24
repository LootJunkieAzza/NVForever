//! The player's voice in co-op, its reactions and its place comments
//! (docs/design/player-voices.md §4-§6, `fnv_game::player_voice`): what is picked, what is
//! sent to the other players, what is heard here, and how often. No game data: the lines
//! are carried lines made from the sets themselves.

use fnv_esm::{LoadOrder, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::player_voice::{add_packed_lines, Deed, PlayerVoice, SetLine, VoiceSet, COMMENT_AGAIN, COMMENT_EVERY, FIGHT_OVER_SECONDS, FORMAT, QUIP_GAP, REACT_EVERY};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

struct Loaded {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn line(event: &str, file: &str, first_person: bool) -> SetLine {
    SetLine {
        info: "Mod.esp|000ABC".into(),
        event: event.into(),
        first_person,
        use_line: true,
        note: String::new(),
        file: Some(file.into()),
        text: Some(format!("{event} line")),
        near: vec![],
        inside: vec![],
    }
}

fn set() -> VoiceSet {
    VoiceSet {
        format: FORMAT.into(),
        id: "nexus.t".into(),
        name: "T".into(),
        sex: "female".into(),
        race: String::new(),
        blurb: String::new(),
        speaker: String::new(),
        rate: 1.0,
        companion: None,
        lines: vec![
            line("Hit", "a.ogg", true),
            line("Attack", "b.ogg", false),
            line("react.throwgrenade", "c.ogg", true),
            line("react.fireweapon", "d.ogg", false),
            line("place", "e.ogg", true),
            line("NormalToCombat", "f.ogg", true),
            line("CombatToNormal", "g.ogg", true),
        ],
    }
}

fn load() -> Loaded {
    let lo = LoadOrder::new(&[]).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let refs = RefTable::build(&lo, &schema);
    let forms = FormStore::build(&lo, &schema);
    let mut data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    add_packed_lines(&mut data, &[set()]);
    Loaded { data, forms, refs }
}

fn game(l: &Loaded) -> Game<'_> {
    let world = World::new(&l.forms, &l.refs, SimConfig::default());
    let mut g = Game::new(&l.data, world, GameConfig::default());
    g.new_game();
    let (v, missing) = PlayerVoice::resolve(&set(), &l.data).unwrap();
    assert_eq!(missing, 0);
    g.player_voice = Some(v);
    g
}

fn info(l: &Loaded, k: usize) -> FormId {
    l.data.packed_lines[&("nexus.t".to_string(), k)]
}

#[test]
fn in_coop_every_line_is_sent_and_only_first_person_ones_are_heard_here() {
    let l = load();
    let mut g = game(&l);
    // Alone: a party-only line is never picked.
    assert_eq!(g.player_quip("Attack"), None);
    assert!(g.quip.out.is_empty());
    assert_eq!(g.player_quip("Hit"), Some(info(&l, 0)));
    assert!(g.quip.out.is_empty(), "nothing is sent without others");
    // In co-op the party-only line goes to the others and the player says its own.
    g.quips.coop = true;
    g.time += QUIP_GAP;
    assert_eq!(g.player_quip("Attack"), None);
    assert_eq!(g.quip.out, vec![info(&l, 1)]);
    g.time += QUIP_GAP;
    assert_eq!(g.player_quip("Hit"), Some(info(&l, 0)));
    assert_eq!(g.quip.out, vec![info(&l, 1), info(&l, 0)]);
    // With First Person Quips off the others still hear it; the player does not.
    g.quips.first_person = false;
    g.time += QUIP_GAP;
    assert_eq!(g.player_quip("Hit"), None);
    assert_eq!(g.quip.out.len(), 3);
    // Another player's quip plays: nobody else starts one (the group cooldown).
    g.quips.first_person = true;
    g.quip.hold_until = g.time + 5.0;
    g.time += QUIP_GAP;
    assert_eq!(g.player_quip("Hit"), None);
    assert_eq!(g.quip.out.len(), 3);
}

#[test]
fn a_deed_is_answered_at_most_every_react_every_and_gunfire_never() {
    let l = load();
    let mut g = game(&l);
    g.quips.coop = true;
    assert_eq!(g.player_reacts(Deed::ThrowGrenade, true), None, "not in a fight of its own");
    assert_eq!(g.player_reacts(Deed::ThrowGrenade, false), Some(info(&l, 2)));
    assert_eq!(g.quip.out, vec![info(&l, 2)]);
    assert_eq!(g.player_reacts(Deed::ThrowGrenade, false), None, "too soon after the last");
    g.time += REACT_EVERY;
    // Another player's gunfire is never answered: its fight is that player's state (item 106).
    assert_eq!(g.player_reacts(Deed::FireWeapon, false), None);
    assert_eq!(g.quip.skipped.last().map(|s| s.0.as_str()), Some("react.fireweapon"));
    assert_eq!(g.player_reacts(Deed::LayMine, false), None, "no line for it");
    assert_eq!(Deed::of_attack(Some(10)), Some(Deed::ThrowGrenade));
    assert_eq!(Deed::of_attack(Some(5)), Some(Deed::FireWeapon));
    assert_eq!(Deed::of_attack(Some(1)), None, "melee");
}

#[test]
fn a_place_comment_is_said_on_coming_in_once_per_comment_every() {
    let l = load();
    let mut g = game(&l);
    let place = info(&l, 4);
    let cell = FormId(0x0001_2345);
    g.player_voice.as_mut().unwrap().inside.insert(place, vec![cell]);
    g.player_voice.as_mut().unwrap().by_event.insert("place".into(), vec![place]);
    // Not there: nothing.
    assert_eq!(g.player_comments(false), None);
    // Coming in: said once.
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(cell);
    g.time += 1.0;
    assert_eq!(g.player_comments(false), Some(place));
    assert_eq!(g.quip.said.last().map(|s| s.0.as_str()), Some("place"));
    // Staying: not again.
    g.time += COMMENT_AGAIN + 1.0;
    assert_eq!(g.player_comments(false), None);
    // Leaving and coming back soon after: within COMMENT_AGAIN not again.
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(FormId(0x0001_0001));
    g.time += 1.0;
    assert_eq!(g.player_comments(false), None);
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(cell);
    g.time += 1.0;
    let said = g.quip.said_at[&place];
    assert!(g.time - said > COMMENT_EVERY);
    assert_eq!(g.player_comments(false), Some(place), "COMMENT_AGAIN has passed since it was said");
    // In a fight a new place is not commented on.
    g.world.get(FormId::PLAYER_REF).unwrap().cell = None;
    g.time += 1.0;
    g.player_comments(false);
    g.world.get(FormId::PLAYER_REF).unwrap().cell = Some(cell);
    g.time += COMMENT_AGAIN + 1.0;
    assert_eq!(g.player_comments(true), None);
}

/// A fight with the player begins: its voice says a NormalToCombat line; once nobody has
/// fought it for FIGHT_OVER_SECONDS, a CombatToNormal line (item 64). Solo, heard here.
/// No two lines of the voice within QUIP_GAP, and each trigger that said nothing is
/// logged with why.
#[test]
fn a_fight_is_remarked_on_as_it_begins_and_ends_with_a_gap_between_lines() {
    let l = load();
    let mut g = game(&l);
    assert_eq!(g.player_fight_lines(false), None, "no fight");
    assert_eq!(g.player_fight_lines(true), Some(info(&l, 5)), "the fight begins");
    assert_eq!(g.quip.said.last().map(|s| s.0.as_str()), Some("fight start"));
    assert_eq!(g.player_fight_lines(true), None, "still fighting");
    // A Hit line right after is held back by the gap, and the reason is logged.
    g.time += 1.0;
    assert_eq!(g.player_quip("Hit"), None);
    assert_eq!(g.quip.skipped.last().map(|s| s.1.as_str()), Some("too soon after the last line"));
    g.time += QUIP_GAP;
    assert_eq!(g.player_quip("Hit"), Some(info(&l, 0)));
    // Nobody fights it any more: not over at once, over after FIGHT_OVER_SECONDS.
    g.time += QUIP_GAP;
    assert_eq!(g.player_fight_lines(false), None);
    g.time += FIGHT_OVER_SECONDS;
    assert_eq!(g.player_fight_lines(false), Some(info(&l, 6)), "the fight is over");
    assert_eq!(g.player_fight_lines(false), None, "only once");
    // A set without the lines says nothing, and says why.
    g.player_voice.as_mut().unwrap().by_event.remove("NormalToCombat");
    g.time += QUIP_GAP;
    assert_eq!(g.player_fight_lines(true), None);
    assert_eq!(g.quip.skipped.last().map(|s| (s.0.as_str(), s.1.as_str())), Some(("fight start", "the set has no first-person line for it")));
}

/// In co-op the player's own grunt (no voice-set line for the event) goes to the others
/// with the voice type it is said in; when a set line went out for the event, that is
/// what the others hear and the grunt stays here (one line per event, item 106). Solo
/// nothing is sent.
#[test]
fn in_coop_the_players_own_grunt_is_sent_once() {
    let l = load();
    let mut g = game(&l);
    let grunt = FormId(0x0004_1E64);
    // Solo: nothing goes out.
    g.share_own_line(grunt, g.quip.out.len());
    assert!(g.quip.own_out.is_empty());
    g.quips.coop = true;
    // No set line went out for the event: the grunt does.
    g.share_own_line(grunt, g.quip.out.len());
    assert_eq!(g.quip.own_out.len(), 1);
    assert_eq!(g.quip.own_out[0].0, grunt);
    // A set line went out for the event (a party-only Attack line): the grunt stays here.
    let sent = g.quip.out.len();
    assert_eq!(g.player_quip("Attack"), None);
    assert_eq!(g.quip.out.len(), sent + 1);
    g.share_own_line(grunt, sent);
    assert_eq!(g.quip.own_out.len(), 1);
    // A set line said here is never sent again as the player's own.
    g.time += QUIP_GAP;
    let sent = g.quip.out.len();
    let hit = g.player_quip("Hit").unwrap();
    g.share_own_line(hit, sent);
    assert_eq!(g.quip.own_out.len(), 1);
}
