//! [design] Pure tile layout; drawing interpretations are listed in
//! docs/spec-notes/menu-xml.md §3.1.1. No textures, fonts or Bevy state are loaded here.

use fnv_ui::{Tile, TileId, Ui, Value};

/// [design] Rectangle in the tree's screen units, without viewport scaling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn intersect(self, other: Self) -> Self {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        Self {
            left,
            top,
            width: ((self.left + self.width).min(other.left + other.width) - left).max(0.0),
            height: ((self.top + self.height).min(other.top + other.height) - top).max(0.0),
        }
    }
}

/// [design] Unmapped local colour inputs; named system colours retain their names
/// because their numbers are unknown (docs/spec-notes/menu-xml.md §2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Colour {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub brightness: f64,
    pub systemcolor: Option<f64>,
    pub systemcolor_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub filename: String,
    pub zoom: f64,
    pub tile: Option<f64>,
    pub cropx: Option<f64>,
    pub cropy: Option<f64>,
    /// [design] Retained for the renderer's atlas lookup; no atlas IO during layout.
    pub texatlas: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub string: String,
    /// [hypothesis: menu-xml.md §3.1.3] Positive zoom scales glyphs, not the anchor.
    pub zoom: f64,
    pub font: f64,
    /// [design] Numeric view retained unchanged; named constants also set alignment.
    pub justify: f64,
    pub alignment: Alignment,
    pub wrapwidth: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Rect,
    Image(Image),
    Text(Text),
    Hotrect,
    Nif,
    Radial,
    Window,
    Menu,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawItem {
    pub tile: TileId,
    pub kind: Kind,
    pub rect: Rect,
    pub depth: f64,
    /// [hypothesis] Parent fraction products, kept unrounded in the range 0..=255.
    pub alpha: f64,
    pub visible: bool,
    pub clip: Option<Rect>,
    pub colour: Colour,
}

/// [design] Font number, string and optional wrap width in; measured screen units out.
pub type Measure<'a> = dyn Fn(f64, &str, Option<f64>) -> (f64, f64) + 'a;

#[derive(Clone, Copy)]
struct Context {
    rect: Rect,
    depth: f64,
    alpha: f64,
    visible: bool,
    clip: Option<Rect>,
}

impl Default for Context {
    fn default() -> Self {
        Self { rect: Rect::default(), depth: 0.0, alpha: 255.0, visible: true, clip: None }
    }
}

fn number(tile: &Tile, name: &str, default: f64) -> f64 {
    tile.get(name).map_or(default, Value::num)
}

fn optional(tile: &Tile, name: &str) -> Option<f64> {
    tile.get(name).map(Value::num)
}

fn symbol(tile: &Tile, name: &str) -> Option<String> {
    match tile.get(name) {
        Some(Value::Sym(name)) => Some(name.clone()),
        _ => None,
    }
}

fn text_zoom(tile: &Tile) -> f64 {
    let zoom = number(tile, "zoom", 100.0);
    if zoom > 0.0 && zoom.is_finite() { zoom / 100.0 } else { 1.0 }
}

fn kind(tile: &Tile) -> Kind {
    match tile.kind.as_str() {
        "image" => Kind::Image(Image {
            filename: tile.get("filename").map(Value::text).unwrap_or_default(),
            zoom: number(tile, "zoom", 100.0),
            tile: optional(tile, "tile"),
            cropx: optional(tile, "cropx"),
            cropy: optional(tile, "cropy"),
            texatlas: tile.get("texatlas").cloned(),
        }),
        "text" => Kind::Text(Text {
            string: tile.get("string").map(Value::text).unwrap_or_default(),
            zoom: text_zoom(tile),
            font: number(tile, "font", 0.0),
            justify: number(tile, "justify", 0.0),
            alignment: match symbol(tile, "justify").as_deref() {
                Some("center") => Alignment::Center,
                Some("right") => Alignment::Right,
                _ => Alignment::Left,
            },
            wrapwidth: optional(tile, "wrapwidth"),
        }),
        "hotrect" => Kind::Hotrect,
        "nif" => Kind::Nif,
        "radial" => Kind::Radial,
        "window" => Kind::Window,
        "menu" => Kind::Menu,
        _ => Kind::Rect,
    }
}

fn place(tile: &Tile, kind: &Kind, parent: Context, measure: &Measure<'_>) -> Context {
    let mut rect = Rect {
        left: parent.rect.left + number(tile, "x", 0.0),
        top: parent.rect.top + number(tile, "y", 0.0),
        width: number(tile, "width", 0.0),
        height: number(tile, "height", 0.0),
    };
    if let Kind::Text(text) = kind {
        if tile.get("width").is_none() || tile.get("height").is_none() {
            let (w, h) = measure(text.font, &text.string, text.wrapwidth);
            rect.width = number(tile, "width", w);
            rect.height = number(tile, "height", h);
        }
        rect.width *= text.zoom;
        rect.height *= text.zoom;
        rect.left -= match text.alignment {
            Alignment::Left => 0.0,
            Alignment::Center => rect.width / 2.0,
            Alignment::Right => rect.width,
        };
    }
    Context {
        rect,
        depth: parent.depth + number(tile, "depth", 0.0),
        alpha: parent.alpha * number(tile, "alpha", 255.0).clamp(0.0, 255.0) / 255.0,
        visible: parent.visible && tile.get("visible").is_none_or(Value::truthy),
        clip: parent.clip,
    }
}

fn children_context(tile: &Tile, mut context: Context) -> Context {
    if tile.get("clips").is_some_and(Value::truthy) {
        context.clip = Some(context.clip.map_or(context.rect, |clip| clip.intersect(context.rect)));
    }
    context
}

/// [design] Lay out a live subtree in parent-first order, siblings stably sorted by
/// local depth. Ancestors outside root still contribute place, alpha, depth and clips.
/// Hidden items retain visible=false; menu/hotrect containers emit no item.
pub fn layout(ui: &Ui, root: TileId, measure: &Measure<'_>) -> Vec<DrawItem> {
    let mut ancestors = Vec::new();
    let mut at = ui.tile(root).parent;
    while let Some(id) = at {
        ancestors.push(id);
        at = ui.tile(id).parent;
    }
    let mut context = Context::default();
    for id in ancestors.into_iter().rev() {
        let tile = ui.tile(id);
        if !tile.live {
            return Vec::new();
        }
        context = children_context(tile, place(tile, &kind(tile), context, measure));
    }
    let mut out = Vec::new();
    let mut stack = vec![(root, context)];
    while let Some((id, parent)) = stack.pop() {
        let tile = ui.tile(id);
        if !tile.live {
            continue;
        }
        let kind = kind(tile);
        let context = place(tile, &kind, parent, measure);
        if !matches!(kind, Kind::Menu | Kind::Hotrect) {
            out.push(DrawItem {
                tile: id,
                kind,
                rect: context.rect,
                depth: context.depth,
                alpha: context.alpha,
                visible: context.visible,
                clip: context.clip,
                colour: Colour {
                    red: number(tile, "red", 255.0).clamp(0.0, 255.0),
                    green: number(tile, "green", 255.0).clamp(0.0, 255.0),
                    blue: number(tile, "blue", 255.0).clamp(0.0, 255.0),
                    brightness: number(tile, "brightness", 255.0),
                    systemcolor: optional(tile, "systemcolor"),
                    systemcolor_name: symbol(tile, "systemcolor"),
                },
            });
        }
        let parent = children_context(tile, context);
        let mut children = tile.children.clone();
        children.sort_by(|&a, &b| number(ui.tile(a), "depth", 0.0).total_cmp(&number(ui.tile(b), "depth", 0.0)));
        stack.extend(children.into_iter().rev().map(|id| (id, parent)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    fn tree(xml: &str) -> (Ui, TileId) {
        let src = MemSource::default().with("test.xml", xml);
        let mut ui = Ui::new();
        let root = ui.load_menu(&src, "test.xml").unwrap();
        ui.update();
        (ui, root)
    }

    fn unmeasured(_: f64, _: &str, _: Option<f64>) -> (f64, f64) {
        panic!("this tree needs no text measurement")
    }

    #[test]
    fn places_and_depth_add_through_containers_and_subtree_roots() {
        let (ui, root) = tree(
            r#"<menu name="M"><x>10</x><y>20</y><depth>-50</depth>
          <hotrect name="H"><x>3</x><y>4</y><depth>2</depth>
            <rect name="R"><x>5</x><y>6</y><width>30</width><height>40</height><depth>1</depth></rect>
          </hotrect></menu>"#,
        );
        let items = layout(&ui, root, &unmeasured);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, Kind::Rect);
        assert_eq!(items[0].rect, Rect { left: 18.0, top: 30.0, width: 30.0, height: 40.0 });
        assert_eq!(items[0].depth, -47.0);
        assert_eq!(layout(&ui, items[0].tile, &unmeasured), items);
    }

    #[test]
    fn parents_precede_children_and_sibling_depth_ties_keep_file_order() {
        let (ui, root) = tree(
            r#"<menu name="M">
          <rect name="Later"><depth>2</depth></rect>
          <rect name="First"><depth>-1</depth><rect name="Child"><depth>100</depth></rect></rect>
          <rect name="Tie1"/><rect name="Tie2"/>
        </menu>"#,
        );
        let items = layout(&ui, root, &unmeasured);
        let names: Vec<_> = items.iter().map(|i| ui.tile(i.tile).name.as_str()).collect();
        assert_eq!(names, ["First", "Child", "Tie1", "Tie2", "Later"]);
        assert_eq!(items[1].depth, 99.0);
    }

    #[test]
    fn alpha_products_stay_unrounded_and_visibility_hides_descendants() {
        let (ui, root) = tree(
            r#"<menu name="M"><alpha>128</alpha><visible>&false;</visible>
          <rect name="R"><alpha>128</alpha><visible>&true;</visible>
            <image name="I"><alpha>128</alpha></image>
          </rect></menu>"#,
        );
        let items = layout(&ui, root, &unmeasured);
        assert!((items[0].alpha - 128.0 * 128.0 / 255.0).abs() < 1e-10);
        assert!((items[1].alpha - 128.0_f64.powi(3) / 255.0_f64.powi(2)).abs() < 1e-10);
        assert!(items.iter().all(|i| !i.visible));
        assert_eq!(layout(&ui, items[1].tile, &unmeasured), items[1..]);
    }

    #[test]
    fn clips_apply_to_children_and_intersect_through_nested_clippers() {
        let (ui, root) = tree(
            r#"<menu name="M"><x>10</x><y>20</y><width>100</width><height>80</height><clips>&true;</clips>
          <rect name="R"><x>50</x><y>-10</y><width>80</width><height>40</height><clips>&true;</clips>
            <image name="I"><width>200</width><height>200</height></image>
          </rect>
          <rect name="Outside"><x>200</x><width>10</width><height>10</height><clips>1</clips><rect name="Empty"/></rect>
        </menu>"#,
        );
        let items = layout(&ui, root, &unmeasured);
        assert_eq!(items[0].clip, Some(Rect { left: 10.0, top: 20.0, width: 100.0, height: 80.0 }));
        assert_eq!(items[1].clip, Some(Rect { left: 60.0, top: 20.0, width: 50.0, height: 30.0 }));
        assert_eq!(items[1].rect.width, 200.0);
        assert_eq!(items[3].clip.unwrap().width, 0.0);
        assert_eq!(layout(&ui, items[1].tile, &unmeasured), items[1..2]);
    }

    #[test]
    fn defaults_and_kind_inputs_are_preserved_without_loading_assets() {
        let (ui, root) = tree(
            r#"<menu name="M">
          <image name="Default"/>
          <image name="Set"><filename>pic.dds</filename><zoom>75</zoom><tile>2</tile><cropx>3</cropx><cropy>4</cropy>
            <texatlas>atlas.tai</texatlas><red>-1</red><green>128</green><blue>300</blue><brightness>160</brightness><systemcolor>7</systemcolor></image>
          <nif name="N"/><radial name="R"/><window name="W"><systemcolor>&hudmain;</systemcolor></window>
        </menu>"#,
        );
        let items = layout(&ui, root, &unmeasured);
        let item = &items[0];
        assert_eq!(item.rect, Rect::default());
        assert_eq!((item.depth, item.alpha, item.visible, item.clip), (0.0, 255.0, true, None));
        assert_eq!(item.colour, Colour { red: 255.0, green: 255.0, blue: 255.0, brightness: 255.0, systemcolor: None, systemcolor_name: None });
        assert_eq!(item.kind, Kind::Image(Image { filename: String::new(), zoom: 100.0, tile: None, cropx: None, cropy: None, texatlas: None }));
        assert_eq!(
            items[1].kind,
            Kind::Image(Image { filename: "pic.dds".into(), zoom: 75.0, tile: Some(2.0), cropx: Some(3.0), cropy: Some(4.0), texatlas: Some(Value::Str("atlas.tai".into())) })
        );
        assert_eq!(items[1].colour, Colour { red: 0.0, green: 128.0, blue: 255.0, brightness: 160.0, systemcolor: Some(7.0), systemcolor_name: None });
        assert_eq!(items[2].kind, Kind::Nif);
        assert_eq!(items[3].kind, Kind::Radial);
        assert_eq!(items[4].kind, Kind::Window);
        assert_eq!(items[4].colour.systemcolor_name.as_deref(), Some("hudmain"));
    }

    #[test]
    fn text_measures_missing_dimensions_and_anchors_named_justification() {
        let (ui, root) = tree(
            r#"<menu name="M"><x>100</x><y>20</y>
          <text name="Left"><string>abc</string><font>3</font><wrapwidth>60</wrapwidth><justify>&left;</justify></text>
          <text name="Center"><string>abc</string><width>40</width><justify>&center;</justify></text>
          <text name="Right"><string>abc</string><height>12</height><justify>&right;</justify></text>
          <text name="Explicit"><string>abc</string><width>0</width><height>0</height><justify>17</justify></text>
        </menu>"#,
        );
        let calls = std::cell::RefCell::new(Vec::new());
        let measure = |font, text: &str, wrap| {
            calls.borrow_mut().push((font, text.to_string(), wrap));
            (30.0, 10.0)
        };
        let items = layout(&ui, root, &measure);
        assert_eq!(*calls.borrow(), [(3.0, "abc".into(), Some(60.0)), (0.0, "abc".into(), None), (0.0, "abc".into(), None)]);
        assert_eq!(items[0].rect, Rect { left: 100.0, top: 20.0, width: 30.0, height: 10.0 });
        assert_eq!(items[1].rect, Rect { left: 80.0, top: 20.0, width: 40.0, height: 10.0 });
        assert_eq!(items[2].rect, Rect { left: 70.0, top: 20.0, width: 30.0, height: 12.0 });
        assert_eq!(items[3].rect, Rect { left: 100.0, top: 20.0, width: 0.0, height: 0.0 });
        let Kind::Text(text) = &items[3].kind else { panic!("expected text") };
        assert_eq!((text.justify, text.alignment), (17.0, Alignment::Left));
    }

    #[test]
    fn unloaded_subtrees_emit_nothing() {
        let (mut ui, root) = tree(r#"<menu name="M"><rect name="R"><image name="I"/></rect></menu>"#);
        let rect = ui.find_tile("M/R").unwrap();
        ui.unload_tile(rect);
        assert!(layout(&ui, root, &unmeasured).is_empty());
        assert!(layout(&ui, rect, &unmeasured).is_empty());
    }
}
