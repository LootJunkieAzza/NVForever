//! Settings uses the menu-sheet router's single raw contact stream, and hits the
//! same Bevy buttons as the mouse. Only the pause consumer applies effects.
use super::*;
use crate::{menu::barter_touch::Frame, pipboy::touch_scroll::gesture::Intent, touch_safe_area::SafeInsets};
use bevy::{input::touch::{TouchInput, TouchPhase}, ui::{CalculatedClip, ComputedUiTargetCamera, UiStack}, window::{PrimaryWindow, Window}};

#[derive(Resource)]
pub struct SettingsTouch {
    enabled: bool,
    frame: Option<Frame>,
    edges: Vec<TouchInput>,
    contact: Option<Contact>,
}

impl Default for SettingsTouch {
    fn default() -> Self {
        Self::for_platform(cfg!(target_os = "android"))
    }
}

impl SettingsTouch {
    fn for_platform(enabled: bool) -> Self { Self { enabled, frame: None, edges: Vec::new(), contact: None } }

    pub fn enabled(&self) -> bool { self.enabled }

    #[cfg(feature = "test-fixtures")]
    pub(super) fn enabled_fixture() -> Self { Self::for_platform(true) }

    pub fn publish(&mut self, frame: Option<Frame>, edges: &mut Vec<TouchInput>) {
        self.edges.clear();
        self.frame = frame.filter(|f| f.admitted && !f.fresh);
        if self.frame.is_some() {
            std::mem::swap(&mut self.edges, edges);
        } else {
            self.contact = None;
        }
    }
}

// Sizes are logical window pixels. A user menu scale cannot shrink the touch targets.
const PAD: f32 = 8.0;
const CELL: f32 = OPTION_ROW;
const NOTE: f32 = 2.0 * NOTE_STEP;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    safe: Rect,
    height: f32,
    columns: usize,
    rail_width: f32,
    row_left: f32,
    row_width: f32,
    header: f32,
    note: f32,
}

impl Layout {
    fn of(size: Vec2, insets: SafeInsets, tabs: usize) -> Option<Self> {
        let safe = Rect::from_corners(Vec2::new(insets.left, insets.top) + Vec2::splat(PAD),
            size - Vec2::new(insets.right, insets.bottom) - Vec2::splat(PAD));
        let width = safe.width();
        let height = safe.height();
        if width < 180.0 || height < 2.0 * CELL || !size.is_finite() { return None; }
        // Use a side rail where there's room. In portrait the same tabs form a grid above the list.
        let side = width >= 590.0 && tabs.div_ceil(2) as f32 * CELL <= height - CELL;
        let available_rail_rows = ((height / CELL).floor() as usize).saturating_sub(3).max(1);
        let columns = if side { 2 } else {
            ((width / 92.0).floor() as usize).max(2).max(tabs.div_ceil(available_rail_rows))
                .min((width / 48.0).floor() as usize)
        };
        let rail_width = if side { 232.0 } else { width };
        let row_left = if side { rail_width + PAD } else { 0.0 };
        let header = if side { 0.0 } else { tabs.div_ceil(columns) as f32 * CELL };
        let note = if height - header >= 5.0 * CELL { NOTE } else { 0.0 };
        Some(Self { safe, height: size.y, columns, rail_width, row_left, row_width: width - row_left, header, note })
    }

    pub(super) fn list(self) -> ListBox {
        ListBox { left: self.row_left - self.safe.width() / 2.0, width: self.row_width }
    }
    pub(super) fn top(self, scope: SettingsScope) -> f32 {
        self.header + if scope == SettingsScope::Search { CELL } else { 0.0 }
    }
    fn footer(self) -> f32 { self.safe.height() - CELL }

    pub(super) fn note(self, root: &mut ChildSpawnerCommands, f: &Faces, text: &str, lb: ListBox, scope: SettingsScope) {
        if self.note == 0.0 { return; }
        let x = lb.left + PAD;
        let top = self.top(scope) + self.lines(scope) as f32 * CELL;
        for (i, line) in note_wrapped(&f.small.font, text, lb.width - 2.0 * PAD).into_iter().take(2).enumerate() {
            root.spawn(from_middle(x, top + i as f32 * NOTE_STEP, lb.width - 2.0 * PAD, NOTE_STEP))
                .with_children(|row| crate::interface::set_line(row, &f.small, &line, 0.0, NOTE_STEP * 0.8, color(Shade::Bright)));
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn choices_panel(self, root: &mut ChildSpawnerCommands, d: &Drawn, f: &Faces, o: Dropdown, names: &[String], left: f32, width: f32) {
        let shown = names.len().min(self.choices());
        let h = shown as f32 * CELL + 2.0 * LINE;
        let y = ((self.footer() - h) / 2.0).max(0.0);
        let (line, _) = highlight_colors();
        root.spawn((Surface::Choices, GlobalZIndex(111), BackgroundColor(Color::BLACK), BorderColor::all(line),
            Node { border: UiRect::all(u(LINE)), ..from_middle(left, y, width, h) })).with_children(|panel| {
            let inside = width - 2.0 * LINE;
            let first = o.first.min(names.len().saturating_sub(shown));
            for (index, name) in names.iter().enumerate().skip(first).take(shown) {
                let text = fit_text(|s| f.entry.font.width(s), &on_page(d.choice_words(o.row, index, name)), inside - 2.0 * PAD);
                let shade = if d.choice_reason(o.row, index).is_some() { Shade::Dim } else { Shade::Bright };
                let x = (inside - f.entry.font.width(&text)) / 2.0;
                panel.spawn((Button, PauseButton::Choice(o.row, index), boxed(at(0.0, (index - first) as f32 * CELL, inside, CELL), index == o.at)))
                    .with_children(|row| sm_text(row, &f.entry, &text, x, crate::start_menu::base_in(&f.entry, CELL), color(shade)));
            }
        });
    }
    pub(super) fn lines(self, scope: SettingsScope) -> usize {
        ((self.footer() - self.top(scope) - self.note) / CELL).floor().max(1.0) as usize
    }
    pub(super) fn choices(self) -> usize {
        (((self.footer() - 2.0 * LINE) / CELL).floor() as usize).clamp(1, DROP_ROWS)
    }
    fn matches(self, frame: Frame, tabs: usize) -> bool {
        Self::of(frame.size, frame.safe, tabs) == Some(self)
    }
    fn node(self) -> Node {
        Node { overflow: Overflow::clip(), ..at(self.safe.min.x, self.safe.min.y, self.safe.width(), self.safe.height()) }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum Surface { Rows, Choices }

pub(super) fn paint(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, settings: &[Setting], selected: Option<RowId>, layout: Layout, glyphs: &crate::pad::PadGlyphs) {
    let _units = crate::interface::units_at(960.0 / layout.height);
    let f = &look.faces;
    root.spawn((layout.node(), BackgroundColor(Color::BLACK.with_alpha(0.8)))).with_children(|sheet| {
        let cell_w = layout.rail_width / layout.columns as f32;
        for (i, item) in d.rail.iter().copied().filter(|r| !matches!(r, RailItem::Defaults | RailItem::Back)).enumerate() {
            let x = (i % layout.columns) as f32 * cell_w;
            let y = (i / layout.columns) as f32 * CELL;
            let on = item == RailItem::of(d.scope) || d.focus == SettingsFocus::Rail(item);
            button(sheet, &f.small, rail_label(item, d.device.1), at(x, y, cell_w, CELL), cell_w, on, PauseButton::Rail(item));
        }
        let w = layout.safe.width() / if d.report.is_some() { 3.0 } else { 2.0 };
        for (i, (label, action, on)) in [
            ("Back", PauseButton::Back, d.focus == SettingsFocus::Rail(RailItem::Back)),
            ("Defaults", PauseButton::Defaults, d.focus == SettingsFocus::Rail(RailItem::Defaults)),
            ("Report a Bug", PauseButton::Report, d.report == Some(true)),
        ].into_iter().take(if d.report.is_some() { 3 } else { 2 }).enumerate() {
            button(sheet, &f.small, tr(label), at(i as f32 * w, layout.footer(), w, CELL), w, on, action);
        }
        settings_page(sheet, d, look, settings, selected, layout.list(), glyphs);
        if let Some((label, left, sel)) = d.keep {
            let w = layout.safe.width().min(600.0);
            let h = 3.0 * CELL;
            let y = ((layout.safe.height() - h) / 2.0).max(0.0);
            sheet.spawn((full(), Button, BackgroundColor(Color::BLACK.with_alpha(0.6)), GlobalZIndex(112)));
            sheet.spawn((from_middle(-w / 2.0, y, w, h), BackgroundColor(Color::BLACK), GlobalZIndex(113))).with_children(|panel| {
                let question = fnv_util::words::trf("Keep this {row}?", &[("row", &tr_str(label))]);
                let count = fnv_util::words::trf("Going back in {left} s.", &[("left", &left)]);
                for (i, label) in [question, count].into_iter().enumerate() {
                    let words = fit_text(|s| f.small.font.width(s), &label, w - 2.0 * PAD);
                    sm_text(panel, &f.small, &words, PAD, BASE + i as f32 * CELL, color(Shade::Bright));
                }
                for (i, label) in KEEP_ANSWERS.into_iter().enumerate() {
                    button(panel, &f.small, tr(label), at(i as f32 * w / 2.0, 2.0 * CELL, w / 2.0, CELL), w / 2.0, i == sel, PauseButton::Keep(i));
                }
            });
        }
    });
}

fn button(root: &mut ChildSpawnerCommands, face: &Face, label: &str, node: Node, w: f32, on: bool, action: PauseButton) {
    let label = fit_text(|s| face.font.width(s), label, w - 2.0 * PAD);
    let x = (w - face.font.width(&label)) / 2.0;
    root.spawn((Button, action, boxed(node, on))).with_children(|b| {
        sm_text(b, face, &label, x, crate::start_menu::base_in(face, CELL), color(Shade::Bright));
    });
}

pub(super) fn parts(width: f32, kind: ValueKind) -> RowParts {
    let right = width - PAD;
    let left = width * 0.48;
    let middle = (left + right) / 2.0;
    let mut parts = RowParts {
        label: (PAD, left - PAD), value: (left, right), text: (left, right), centre: middle,
        meter: (left, right - left), right_arrow: right - METER_ARROW.x,
    };
    match kind {
        ValueKind::List(_) => {
            parts.text = (left + VALUE_PAD, right - LIST_ARROW_STRIP - VALUE_PAD);
            parts.centre = (parts.text.0 + parts.text.1) / 2.0;
        }
        ValueKind::Slider(words) => {
            let text = words.clamp(28.0, (right - left) * 0.45);
            parts.text = (right - text, right);
            parts.centre = right - text / 2.0;
            parts.right_arrow = parts.text.0 - METER_ARROW.x - VALUE_PAD;
            parts.meter = (left + METER_ARROW.x, (parts.right_arrow - left - METER_ARROW.x).max(1.0));
        }
        _ => {}
    }
    parts
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Modal { Page, Choices(usize), Keep(usize), Listening }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target { Button(PauseButton), Body, Choices, Outside }

#[derive(Clone, Copy)]
struct Hit { target: Target, in_rows: bool, meter: Option<(f32, f32)> }

struct Contact {
    id: u64,
    origin: Vec2,
    last_y: f32,
    hit: Hit,
    intent: Intent,
    first: f32,
    scope: SettingsScope,
    modal: Modal,
    layout: Layout,
    camera: Entity,
    density: f32,
    ui_scale: f32,
}

#[derive(Default)]
pub(super) struct Action {
    pub click: Option<PauseButton>,
    pub outside: bool,
    pub first: Option<usize>,
    pub meter: Option<(RowId, f32)>,
}

#[derive(SystemParam)]
pub(super) struct Input<'w, 's> {
    state: Option<ResMut<'w, SettingsTouch>>,
    insets: Option<Res<'w, SafeInsets>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    geometry: Geometry<'w, 's>,
}

impl Input<'_, '_> {
    pub(super) fn layout(&self, tabs: usize) -> Option<Layout> {
        if !self.state.as_ref().is_some_and(|s| s.enabled()) { return None; }
        let w = self.windows.single().ok()?;
        Layout::of(w.resolution.size(), self.insets.as_deref().copied().unwrap_or_default(), tabs + 2)
    }

    pub(super) fn read(&mut self, view: &PauseView, page: Page, rows: &[ViewEntry], keep: Option<usize>, listening: bool) -> Action {
        let mut result = Action::default();
        let Some(state) = self.state.as_deref_mut() else { return result };
        let Some((layout, drawn, frame)) = view.mobile.zip(view.drawn.as_ref()).zip(state.frame.take()).map(|((l,d),f)| (l,d,f)).filter(|(layout, d, frame)| {
            d.mobile == Some(*layout) && matches!(page, Page::Settings { .. }) && matches!(d.page, Page::Settings { .. })
                && d.scope == view.scope && (listening || d.entries == rows) && d.scroll == view.scroll && d.dropdown == view.dropdown
                && d.keep.is_some() == keep.is_some() && d.listening.is_some() == listening
                && layout.matches(*frame, d.rail.len().saturating_sub(2))
        }) else {
            state.contact = None;
            state.edges.clear();
            return result;
        };
        let modal = if let Some(row) = keep { Modal::Keep(row) }
            else if listening { Modal::Listening }
            else if let Some(o) = view.dropdown { Modal::Choices(o.row) }
            else { Modal::Page };
        let first = view.dropdown.map_or(view.scroll, |o| o.first);
        let last = match modal {
            Modal::Choices(row) => drawn.list(row).map_or(0, |(n, _)| n.len().saturating_sub(layout.choices())),
            Modal::Page => rows.len().saturating_sub(view.lines()),
            _ => 0,
        };
        if frame.neutral || state.contact.as_ref().is_some_and(|c| c.scope != view.scope || c.modal != modal
            || c.layout != layout || c.camera != frame.camera || c.density != frame.density || c.ui_scale != frame.ui_scale) {
            state.contact = None;
        }
        for e in state.edges.drain(..) {
            if e.window != frame.window { continue; }
            if e.phase == TouchPhase::Started {
                if state.contact.is_none() && layout.safe.inflate(PAD).contains(e.position) {
                    let hit = self.geometry.hit(frame, e.position, modal);
                    state.contact = Some(Contact { id: e.id, origin: e.position, last_y: e.position.y, hit,
                        intent: Intent::Pending, first: first.min(last) as f32, scope: view.scope, modal, layout,
                        camera: frame.camera, density: frame.density, ui_scale: frame.ui_scale });
                } else if state.contact.as_ref().is_some_and(|c| c.id == e.id) {
                    state.contact = None;
                }
                continue;
            }
            let Some(c) = state.contact.as_mut().filter(|c| c.id == e.id) else { continue };
            if e.phase == TouchPhase::Canceled || !e.position.is_finite() {
                state.contact = None;
                continue;
            }
            c.intent.observe((e.position - c.origin).to_array(), 0);
            let ended = e.phase == TouchPhase::Ended;
            let matched = || layout.safe.inflate(PAD).contains(e.position) && self.geometry.hit(frame, e.position, modal).target == c.hit.target;
            if c.intent == Intent::Scroll && c.hit.in_rows {
                c.first = (c.first + (c.last_y - e.position.y) / CELL).clamp(0.0, last as f32);
                c.last_y = e.position.y;
                let next = c.first.round() as usize;
                result.first = (next != first).then_some(next);
            } else if let Target::Button(PauseButton::Meter(row)) = c.hit.target {
                // Repaint can replace the node each frame; re-admit its semantic row
                // and the meter still painted at the same origin before writing.
                let origin = self.geometry.hit(frame, c.origin, modal);
                if origin.target == c.hit.target && (c.intent == Intent::Assign || (ended && c.intent == Intent::Pending && matched())) {
                    if let Some((offset, per_x)) = origin.meter {
                        result.meter = Some((row, (offset + per_x * e.position.x).clamp(0.0, 1.0)));
                    }
                }
            } else if ended && c.intent == Intent::Pending && matched() {
                match c.hit.target {
                    Target::Button(b) => result.click = Some(b),
                    Target::Outside => result.outside = true,
                    _ => {}
                }
            }
            if ended {
                state.contact = None;
                // A later Start in this batch has no new painted targets yet.
                break;
            }
        }
        result
    }
}

type HitNode = (&'static ComputedNode, &'static UiGlobalTransform, &'static ComputedUiTargetCamera,
    &'static InheritedVisibility, Option<&'static CalculatedClip>, Option<&'static PauseButton>, Option<&'static Surface>);
type HitFilter = Or<(With<PauseButton>, With<Surface>)>;

#[derive(SystemParam)]
struct Geometry<'w, 's> {
    stack: Option<Res<'w, UiStack>>,
    nodes: Query<'w, 's, HitNode, HitFilter>,
}

impl Geometry<'_, '_> {
    fn hit(&self, frame: Frame, point: Vec2, modal: Modal) -> Hit {
        let physical = point * frame.density;
        if let Some(stack) = self.stack.as_ref() {
            for &entity in stack.uinodes.iter().rev() {
                let Ok((node, transform, camera, visible, clip, button, surface)) = self.nodes.get(entity) else { continue };
                if camera.get() != Some(frame.camera) || !visible.get() || !node.contains_point(*transform, physical)
                    || clip.is_some_and(|c| !c.contains_point(physical)) { continue; }
                let (target, in_rows) = match (modal, button.copied(), surface) {
                    (Modal::Keep(_), Some(b @ PauseButton::Keep(_)), _) | (Modal::Listening, Some(b @ PauseButton::Back), _) => (Target::Button(b), false),
                    (Modal::Choices(row), Some(b @ PauseButton::Choice(r, _)), _) if row == r => (Target::Button(b), true),
                    (Modal::Choices(_), _, Some(Surface::Choices)) => (Target::Choices, true),
                    (Modal::Page, Some(b), _) if !matches!(b, PauseButton::Choice(..) | PauseButton::Keep(_)) => {
                        (Target::Button(b), matches!(b, PauseButton::Row(_) | PauseButton::Meter(_) | PauseButton::SliderStep(..)))
                    }
                    (Modal::Page, _, Some(Surface::Rows)) => (Target::Body, true),
                    _ => continue,
                };
                let meter = if matches!(target, Target::Button(PauseButton::Meter(_))) {
                    node.normalize_point(*transform, Vec2::new(0.0, physical.y)).zip(node.normalize_point(*transform, Vec2::new(frame.density, physical.y)))
                        .map(|(a, b)| (a.x + 0.5, b.x - a.x))
                } else { None };
                return Hit { target, in_rows, meter };
            }
        }
        Hit { target: if matches!(modal, Modal::Choices(_)) { Target::Outside } else { Target::Body }, in_rows: false, meter: None }
    }
}

#[cfg(test)]
mod tests;
