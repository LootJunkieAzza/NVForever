//! Which model of an item record `base_object` gives as the one lying in the world, on a
//! synthetic plugin built in memory and decoded through the repository schemas (no game
//! data). The paths are the game's own for these records (docs/spec-notes/simulation.md
//! §3 "Item models").

use fnv_esm::raw::{write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::placed::base_object;

fn sub(sig: &[u8; 4], text: &str) -> Subrecord {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    Subrecord { sig: *sig, data, extended: false }
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [sig.as_slice(), &(payload.len() as u32).to_le_bytes(), &0u32.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload].concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [b"GRUP".as_slice(), &((24 + body.len()) as u32).to_le_bytes(), sig, &0i32.to_le_bytes(), &[0; 8], body].concat()
}

const LEATHER: u32 = 0x100;
const NO_GROUND: u32 = 0x101;
const REVOLVER: u32 = 0x200;
const KNUCKLES: u32 = 0x201;
const VARMINT: u32 = 0x202;
const CAN: u32 = 0x300;

fn plugin_bytes() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let tes4 = record(b"TES4", 0, &[Subrecord { sig: *b"HEDR", data: hedr, extended: false }]);
    let armo = [
        record(
            b"ARMO",
            LEATHER,
            &[
                sub(b"EDID", "ArmorLeather"),
                sub(b"MODL", "Armor\\LeatherArmor\\M\\OutfitM.NIF"),
                sub(b"MOD2", "Armor\\LeatherArmor\\M\\GO.NIF"),
                sub(b"MOD3", "Armor\\LeatherArmor\\F\\OutfitF.NIF"),
            ],
        ),
        record(b"ARMO", NO_GROUND, &[sub(b"EDID", "NoGroundModel"), sub(b"MODL", "Armor\\Worn.NIF")]),
    ]
    .concat();
    let weap = [
        record(
            b"WEAP",
            REVOLVER,
            &[
                sub(b"EDID", "WeapNV44Revolver"),
                sub(b"MODL", "weapons\\1handpistol\\44magnumrevolvernoscope.nif"),
                sub(b"MOD4", "weapons\\1handpistol\\44magnumrevolvernoscopeGO.nif"),
            ],
        ),
        record(b"WEAP", KNUCKLES, &[sub(b"EDID", "WeapBrassKnuckles"), sub(b"MOD4", "Weapons\\Hand2Hand\\BrassKnucklesWorldObject.NIF")]),
        // MOD2 and MOD3 of a weapon are its shell casing and scope overlay.
        record(
            b"WEAP",
            VARMINT,
            &[
                sub(b"EDID", "WeapNVVarmintRifle"),
                sub(b"MODL", "weapons\\2handrifle\\VarmintRifle.nif"),
                sub(b"MOD2", "weapons\\2handautomatic\\assaultrifleshellcasing.nif"),
                sub(b"MOD3", "interface\\HUD\\varmitscope.NIF"),
            ],
        ),
    ]
    .concat();
    // MOD2 means nothing on a record type that is not armour.
    let misc = record(b"MISC", CAN, &[sub(b"EDID", "TinCan"), sub(b"MODL", "Clutter\\Junk\\TinCan01.NIF"), sub(b"MOD2", "Other.NIF")]);
    [tes4, top(b"ARMO", &armo), top(b"WEAP", &weap), top(b"MISC", &misc)].concat()
}

#[test]
fn items_lie_in_the_world_as_their_world_model() {
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Test.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let world = |fid: u32| base_object(&lo, &schema, fid).unwrap().world_model_path();
    let modl = |fid: u32| base_object(&lo, &schema, fid).unwrap().model_path();

    assert_eq!(world(LEATHER).as_deref(), Some("meshes\\armor\\leatherarmor\\m\\go.nif"));
    assert_eq!(modl(LEATHER).as_deref(), Some("meshes\\armor\\leatherarmor\\m\\outfitm.nif"));
    assert_eq!(world(NO_GROUND).as_deref(), Some("meshes\\armor\\worn.nif"));

    assert_eq!(world(REVOLVER).as_deref(), Some("meshes\\weapons\\1handpistol\\44magnumrevolvernoscopego.nif"));
    assert_eq!(world(KNUCKLES).as_deref(), Some("meshes\\weapons\\hand2hand\\brassknucklesworldobject.nif"));
    assert_eq!(modl(KNUCKLES), None);
    assert_eq!(world(VARMINT).as_deref(), Some("meshes\\weapons\\2handrifle\\varmintrifle.nif"));

    assert_eq!(world(CAN).as_deref(), Some("meshes\\clutter\\junk\\tincan01.nif"));
}
