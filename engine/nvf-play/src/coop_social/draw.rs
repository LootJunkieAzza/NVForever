//! The interaction wheel's look: the weapon wheel's (`crate::weapon_wheel::draw`: its
//! matte, its slice shader, its fonts) with three slices; the invitation's Y/N prompt and
//! the duel's directions in a line under the crosshair. The trade is the barter menu's
//! screen (`super::barter`).

use super::duel::{Phase, Stance};
use super::wire::Kind;
use super::{Social, SocialMates};
use crate::interface::{amber, at, face, plain_font, Face, Pictures};
use crate::weapon_wheel::draw::{slice_uniform, slice_uniform_gap, text, SliceMaterial, FONT_1, FONT_3, HUB, INNER, LINE, MATTE, OUTER, RAISE, RIM, SIZE};
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use bevy::ui_render::prelude::MaterialNode;
use bevy::window::PrimaryWindow;
use nvf_assets::GameData;

pub fn build(app: &mut App) {
    if app.world().contains_resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>() {
        app.add_systems(Update, (draw_wheel, draw_banner).in_set(PlaySet::Ui));
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
    /// The medium font drawn smaller ([`SMALL`]): why a slice cannot be asked.
    small: Face,
    pictures: Pictures,
}

#[derive(Component)]
struct SocialWheelRoot;

/// A slice's picture side, interface units (the weapon wheel's item pictures are 132).
const ICON: f32 = 132.0;
/// A slice not picked: its picture and name this strong (the weapon wheel's).
const DIM: f32 = 0.62;

/// The slice's picture, first choice: the owner's icons, made from the owner's own
/// pictures and installed as a mod (not in this repository: Vault Boy art), white on clear
/// like the game's Pip-Boy pictures; else the game's own Vault Boy art
/// (`textures\interface\icons\pipboyimages\…`, [observed] in Fallout - Textures2.bsa).
/// Nothing is drawn when the load order has neither.
fn icons_of(kind: Kind) -> [&'static str; 2] {
    match kind {
        Kind::Duel => [r"Interface\Icons\NVForever\social_duel.dds", r"Interface\Icons\PipboyImages\Perks\perk_cowboy.dds"],
        Kind::Trade => [r"Interface\Icons\NVForever\social_trade.dds", r"Interface\Icons\PipboyImages\Perks\perk_scrounger.dds"],
        Kind::Caravan => [r"Interface\Icons\NVForever\social_caravan.dds", r"Interface\Icons\PipboyImages\Items\item_caravan_deck.dds"],
    }
}

/// How much smaller than the medium font a refusal's reason is drawn [design: owner
/// 2026-11-05, the duel slice's reason "too big"]; the game has no smaller font.
const SMALL: f32 = 0.72;

/// `face` drawn `k` times its size: every glyph's box and spacing scaled.
fn scaled(face: &Face, k: f32) -> Face {
    let mut font = face.font.clone();
    font.size *= k;
    for g in &mut font.glyphs {
        (g.width, g.height, g.before, g.after, g.ascent) = (g.width * k, g.height * k, g.before * k, g.after * k, g.ascent * k);
    }
    Face { font, image: face.image.clone() }
}

/// The gap between the wheel's slices, interface units, at the slices' inner and outer
/// edges: widening a little towards the rim, so the slices look fitted round the hub, not
/// cut as wedges [design: owner 2026-11-05, "slight so that it's not jarring"].
const SLICE_GAP: (f32, f32) = (8.0, 13.0);

/// The gap at radius `r`, widening as [`SLICE_GAP`] says (the picked slice's rim goes on
/// from its edge).
fn gap_at(r: f32) -> f32 {
    SLICE_GAP.0 + (SLICE_GAP.1 - SLICE_GAP.0) * (r - INNER) / (OUTER - INNER)
}

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
        small: scaled(&face(FONT_3, plain_font(25.0, 10.0), data, &mut images, bc), SMALL),
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
                // The spokes between the slices; the gap is cut beside them
                // ([`SLICE_GAP`]).
                let (from, to) = (mid - 60.0, mid + 60.0);
                let lit = i == s.picked;
                let refused = s.refusals[i].is_some();
                let (fill, edge) = if lit { (bright.with_alpha(if refused { 0.08 } else { 0.15 }), bright.with_alpha(0.9)) } else { (dark.with_alpha(0.6), bright.with_alpha(0.35)) };
                m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform_gap(from, to, INNER, OUTER, fill, edge, if lit { 1.5 * LINE } else { LINE }, SLICE_GAP) }))));
                if lit {
                    m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform_gap(from, to, RIM[0], RIM[1], bright, bright, 0.0, (gap_at(RIM[0]), gap_at(RIM[1]))) }))));
                }
            }
            m.spawn((at(0.0, 0.0, SIZE, SIZE), MaterialNode(materials.add(SliceMaterial { u: slice_uniform(0.0, 360.0, 0.0, HUB, dark.with_alpha(0.92), bright.with_alpha(0.63), LINE) }))));
            for (i, kind) in Kind::ALL.iter().enumerate() {
                let lit = i == s.picked;
                let refused = s.refusals[i].is_some();
                // The lower slices' pictures and names a little in towards the hub, so the
                // longest name (PLAY CARAVAN) stays well inside its slice; the top one's a
                // little out towards the rim, so a refusal's line under its name clears the
                // hub.
                let r = (INNER + OUTER) / 2.0 + if i == 0 { 14.0 } else { -26.0 };
                let p = crate::weapon_wheel::point(super::wheel::angle_of(i), r) + Vec2::splat(c);
                let strength = if lit { 1.0 } else { DIM } * if refused { 0.5 } else { 1.0 };
                let tint = bright.with_alpha(strength);
                // The game's Vault Boy picture, white on clear, tinted the HUD's colour as
                // the weapon wheel's item pictures are; under it the name, small.
                if let Some((image, rect)) = icons_of(*kind).iter().find_map(|p| kit.pictures.picture(p, data, &mut images, bc)) {
                    m.spawn((at(p.x - ICON / 2.0, p.y - ICON / 2.0 - 16.0, ICON, ICON), ImageNode { rect, color: tint, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }));
                }
                let size = kit.medium.font.size;
                text(m, &kit.medium, &kind.label().to_uppercase(), p.x, p.y + ICON / 2.0 - 14.0, 0, bright.with_alpha(strength.max(0.68)));
                if let Some(why) = &s.refusals[i] {
                    // Why it cannot be asked, in two lines when long.
                    let words: Vec<&str> = why.split(' ').collect();
                    let half = if why.len() > 32 { words.len().div_ceil(2) } else { words.len() };
                    let (a, b) = (words[..half].join(" "), words[half..].join(" "));
                    let (y, line) = (p.y + ICON / 2.0 - 14.0 + size, kit.small.font.size);
                    text(m, &kit.small, &a, p.x, y, 0, bright.with_alpha(0.6));
                    text(m, &kit.small, &b, p.x, y + line, 0, bright.with_alpha(0.6));
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
    // Under the trade's barter screen its line is the screen's own (`super::barter`).
    let trading = social.trade.as_ref().is_some_and(|t| t.opened && t.closing.is_none());
    let s = if trading { String::new() } else { banner_text(&social, &mates, real.elapsed_secs()) };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coop_social::duel::Duel;
    use crate::coop_social::{Mate, SocialMates};

    fn mates() -> SocialMates {
        SocialMates { me: Some(0), interior: false, mates: vec![Mate { id: 1, name: "Veronica".into(), reference: 1, here: true, feet: [0.0; 3], yaw: 0.0, down: false, lost: false, weapon: 0, attacks: 0, sights: false, pitch: 0.0 }] }
    }

    #[test]
    fn the_gap_widens_a_little_from_the_hub_to_the_rim() {
        use crate::weapon_wheel::draw::{slice_uniform_gap, SIZE};
        assert!((gap_at(INNER) - SLICE_GAP.0).abs() < 1e-4 && (gap_at(OUTER) - SLICE_GAP.1).abs() < 1e-4);
        assert!(gap_at(RIM[1]) > SLICE_GAP.1);
        // The shader's half gap at r (parts of the half width) is half the gap asked for.
        let u = slice_uniform_gap(0.0, 120.0, INNER, OUTER, Color::WHITE, Color::WHITE, 1.0, SLICE_GAP);
        let half = SIZE / 2.0;
        for (r, g) in [(INNER, SLICE_GAP.0), (OUTER, SLICE_GAP.1)] {
            let got = (u.line.y + u.line.z * r / half) * half * 2.0;
            assert!((got - g).abs() < 1e-3, "{r}: {got}");
        }
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
