//! Actor appearance on a synthetic plugin built in memory and decoded through the
//! repository schemas (no game data).

use fnv_actor::records::template;
use fnv_actor::resolve::{slot, HUMAN_SKELETON};
use fnv_actor::{ActorForms, Resolver};
use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_sim::{FormId, FormStore};

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    cat(&[sig, &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload])
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    cat(&[b"GRUP", &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], body])
}

fn tes4() -> Vec<u8> {
    let hedr = cat(&[&1.34f32.to_le_bytes(), &0i32.to_le_bytes(), &0x800u32.to_le_bytes()]);
    let payload = write_subrecords(&[sub(b"HEDR", hedr)]);
    cat(&[b"TES4", &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &0u32.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload])
}

fn fid(x: u32) -> Vec<u8> {
    x.to_le_bytes().to_vec()
}

fn floats(n: usize, v: f32) -> Vec<u8> {
    (0..n).flat_map(|_| v.to_le_bytes()).collect()
}

fn part(index: u32, model: Option<&str>, icon: Option<&str>) -> Vec<Subrecord> {
    let mut v = vec![sub(b"INDX", fid(index))];
    if let Some(m) = model {
        v.push(sub(b"MODL", z(m)));
    }
    if let Some(i) = icon {
        v.push(sub(b"ICON", z(i)));
    }
    v
}

fn acbs(flags: u32, template_flags: u16) -> Subrecord {
    let d = cat(&[
        &flags.to_le_bytes(),
        &[0; 2],
        &[0; 2],
        &1u16.to_le_bytes(),
        &[0; 2],
        &[0; 2],
        &100u16.to_le_bytes(),
        &0f32.to_le_bytes(),
        &0i16.to_le_bytes(),
        &template_flags.to_le_bytes(),
    ]);
    sub(b"ACBS", d)
}

fn armo(id: u32, edid: &str, slots: u32, dt: f32, male: &str, female: Option<&str>) -> Vec<u8> {
    let mut subs = vec![
        sub(b"EDID", z(edid)),
        sub(b"BMDT", cat(&[&slots.to_le_bytes(), &[0u8, 0, 0, 0]])),
        sub(b"MODL", z(male)),
    ];
    if let Some(f) = female {
        subs.push(sub(b"MOD3", z(f)));
    }
    subs.push(sub(b"DATA", cat(&[&10i32.to_le_bytes(), &50i32.to_le_bytes(), &1f32.to_le_bytes()])));
    subs.push(sub(b"DNAM", cat(&[&0i16.to_le_bytes(), &[0; 2], &dt.to_le_bytes(), &0u16.to_le_bytes(), &[0; 2]])));
    record(b"ARMO", id, &subs)
}

const RACE: u32 = 0x100;
const HAIR: u32 = 0x101;
const HAIR_DEFAULT: u32 = 0x102;
const EYES: u32 = 0x103;
const BEARD: u32 = 0x104;
const BROW: u32 = 0x105;
const OUTFIT: u32 = 0x110; // upper body, DT 2
const ARMOR: u32 = 0x111; // upper body, DT 12
const COWBOY_HAT: u32 = 0x112; // hair + headband + hat
const HELMET: u32 = 0x113; // head + hair + hat
const WIG: u32 = 0x114; // hair
const GLOVES: u32 = 0x115; // both hands
const BASE: u32 = 0x200; // man: outfit + armour + gloves, beard
const HATTED: u32 = 0x201; // cowboy hat
const HELMETED: u32 = 0x202;
const WIGGED: u32 = 0x203;
const WOMAN: u32 = 0x204; // female, outfit (with a female model), no hair: race default
const TEMPLATED: u32 = 0x205; // traits + model from LVLN -> BASE; own inventory (wig)
const MARKER: u32 = 0x206; // names marker_creature.nif
const LEVELED: u32 = 0x300; // LVLN -> BASE

fn plugin_bytes() -> Vec<u8> {
    let mut race = vec![
        sub(b"EDID", z("TestRace")),
        sub(b"DATA", cat(&[&[0u8; 14], &[0; 2], &1.0f32.to_le_bytes(), &0.95f32.to_le_bytes(), &1f32.to_le_bytes(), &1f32.to_le_bytes(), &0u32.to_le_bytes()])),
        sub(b"DNAM", cat(&[&fid(HAIR_DEFAULT), &fid(HAIR_DEFAULT)])),
        sub(b"NAM0", vec![]),
        sub(b"MNAM", vec![]),
    ];
    race.extend(part(0, Some("Characters\\Head\\HeadHuman.NIF"), Some("Characters\\Male\\HeadHuman.dds")));
    race.extend(part(1, None, Some("Characters\\Head\\EarsHuman.dds")));
    race.extend(part(2, Some("Characters\\Head\\MouthHuman.NIF"), None));
    race.extend(part(6, Some("Characters\\Head\\EyeLeftHuman.NIF"), None));
    race.extend(part(7, Some("Characters\\Head\\EyeRightHuman.NIF"), None));
    race.push(sub(b"FNAM", vec![]));
    race.extend(part(0, Some("Characters\\Head\\HeadFemale.NIF"), Some("Characters\\Female\\HeadHuman.dds")));
    race.push(sub(b"NAM1", vec![]));
    race.push(sub(b"MNAM", vec![]));
    race.extend(part(0, Some("characters\\_Male\\UpperBody.nif"), Some("Characters\\Male\\UpperBodyMale.dds")));
    race.extend(part(1, Some("characters\\_Male\\LeftHand.nif"), None));
    race.extend(part(2, Some("characters\\_Male\\RightHand.nif"), None));
    race.extend(part(3, Some("Characters\\_Male\\UpperBodyHumanMale.egt"), None));
    race.push(sub(b"FNAM", vec![]));
    race.extend(part(0, Some("Characters\\_Male\\FemaleUpperBody.NIF"), None));
    race.push(sub(b"HNAM", cat(&[&fid(HAIR), &fid(HAIR_DEFAULT)])));
    race.push(sub(b"ENAM", fid(EYES)));
    race.push(sub(b"MNAM", vec![]));
    race.push(sub(b"FGGS", floats(50, 0.5)));
    race.push(sub(b"FGGA", floats(30, 0.0)));
    race.push(sub(b"FGTS", floats(50, 0.25)));
    race.push(sub(b"SNAM", vec![0, 0]));
    race.push(sub(b"FNAM", vec![]));
    race.push(sub(b"FGGS", floats(50, -0.5)));
    race.push(sub(b"FGGA", floats(30, 0.0)));
    race.push(sub(b"FGTS", floats(50, 0.0)));
    race.push(sub(b"SNAM", vec![0, 0]));
    let race = record(b"RACE", RACE, &race);

    let hair = cat(&[
        &record(b"HAIR", HAIR, &[sub(b"EDID", z("HairA")), sub(b"MODL", z("Characters\\Hair\\HairA.NIF")), sub(b"ICON", z("Characters\\Hair\\HairA.dds")), sub(b"DATA", vec![1])]),
        &record(b"HAIR", HAIR_DEFAULT, &[sub(b"EDID", z("HairDefault")), sub(b"MODL", z("Characters\\Hair\\HairBase.NIF")), sub(b"ICON", z("Characters\\Hair\\HairBase.dds")), sub(b"DATA", vec![1])]),
    ]);
    let eyes = record(b"EYES", EYES, &[sub(b"EDID", z("EyeBlue")), sub(b"ICON", z("Characters\\Eyes\\EyeBlue.dds")), sub(b"DATA", vec![1])]);
    let hdpt = cat(&[
        &record(b"HDPT", BEARD, &[sub(b"EDID", z("Beard")), sub(b"MODL", z("Characters\\Hair\\Beard.nif")), sub(b"DATA", vec![1]), sub(b"HNAM", fid(BROW))]),
        &record(b"HDPT", BROW, &[sub(b"EDID", z("Brow")), sub(b"MODL", z("Characters\\Hair\\EyebrowM.nif")), sub(b"DATA", vec![1])]),
    ]);
    let armo = cat(&[
        &armo(OUTFIT, "Outfit", slot::UPPER_BODY, 2.0, "armor\\outfit.nif", Some("armor\\outfitF.nif")),
        &armo(ARMOR, "Armor", slot::UPPER_BODY, 12.0, "armor\\metal.nif", None),
        &armo(COWBOY_HAT, "CowboyHat", slot::HAIR | (1 << 9) | slot::HAT, 1.0, "armor\\cowboyhat.nif", None),
        &armo(HELMET, "Helmet", slot::HEAD | slot::HAIR | slot::HAT, 5.0, "armor\\helmet.nif", None),
        &armo(WIG, "Wig", slot::HAIR, 0.0, "armor\\wig.nif", None),
        &armo(GLOVES, "Gloves", slot::LEFT_HAND | slot::RIGHT_HAND, 1.0, "armor\\gloves.nif", None),
    ]);
    let cnto = |item: u32| sub(b"CNTO", cat(&[&fid(item), &1i32.to_le_bytes()]));
    let npc = |id: u32, edid: &str, flags: u32, tf: u16, extra: Vec<Subrecord>| {
        let mut subs = vec![
            sub(b"EDID", z(edid)),
            sub(b"MODL", z("Characters\\_Male\\Skeleton.NIF")),
            acbs(flags, tf),
            sub(b"RNAM", fid(RACE)),
            sub(b"ENAM", fid(EYES)),
            sub(b"HCLR", vec![10, 20, 30, 0]),
            sub(b"FGGS", floats(50, 1.0)),
            sub(b"FGGA", floats(30, 0.0)),
            sub(b"FGTS", floats(50, 0.5)),
        ];
        subs.extend(extra);
        record(b"NPC_", id, &subs)
    };
    let npcs = cat(&[
        &npc(BASE, "Base", 0, 0, vec![cnto(OUTFIT), cnto(ARMOR), cnto(GLOVES), sub(b"PNAM", fid(BEARD)), sub(b"HNAM", fid(HAIR))]),
        &npc(HATTED, "Hatted", 0, 0, vec![cnto(COWBOY_HAT), sub(b"HNAM", fid(HAIR))]),
        &npc(HELMETED, "Helmeted", 0, 0, vec![cnto(HELMET), sub(b"PNAM", fid(BEARD)), sub(b"HNAM", fid(HAIR))]),
        &npc(WIGGED, "Wigged", 0, 0, vec![cnto(WIG), sub(b"HNAM", fid(HAIR))]),
        &npc(WOMAN, "Woman", 1, 0, vec![cnto(OUTFIT)]),
        &npc(
            TEMPLATED,
            "Templated",
            1,
            template::TRAITS | template::MODEL_ANIMATION,
            vec![sub(b"TPLT", fid(LEVELED)), cnto(WIG)],
        ),
        &record(b"NPC_", MARKER, &[sub(b"EDID", z("Marker")), sub(b"MODL", z("marker_creature.nif")), acbs(0, 0), sub(b"RNAM", fid(RACE))]),
    ]);
    let lvlo = |level: u16, item: u32| sub(b"LVLO", cat(&[&level.to_le_bytes(), &[0; 2], &fid(item), &1u16.to_le_bytes(), &[0; 2]]));
    let lvln = record(b"LVLN", LEVELED, &[sub(b"EDID", z("LvlTest")), sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]), lvlo(1, BASE)]);
    cat(&[
        &tes4(),
        &top(b"HAIR", &hair),
        &top(b"EYES", &eyes),
        &top(b"HDPT", &hdpt),
        &top(b"RACE", &race),
        &top(b"ARMO", &armo),
        &top(b"NPC_", &npcs),
        &top(b"LVLN", &lvln),
    ])
}

fn schema() -> Schema {
    Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
}

struct Fixture {
    actors: ActorForms,
    forms: FormStore,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let s = schema();
    Fixture { actors: ActorForms::build(&lo, &s), forms: FormStore::build(&lo, &s) }
}

fn roles(a: &fnv_actor::Appearance) -> Vec<String> {
    a.parts.iter().map(|p| format!("{} {}", p.role, p.model.rsplit('\\').next().unwrap())).collect()
}

#[test]
fn race_sections_load_positionally() {
    let f = fixture();
    assert!(f.actors.issues.is_empty(), "{:?}", f.actors.issues.to_json());
    let r = &f.actors.races[&FormId(RACE)];
    assert_eq!(r.head[0].iter().map(|p| p.index).collect::<Vec<_>>(), vec![0, 1, 2, 6, 7]);
    assert_eq!(r.head[1].len(), 1);
    assert_eq!(r.body[0].iter().map(|p| p.index).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    assert_eq!(r.body[1].len(), 1);
    assert_eq!(r.head[0][1].model, None);
    assert_eq!(r.head[0][1].icon.as_deref(), Some("Characters\\Head\\EarsHuman.dds"));
    assert_eq!(r.height, [1.0, f64::from(0.95f32)]);
    assert_eq!(r.default_hair, [Some(FormId(HAIR_DEFAULT)), Some(FormId(HAIR_DEFAULT))]);
    let fg = |s: usize| r.facegen[s].as_ref().unwrap();
    assert_eq!((fg(0).fggs.len(), fg(0).fgga.len(), fg(0).fgts.len()), (50, 30, 50));
    assert_eq!((fg(0).fggs[0], fg(1).fggs[0]), (0.5, -0.5));
    assert_eq!(f.actors.head_parts[&FormId(BEARD)].extra, vec![FormId(BROW)]);
}

#[test]
fn best_protection_is_worn_and_covers_race_parts() {
    let f = fixture();
    let r = Resolver { actors: &f.actors, forms: &f.forms };
    let a = r.resolve(FormId(BASE), FormId(0x900)).unwrap();
    assert_eq!(a.worn, vec![FormId(ARMOR), FormId(GLOVES)]);
    assert_eq!(a.skeleton, HUMAN_SKELETON);
    assert!(!a.hat);
    assert_eq!(
        roles(&a),
        vec![
            "head:head headhuman.nif",
            "head:mouth mouthhuman.nif",
            "head:left_eye eyelefthuman.nif",
            "head:right_eye eyerighthuman.nif",
            "head_part beard.nif",
            "head_part eyebrowm.nif",
            "hair haira.nif",
            "armor metal.nif",
            "armor gloves.nif",
        ]
    );
    assert_eq!(a.parts[0].texture.as_deref(), Some("textures\\characters\\male\\headhuman.dds"));
    assert_eq!(a.parts[2].texture.as_deref(), Some("textures\\characters\\eyes\\eyeblue.dds"));
    assert_eq!(a.hair_color, Some([10, 20, 30]));
    assert_eq!(a.body_egt.as_deref(), Some("meshes\\characters\\_male\\upperbodyhumanmale.egt"));
    assert_eq!(a.facegen.as_ref().unwrap().fggs[0], 1.0);
    // The face is shaped by the NPC's values plus the race's for its sex (male: 0.5).
    let shape = a.shape_facegen(true).unwrap();
    assert_eq!((shape.fggs[0], shape.fggs.len(), shape.fgga.len(), shape.fgts.len()), (1.5, 50, 30, 0));
    assert_eq!(a.shape_facegen(false).unwrap().fggs[0], 1.0);
}

#[test]
fn headgear_rules() {
    let f = fixture();
    let r = Resolver { actors: &f.actors, forms: &f.forms };
    let hatted = r.resolve(FormId(HATTED), FormId(0x901)).unwrap();
    assert!(hatted.hat);
    assert!(roles(&hatted).contains(&"hair haira.nif".to_string()));
    assert!(roles(&hatted).contains(&"body:upper_body upperbody.nif".to_string()));

    let helmeted = r.resolve(FormId(HELMETED), FormId(0x902)).unwrap();
    assert!(roles(&helmeted).iter().all(|p| !p.starts_with("head") && !p.starts_with("hair")), "{:?}", roles(&helmeted));

    let wigged = r.resolve(FormId(WIGGED), FormId(0x903)).unwrap();
    assert!(!wigged.hat);
    assert!(roles(&wigged).iter().all(|p| !p.starts_with("hair")));
    assert!(roles(&wigged).contains(&"armor wig.nif".to_string()));
}

#[test]
fn female_parts_models_and_default_hair() {
    let f = fixture();
    let r = Resolver { actors: &f.actors, forms: &f.forms };
    let a = r.resolve(FormId(WOMAN), FormId(0x904)).unwrap();
    assert!(a.female);
    assert_eq!(a.race_height, f64::from(0.95f32));
    assert_eq!(roles(&a), vec!["head:head headfemale.nif", "hair hairbase.nif", "armor outfitf.nif"]);
}

#[test]
fn templates_follow_their_groups() {
    let f = fixture();
    let r = Resolver { actors: &f.actors, forms: &f.forms };
    let a = r.resolve(FormId(TEMPLATED), FormId(0x905)).unwrap();
    // Traits (sex) and model (hair, head parts) from BASE through the LVLN; the
    // inventory is its own: a wig.
    assert!(!a.female);
    assert_eq!(a.worn, vec![FormId(WIG)]);
    assert!(roles(&a).contains(&"head_part beard.nif".to_string()));
    assert!(roles(&a).iter().all(|p| !p.starts_with("hair")));
}

#[test]
fn editor_marker_skeleton_falls_back() {
    let f = fixture();
    let r = Resolver { actors: &f.actors, forms: &f.forms };
    let a = r.resolve(FormId(MARKER), FormId(0x906)).unwrap();
    assert_eq!(a.skeleton, HUMAN_SKELETON);
}
