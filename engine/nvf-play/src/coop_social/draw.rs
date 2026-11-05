//! The interaction wheel's look: the weapon wheel's (`crate::weapon_wheel::draw`: its
//! matte, its slice shader, its fonts) with three slices; the invitation's Y/N prompt and
//! the duel's directions in a line under the crosshair; the trade panel.

use super::duel::{Phase, Stance};
use super::trade::AskState;
use super::wire::Kind;
use super::{Social, SocialMates};
use crate::interface::{amber, at, face, plain_font, Face, Pictures};
use crate::sim::SimState;
use crate::weapon_wheel::draw::{slice_uniform, text, SliceMaterial, FONT_1, FONT_3, GAP, HUB, INNER, LINE, MATTE, OUTER, RAISE, RIM, SIZE};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::ui_render::prelude::MaterialNode;
use bevy::window::PrimaryWindow;
use fnv_sim::FormId;
use nvf_assets::GameData;

pub fn build(app: &mut App) {
    if app.world().contains_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>() {
        app.add_systems(Update, (draw_wheel, draw_banner, draw_trade).in_set(PlaySet::Ui));
    }
}

/// What the wheel shows.
#[derive(Debug, Clone, PartialEq)]
struct Shown {
    picked: usize,
    name: String,
    refusals: [Option<String>; 3],
    width: i32,
}

struct Kit {
    large: Face,
    medium: Face,
    pictures: Pictures,
}

#[derive(Component)]
struct SocialWheelRoot;

#[allow(clippy::too_many_arguments)]
fn draw_wheel(
    mut commands: Commands,
    social: Res<Social>,
    roots: Query<Entity, With<SocialWheelRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (data, formats): (Option<Res<GameData>>, Option<Res<CompressedImageFormatSupport>>),
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SliceMaterial>>,
    mut last: Local<Option<Shown>>,
    mut kit: Local<Option<Kit>>,
) {
    let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let now = social.wheel.as_ref().map(|w| Shown { picked: w.picked, name: w.name.clone(), refusals: w.refusals.clone(), width: width.round() as i32 });
    if now == *last {
        return;
    }
    *last = now.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(s) = now else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let data = data.as_deref();
    let kit = kit.get_or_insert_with(|| Kit {
        large: face(FONT_1, plain_font(31.0, 12.0), data, &mut images, bc),
        medium: face(FONT_3, plain_font(25.0, 10.0), data, &mut images, bc),
        pictures: Pictures::load(data),
    });
    let bright = amber(255.0);
    let dark = Color::srgb(0.08, 0.055, 0.025);
    let left = (width - SIZE) / 2.0;
    let top = (960.0 - SIZE) / 2.0 - RAISE;
    let c = SIZE / 2.0;
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    let matte = kit.pictures.picture(MATTE, data, &mut images, bc);
    commands.spawn((SocialWheelRoot, full, BackgroundColor(Color::BLACK.with_alpha(crate::weapon_wheel::DARKEN)), GlobalZIndex(105))).with_children(|r| {
        r.spawn(at(left, top, SIZE, SIZE)).with_children(|m| {
            if let Some((image, rect)) = matte {
                m.spawn((at(0.0, 0.0, SIZE, SIZE), ImageNode { rect, color: Color::BLACK.with_alpha(0.88), image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
            }
            for i in 0..3 {
                let mid = super::wheel::angle_of(i);
                let (from, to) = (mid - 60.0 + GAP, mid + 60.0 - GAP);
                let lit = i == s.picked;
                let refused = s.refusals[i].is_some();
                let (fill, edge) = if lit { (bright.with_alpha(if refused { 0.12 } else { 0.27 }), bright.with_alpha(0.9)) } else { (dark.with_alpha(0.6), bright.with_alpha(0.35)) };
                m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform(from, to, INNER, OUTER, fill, edge, if lit { 1.5 * LINE } else { LINE }) }))));
                if lit {
                    m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform(from, to, RIM[0], RIM[1], bright, bright, 0.0) }))));
                }
            }
            m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform(0.0, 360.0, 0.0, HUB, dark.with_alpha(0.92), bright.with_alpha(0.63), LINE) }))));
            for (i, kind) in Kind::ALL.iter().enumerate() {
                let lit = i == s.picked;
                let refused = s.refusals[i].is_some();
                let p = crate::weapon_wheel::point(super::wheel::angle_of(i), (INNER + OUTER) / 2.0) + Vec2::splat(c);
                let strength = if lit { 1.0 } else { 0.62 } * if refused { 0.55 } else { 1.0 };
                text(m, &kit.large, &kind.label().to_uppercase(), p.x, p.y - kit.large.font.size / 2.0 - if refused { 14.0 } else { 0.0 }, 0, bright.with_alpha(strength));
                if let Some(why) = &s.refusals[i] {
                    // The reason in two lines when long.
                    let words: Vec<&str> = why.split(' ').collect();
                    let half = words.len().div_ceil(2);
                    let (a, b) = (words[..half].join(" "), words[half..].join(" "));
                    text(m, &kit.medium, &a, p.x, p.y + 12.0, 0, bright.with_alpha(strength.max(0.6)));
                    text(m, &kit.medium, &b, p.x, p.y + 12.0 + kit.medium.font.size, 0, bright.with_alpha(strength.max(0.6)));
                }
            }
            text(m, &kit.medium, "INTERACT", c, c - 40.0, 0, bright.with_alpha(0.7));
            text(m, &kit.large, &s.name, c, c - 10.0, 0, bright);
        });
        let hint = "Mouse: pick      Click or E: ask      Esc: cancel";
        text(r, &kit.medium, hint, width / 2.0, top + SIZE - 10.0, 0, bright.with_alpha(0.8));
    });
}

#[derive(Component)]
struct Banner;

/// The line under the crosshair: the invitation's prompt first, then the duel's
/// direction, then the last thing said.
fn banner_text(social: &Social, mates: &SocialMates, now: f32) -> String {
    if let Some(i) = social.invites.shown() {
        let name = mates.get(i.with).map_or_else(|| format!("Player {}", i.with + 1), |m| m.name.clone());
        return format!("{}\nY) Accept      N) Decline", super::invite::invite_line(&name, i.kind));
    }
    let said = social.banner.as_ref().filter(|(_, until)| now < *until).map(|(l, _)| l.clone());
    if let Some(d) = social.duel.as_ref() {
        let name = mates.get(d.with).map_or_else(|| format!("Player {}", d.with + 1), |m| m.name.clone());
        match d.phase {
            // The countdown is never shown, nor anything else meanwhile.
            Phase::Countdown { .. } => return String::new(),
            Phase::Positioning => {
                let dir = d.mine.hint(&name).unwrap_or_else(|| format!("In position: wait for {name}, then for the signal to draw"));
                return match said.filter(|l| *l != dir && d.mine != Stance::Ready) {
                    Some(l) if l.starts_with("The duel") => l,
                    _ => format!("DUEL WITH {}\n{dir}", name.to_uppercase()),
                };
            }
            Phase::Draw { .. } => return "DRAW!".into(),
            _ => {}
        }
    }
    said.unwrap_or_default()
}

fn draw_banner(mut commands: Commands, real: Res<Time<Real>>, social: Res<Social>, mates: Res<SocialMates>, mut q: Query<(&mut Text, &mut Visibility), With<Banner>>) {
    let s = banner_text(&social, &mates, real.elapsed_secs());
    match q.single_mut() {
        Ok((mut t, mut vis)) => {
            if t.0 != s {
                t.0 = s.clone();
            }
            vis.set_if_neq(if s.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
        }
        Err(_) if !s.is_empty() => {
            commands
                .spawn((Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), top: Val::Percent(58.0), justify_content: JustifyContent::Center, ..default() }, GlobalZIndex(40)))
                .with_children(|p| {
                    p.spawn((
                        Banner,
                        Node { padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)), ..default() },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                        Text::new(s),
                        TextFont { font_size: FontSize::Px(20.0), ..default() },
                        TextColor(amber(255.0)),
                    ));
                });
        }
        Err(_) => {}
    }
}

#[derive(Component)]
struct TradePanel;

/// The trade panel's three columns: their items, this player's asks, what they asked of
/// this player; the picked column's title and row marked.
fn trade_text(social: &Social, sim: &SimState) -> Option<[String; 4]> {
    let t = social.trade.as_ref().filter(|t| t.closing.is_none())?;
    let name = |f: u32| sim.name(FormId(f)).map_or_else(|| FormId(f).to_string(), str::to_string);
    let mark = |panel: usize, row: usize| if t.panel == panel && t.rows[panel] == row { "> " } else { "  " };
    let title = |panel: usize, s: &str| if t.panel == panel { format!("[ {s} ]") } else { format!("  {s}") };
    let mut a = vec![title(0, &format!("{}'s items", t.name)), String::new()];
    if t.book.stock.is_empty() {
        a.push("  (nothing yet)".into());
    }
    for (i, &(f, n)) in t.book.stock.iter().enumerate() {
        let ask = if t.panel == 0 && t.rows[0] == i { format!("   ask {} -/+", t.count) } else { String::new() };
        a.push(format!("{}{} ({n}){ask}", mark(0, i), name(f)));
    }
    let mut b = vec![title(1, "Your requests"), String::new()];
    for (i, ask) in t.book.asks.iter().enumerate() {
        let state = match ask.state {
            AskState::Waiting => "waiting for a price".to_string(),
            AskState::Priced(each) => format!("{each} each, {} caps: Enter buys", each.saturating_mul(ask.count as u32)),
            AskState::Declined => "not for sale".to_string(),
            AskState::Buying(_) => "buying…".to_string(),
            AskState::Bought => "bought".to_string(),
        };
        b.push(format!("{}{} × {}: {state}", mark(1, i), ask.count, name(ask.item)));
    }
    let mut c = vec![title(2, &format!("Asked of you by {}", t.name)), String::new()];
    for (i, o) in t.book.offers.iter().enumerate() {
        let sent = match o.sent {
            Some(p) if p == o.each => " (sent)",
            Some(_) => " (changed: Enter sends)",
            None => " (Enter sends)",
        };
        c.push(format!("{}{} × {}: {} caps each{sent}", mark(2, i), o.count, name(o.item), o.each));
    }
    let caps = super::caps_of(sim);
    let foot = format!("Your caps: {caps}      Left/Right: column   Up/Down: item   -/+: count or price (Shift: 10)   Enter: ask, buy or price   X: decline   Esc: leave");
    Some([a.join("\n"), b.join("\n"), c.join("\n"), foot])
}

fn draw_trade(mut commands: Commands, social: Res<Social>, sim: Res<SimState>, roots: Query<Entity, With<TradePanel>>, mut last: Local<Option<[String; 4]>>) {
    let now = trade_text(&social, &sim);
    if now == *last {
        return;
    }
    *last = now.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(cols) = now else { return };
    let font = TextFont { font_size: FontSize::Px(16.0), ..default() };
    commands
        .spawn((
            TradePanel,
            Node { position_type: PositionType::Absolute, left: Val::Percent(6.0), right: Val::Percent(6.0), top: Val::Percent(10.0), bottom: Val::Percent(14.0), flex_direction: FlexDirection::Column, padding: UiRect::all(Val::Px(16.0)), row_gap: Val::Px(10.0), ..default() },
            BackgroundColor(Color::srgba(0.02, 0.015, 0.005, 0.86)),
            BorderColor::all(amber(160.0)),
            GlobalZIndex(106),
        ))
        .with_children(|p| {
            p.spawn((Text::new("TRADE"), TextFont { font_size: FontSize::Px(24.0), ..default() }, TextColor(amber(255.0))));
            p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(24.0), flex_grow: 1.0, ..default() }).with_children(|row| {
                for col in &cols[..3] {
                    row.spawn((Node { width: Val::Percent(33.0), overflow: Overflow::clip(), ..default() }, Text::new(col.clone()), font.clone(), TextColor(amber(230.0))));
                }
            });
            p.spawn((Text::new(cols[3].clone()), TextFont { font_size: FontSize::Px(14.0), ..default() }, TextColor(amber(200.0))));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coop_social::duel::Duel;
    use crate::coop_social::{Mate, SocialMates};

    fn mates() -> SocialMates {
        SocialMates { me: Some(0), interior: false, mates: vec![Mate { id: 1, name: "Veronica".into(), reference: 1, here: true, feet: [0.0; 3], yaw: 0.0, down: false, lost: false }] }
    }

    #[test]
    fn the_banner_shows_the_prompt_then_the_duels_directions() {
        let m = mates();
        let mut s = Social::default();
        let mut sink = Vec::new();
        s.invites.heard(0, 1, Kind::Trade, 1 << 12 | 1, 0.0, false, &mut sink);
        assert_eq!(banner_text(&s, &m, 0.0), "Veronica would like to trade with you\nY) Accept      N) Decline");
        s.invites.asked.clear();
        let mut d = Duel::new(0, 1, 1, true, 0.0);
        d.step(0.0, Stance::Facing, "Veronica", 0);
        s.duel = Some(d);
        assert_eq!(banner_text(&s, &m, 0.0), "DUEL WITH VERONICA\nTurn your back to Veronica");
        s.duel.as_mut().unwrap().phase = Phase::Countdown { draw_at: 3.0 };
        s.banner = Some(("anything".into(), 99.0));
        assert_eq!(banner_text(&s, &m, 1.0), "", "the countdown is never shown");
    }
}
