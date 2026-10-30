//! What `Ui::update` costs next to a HUD while a 100-tile menu is opened and closed
//! 1,000 times (issue #1039): `cargo run --release -p fnv-ui --example reopen_bench`.
//! No game data: both menus are written here. Times are the median of many updates.

use fnv_ui::{MemSource, Ui, Value};
use std::time::Instant;

/// HUD tiles, each with three expressions: one reads `screen()`, one a menu trait and one
/// a tile found by name (the path that walks the menu).
const HUD_TILES: usize = 200;
/// Tiles in the menu that is opened and closed.
const MENU_TILES: usize = 100;
const CYCLES: usize = 1000;

fn files() -> MemSource {
    let mut hud = String::from(r#"<menu name="HUDMainMenu"> <_wide> 1 </_wide> <rect name="Box"> <_v> 0 </_v> </rect> <text name="Label"> <string> x </string> </text>"#);
    for i in 0..HUD_TILES {
        hud.push_str(&format!(
            r#"<rect name="R{i}"> <x> <copy src="screen()" trait="width"/> <div> 2 </div> </x> <y> <copy src="HUDMainMenu" trait="_wide"/> <add> {i} </add> </y> <width> <copy src="Box" trait="_v"/> <mul> 2 </mul> </width> </rect>"#
        ));
    }
    hud.push_str("</menu>");
    let mut menu = String::from(r#"<menu name="InventoryMenu"> <_n> 1 </_n>"#);
    for i in 0..MENU_TILES - 1 {
        menu.push_str(&format!(r#"<rect name="I{i}"> <x> <copy src="screen()" trait="width"/> <sub> {i} </sub> </x> <y> <copy src="io()" trait="_n"/> </y> </rect>"#));
    }
    menu.push_str("</menu>");
    MemSource::default().with("menus\\main\\hud_main_menu.xml", &hud).with("menus\\inventory_menu.xml", &menu)
}

/// The median time of an update after `write`, in microseconds.
fn time(ui: &mut Ui, mut write: impl FnMut(&mut Ui, usize)) -> f64 {
    let mut times: Vec<f64> = (0..201)
        .map(|n| {
            write(ui, n);
            let t = Instant::now();
            ui.update();
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

fn report(ui: &mut Ui, when: &str) {
    let s = ui.screen();
    let width = time(ui, |ui, n| ui.set_trait(s, "width", Value::Num(1000.0 + n as f64)));
    let one = time(ui, |ui, n| {
        ui.set_float("HUDMainMenu/Box/_v", n as f64);
    });
    let unread = time(ui, |ui, n| {
        ui.set_string("HUDMainMenu/Label/string", &n.to_string());
    });
    println!("{when}: {} tile slots, {} live", ui.tile_slots(), ui.live_tiles());
    println!("{when}: update after screen width {width:.1} us, after a value 200 traits read {one:.1} us, after a value nothing reads {unread:.1} us");
}

fn main() {
    let src = files();
    let mut ui = Ui::new();
    ui.load_menu(&src, "menus\\main\\hud_main_menu.xml").unwrap();
    ui.update();
    report(&mut ui, "HUD alone");
    let start = Instant::now();
    for _ in 0..CYCLES {
        let m = ui.load_menu(&src, "menus\\inventory_menu.xml").unwrap();
        ui.update();
        ui.unload_tile(m);
        ui.update();
    }
    println!("{CYCLES} opens and closes of a {MENU_TILES}-tile menu: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    report(&mut ui, "after the cycles");
}
