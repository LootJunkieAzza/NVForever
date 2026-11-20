//! Source calls through the real barter request/open systems, with authored data.
use super::*;
use fnv_esm::{raw::{write_subrecords, Subrecord}, Plugin};

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord { Subrecord { sig: *sig, data, extended: false } }
fn z(text: &str) -> Vec<u8> { [text.as_bytes(), &[0]].concat() }
fn record(sig: &[u8; 4], id: u32, subs: &[Subrecord]) -> Vec<u8> {
    let body = write_subrecords(subs);
    [&sig[..], &(body.len() as u32).to_le_bytes(), &[0; 4], &id.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &body].concat()
}
fn group(label: [u8; 4], kind: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &kind.to_le_bytes(), &[0; 8], body].concat()
}
fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> { group(*sig, 0, body) }

fn fixture() -> Vec<u8> {
    let inventory = |item: u32| sub(b"CNTO", [item.to_le_bytes(), 1i32.to_le_bytes()].concat());
    let actor = |sig, id| record(sig, id, &[inventory(0x901)]);
    let list = |sig, id, actor: u32| record(sig, id, &[
        sub(b"LVLD", vec![0]), sub(b"LVLF", vec![0]),
        sub(b"LVLO", [1u16.to_le_bytes().to_vec(), vec![0; 2], actor.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), vec![0; 2]].concat()),
    ]);
    let placed = |sig, id: u32, base: u32| record(sig, id, &[
        sub(b"EDID", z(&format!("Ref{id:X}"))), sub(b"NAME", base.to_le_bytes().to_vec()),
        sub(b"DATA", vec![0; 24]), sub(b"XMRC", 0x310u32.to_le_bytes().to_vec()),
    ]);
    let refs = [placed(b"ACHR", 0x301, 0x801), placed(b"ACHR", 0x302, 0x811),
        placed(b"ACRE", 0x303, 0x802), placed(b"ACRE", 0x304, 0x812),
        placed(b"REFR", 0x310, 0x820), placed(b"REFR", 0x311, 0x821)].concat();
    let cell = [record(b"CELL", 0x200, &[sub(b"DATA", vec![1])]), group(0x200u32.to_le_bytes(), 6, &group(0x200u32.to_le_bytes(), 8, &refs))].concat();
    let item = |id: u32| record(b"MISC", id, &[sub(b"EDID", z(&format!("Item{id:X}"))), sub(b"FULL", z(&format!("Item {id:X}"))), sub(b"DATA", [5i32.to_le_bytes(), 1f32.to_le_bytes()].concat())]);
    [record(b"TES4", 0, &[sub(b"HEDR", [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat())]),
        top(b"NPC_", &[actor(b"NPC_", FormId::PLAYER_BASE.0), actor(b"NPC_", 0x801)].concat()),
        top(b"CREA", &actor(b"CREA", 0x802)), top(b"LVLN", &list(b"LVLN", 0x811, 0x801)), top(b"LVLC", &list(b"LVLC", 0x812, 0x802)),
        top(b"CONT", &[record(b"CONT", 0x820, &[inventory(0x902)]), record(b"CONT", 0x821, &[inventory(0x903)])].concat()),
        top(b"MISC", &[item(0x901), item(0x902), item(0x903)].concat()),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell))),
    ].concat()
}

fn source(app: &mut App, text: &str) {
    app.world_mut().resource_scope(|world, mut script: Mut<crate::script::ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        crate::script::with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game.run_query(&format!("Begin Function {{}}\n{text}\nEnd\n"), Default::default()).unwrap();
        });
    });
}

fn open_and_assert(app: &mut App, merchant: u32, stock: u32, item: u32) {
    app.world_mut().resource_mut::<ActiveMenu>().0 = None;
    source(app, &format!("Ref{merchant:X}.ShowBarterMenu 0\n"));
    app.update();
    let active = app.world().resource::<ActiveMenu>();
    let menu = active.0.as_ref().expect("the production request opens barter");
    assert_eq!(menu.mode, Mode::Barter(FormId(merchant)));
    let trade = menu.trade.as_ref().unwrap();
    assert_eq!(trade.stock, FormId(stock));
    // These merchants have no services: the actual stock still reaches the model,
    // whose existing service filter withholds its item rather than offering it.
    assert_eq!(trade.withheld.iter().map(|(id, _)| id.0).collect::<Vec<_>>(), vec![item]);
}

#[test]
fn source_merchant_clear_and_setter_reach_actual_barter_stock_before_and_after_restore() {
    let bytes = fixture();
    let plugins = [Plugin::parse(&bytes, "MerchantSource.esm".into()).unwrap()];
    let mut script = crate::script::ScriptState::over_plugins(&plugins);
    let rt = script.rt.as_mut().unwrap();
    let forms = rt.game.world.forms;
    let refs = rt.game.world.placed;
    let sim = SimState { forms, refs, world: std::mem::replace(&mut rt.game.world, fnv_sim::World::new(forms, refs, Default::default())) };
    let mut app = App::new();
    app.insert_resource(script).insert_resource(sim)
        .insert_resource(State::new(PlayState::Playing)).init_resource::<NextState<PlayState>>()
        .init_resource::<ActiveMenu>().init_resource::<FreshMenu>()
        .add_message::<OpenContainer>().add_message::<crate::pickpocket::OpenPickpocket>()
        .add_message::<OpenInventory>().add_message::<OpenBarter>()
        .add_systems(Update, (scripts_open_barter, open_menus).chain());

    for merchant in [0x301, 0x302, 0x303, 0x304] {
        open_and_assert(&mut app, merchant, 0x310, 0x902);
        source(&mut app, &format!("RemoveMerchantContainer Ref{merchant:X}\n"));
        open_and_assert(&mut app, merchant, merchant, 0x901);
    }
    source(&mut app, "RemoveMerchantContainer 0\nRemoveMerchantContainer Ref310\n");
    open_and_assert(&mut app, 0x302, 0x302, 0x901);

    app.world_mut().resource_scope(|world, mut script: Mut<crate::script::ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        crate::script::with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            let snapshot = rt.game.snapshot();
            let restored_world = fnv_sim::World::restore(forms, refs, &snapshot.sim).unwrap();
            rt.game = fnv_game::Game::restore(rt.game.data, restored_world, rt.game.cfg, &snapshot).unwrap();
        });
    });
    for merchant in [0x301, 0x302, 0x303, 0x304] {
        open_and_assert(&mut app, merchant, merchant, 0x901);
        source(&mut app, &format!("Ref{merchant:X}.SetMerchantContainer Ref311\n"));
        open_and_assert(&mut app, merchant, 0x311, 0x903);
    }
}
