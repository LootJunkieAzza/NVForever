//! `TextEditMenu` (`menus::NAME`, 1051) XML path resolution and JIP `ShowTextInputMenu`
//! tile-tree trait bridge (`docs/spec-notes/menu-xml.md` §12).
//!
//! Vanilla `GetPlayerName` / `ShowNameMenu` opens `menus\dialog\texteditmenu.xml` without
//! any extra trait writes. When JIP LN's `ShowTextInputMenu` opens menu `1051`, if the
//! VFS provides `NVSE\Plugins\textinput\texteditmenu.xml` ([observed: JIP LN NVSE Plugin
//! 57.30, archive `JIP LN NVSE Plugin-58277-57-30-1716662080`, shipped
//! `NVSE\Plugins\textinput\texteditmenu.xml` read directly; only the XML layout was read,
//! not the DLL or source]), that file is loaded into the tile tree instead; otherwise the
//! vanilla `TextEditMenu` XML is used as a fallback.
//!
//! Only traits already declared in the loaded XML are written (`Ui::set_existing`), and
//! only while `Game::text_input()` is active, so unmodded vanilla `GetPlayerName` behavior
//! and tile counts remain unchanged.

use crate::{game::menus, nvse_jip12::TextInput, Game};
use fnv_ui::{value::menu_class_number, Source, TileId, Ui, Value};

/// VFS path of the JIP LN `ShowTextInputMenu` layout XML (relative to `Data\`).
pub const TEXT_INPUT_MENU_PATH: &str = "nvse\\plugins\\textinput\\texteditmenu.xml";

/// Resolve which XML path should be loaded for `TextEditMenu` (menu 1051) [design].
///
/// - When `ShowTextInputMenu` is active (`game.text_input().is_some()`) and
///   [`TEXT_INPUT_MENU_PATH`] exists in `files`, prefer [`TEXT_INPUT_MENU_PATH`].
/// - Otherwise fall back to the indexed `TextEditMenu` class path
///   (`menus\dialog\texteditmenu.xml`).
pub fn menu_path<'a>(game: &'a Game<'_>, files: &dyn Source) -> Option<&'a str> {
    if game.text_input().is_some() && files.read(TEXT_INPUT_MENU_PATH).is_some() {
        return Some(TEXT_INPUT_MENU_PATH);
    }
    game.ui_menus.code_path(menus::NAME)
}

fn find_text_edit_root(ui: &Ui) -> Option<TileId> {
    ui.menus().find(|&root| match ui.tile(root).get("class") {
        Some(Value::Sym(name)) => menu_class_number(name) == Some(menus::NAME),
        _ => false,
    })
}

fn title_tile(ui: &Ui, root: TileId) -> Option<TileId> {
    let menu = &ui.tile(root).name;
    ui.find_tile(&format!("{menu}/NOGLOW_BRANCH/TEM_TitleText"))
        .or_else(|| ui.find_tile(&format!("{menu}/TEM_MainRect/textedit_prompt")))
}

fn input_tile(ui: &Ui, root: TileId) -> Option<TileId> {
    let menu = &ui.tile(root).name;
    ui.find_tile(&format!("{menu}/NOGLOW_BRANCH/TEM_TextInputBox/TEM_InputText"))
        .or_else(|| ui.find_tile(&format!("{menu}/TEM_MainRect/textedit_text")))
}

/// Write `input`'s size (`user0`/`user1`), offset (`user2`/`user3`), title, and live text
/// into the existing traits of a loaded `TextEditMenu` root [design; layout trait roles,
/// defaults, and `NOGLOW_BRANCH/TEM_*` tile names observed: JIP LN NVSE Plugin 57.30,
/// archive `JIP LN NVSE Plugin-58277-57-30-1716662080`, shipped
/// `NVSE\Plugins\textinput\texteditmenu.xml` read directly; only the XML layout was read,
/// not the DLL or source].
///
/// Uses `Ui::set_existing` so traits not present in the loaded XML (such as `user0`..`user3`
/// on vanilla `menus\dialog\texteditmenu.xml`) are never invented, and only writes `user0` /
/// `user1` when the caller supplied a positive dimension (preserving the XML defaults when
/// width/height are 0). Returns `true` if any trait was written.
pub fn sync_to_tree(ui: &mut Ui, root: TileId, input: &TextInput) -> bool {
    if !ui.is_live(root) {
        return false;
    }
    let mut wrote = false;
    if input.size.0 > 0.0 {
        wrote |= ui.set_existing(root, "user0", Value::Num(f64::from(input.size.0)));
    }
    if input.size.1 > 0.0 {
        wrote |= ui.set_existing(root, "user1", Value::Num(f64::from(input.size.1)));
    }
    wrote |= ui.set_existing(root, "user2", Value::Num(f64::from(input.offset.0)));
    wrote |= ui.set_existing(root, "user3", Value::Num(f64::from(input.offset.1)));
    if let Some(title) = title_tile(ui, root) {
        wrote |= ui.set_existing(title, "string", Value::Str(input.title.clone()));
    }
    if let Some(text) = input_tile(ui, root) {
        wrote |= ui.set_existing(text, "string", Value::Str(input.text.clone()));
    }
    wrote
}

/// Sync the active `ShowTextInputMenu` properties (dimensions, offsets, title, and text)
/// into an already-loaded `TextEditMenu` in `game.ui_tree`, recomputing dependent XML
/// expressions when any trait changed.
pub fn sync_open(game: &mut Game<'_>) -> bool {
    if game.menu != Some(menus::NAME) {
        return false;
    }
    let Some(input) = game.text_input().cloned() else {
        return false;
    };
    let Some(root) = find_text_edit_root(&game.ui_tree) else {
        return false;
    };
    let wrote = sync_to_tree(&mut game.ui_tree, root, &input);
    if wrote {
        game.ui_tree.update();
    }
    wrote
}

/// Sync only the live input string into an already-loaded `TextEditMenu` without touching
/// `user0`..`user3` or title traits (preserving any script `SetUIFloat`/`SetUIString`
/// writes on those traits).
pub fn sync_live_text(game: &mut Game<'_>, text: &str) -> bool {
    if game.menu != Some(menus::NAME) || game.text_input().is_none() {
        return false;
    }
    let Some(root) = find_text_edit_root(&game.ui_tree) else {
        return false;
    };
    let Some(tile) = input_tile(&game.ui_tree, root) else {
        return false;
    };
    let wrote = game.ui_tree.set_existing(tile, "string", Value::Str(text.to_string()));
    if wrote {
        game.ui_tree.update();
    }
    wrote
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_script::vm::XVal;
    use fnv_ui::MemSource;

    fn sample_input(size: (f32, f32), offset: (f32, f32), title: &str, text: &str) -> TextInput {
        TextInput {
            callback: XVal::Num(0.0),
            title: title.into(),
            size,
            offset,
            min: 1,
            max: 64,
            digits_only: false,
            enter_closes: false,
            text: text.into(),
            set_text: None,
        }
    }

    #[test]
    fn sync_to_tree_updates_existing_jip_traits_and_preserves_defaults_on_zero_size() {
        let src = MemSource::default().with(
            TEXT_INPUT_MENU_PATH,
            r#"<menu name="TextEditMenu">
                <class>&TextEditMenu;</class>
                <user0>400</user0>
                <user1>200</user1>
                <user2>0</user2>
                <user3>0</user3>
                <rect name="NOGLOW_BRANCH">
                    <text name="TEM_TitleText"><string></string></text>
                    <hotrect name="TEM_TextInputBox">
                        <text name="TEM_InputText"><string></string></text>
                    </hotrect>
                </rect>
            </menu>"#,
        );
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, TEXT_INPUT_MENU_PATH).unwrap();
        assert!(sync_to_tree(&mut ui, root, &sample_input((0.0, 0.0), (15.0, 25.0), "Prompt", "Typed")));
        ui.update();
        assert_eq!(ui.get_float("TextEditMenu/user0"), Some(400.0));
        assert_eq!(ui.get_float("TextEditMenu/user1"), Some(200.0));
        assert_eq!(ui.get_float("TextEditMenu/user2"), Some(15.0));
        assert_eq!(ui.get_float("TextEditMenu/user3"), Some(25.0));
        assert_eq!(ui.get_string("TextEditMenu/NOGLOW_BRANCH/TEM_TitleText/string").as_deref(), Some("Prompt"));
        assert_eq!(
            ui.get_string("TextEditMenu/NOGLOW_BRANCH/TEM_TextInputBox/TEM_InputText/string").as_deref(),
            Some("Typed")
        );
    }

    #[test]
    fn sync_to_tree_does_not_invent_user_traits_on_vanilla_layout() {
        let src = MemSource::default().with(
            "menus/dialog/texteditmenu.xml",
            r#"<menu name="TextEditMenu">
                <class>&TextEditMenu;</class>
                <rect name="TEM_MainRect">
                    <text name="textedit_prompt"><string></string></text>
                    <text name="textedit_text"><string></string></text>
                </rect>
            </menu>"#,
        );
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "menus/dialog/texteditmenu.xml").unwrap();
        assert!(sync_to_tree(&mut ui, root, &sample_input((600.0, 300.0), (10.0, 20.0), "Ask", "Ans")));
        assert_eq!(ui.get_float("TextEditMenu/user0"), None);
        assert_eq!(ui.get_float("TextEditMenu/user2"), None);
        assert_eq!(ui.get_string("TextEditMenu/TEM_MainRect/textedit_prompt/string").as_deref(), Some("Ask"));
        assert_eq!(ui.get_string("TextEditMenu/TEM_MainRect/textedit_text/string").as_deref(), Some("Ans"));
    }
}
