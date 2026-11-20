//! The interaction wheel's look: the weapon wheel's (`crate::weapon_wheel::draw`: its
//! matte, its slice shader, its fonts) with three slices; the invitation's Y/N prompt and
//! the duel's directions in a line under the crosshair. The trade is the barter menu's
//! screen (`super::barter`).

use super::duel::{Phase, Stance};
use super::wire::Kind;
use super::{Social, SocialMates};
use crate::interface::{amber, at, face, plain_font, set_line, Face, Pictures};
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
    /// The Activate control's key or button, as the prompts name it.
    key: String,
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
    bindings: Option<Res<crate::bindings::Bindings>>,
    mut last: Local<Option<Shown>>,
    mut kit: Local<Option<Kit>>,
) {
    let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    // Asking is the Activate control (`coop_social::input`): named as it is bound, as the
    // crosshair's prompts name it ("F)").
    let key = bindings.as_deref().map_or_else(|| "E".to_string(), |b| b.prompt(crate::bindings::Control::Activate).trim_end_matches(')').to_string());
    let now = social.wheel.as_ref().map(|w| Shown { picked: w.picked, name: w.name.clone(), refusals: w.refusals.clone(), width: width.round() as i32, key: key.clone() });
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
        small: crate::interface::scaled_face(&face(FONT_3, plain_font(25.0, 10.0), data, &mut images, bc), SMALL),
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
        let hint = format!("Mouse: pick      Click or {}: ask      Esc: cancel", s.key);
        text(r, &kit.medium, &hint, width / 2.0, top + SIZE - 10.0, 0, bright.with_alpha(0.8));
    });
}

#[derive(Component)]
struct BannerRoot;

/// What the line under the crosshair shows: a small message box [design: the user's
/// pick of 2026-11-06, mockup 1B], its title in the menus' glow Monofonto, and under a
/// fading rule either the keys of an invitation or one more line in the HUD's font.
#[derive(Debug, Clone, PartialEq)]
struct BannerShown {
    title: String,
    lower: Lower,
}

#[derive(Debug, Clone, PartialEq)]
enum Lower {
    None,
    /// "Y) Accept" and "N) Decline", side by side.
    Keys,
    Line(String),
}

/// The story wait's prompts [design: owner 2026-11-09, "on the hosts end it'll say waiting
/// for players, and then give guests only the option to teleport"].
const STORY_HOST: &str = "WAITING FOR PLAYERS...";
const STORY_GUEST_KEY: &str = "Y) Teleport to the host";
/// The join question for joint progression (plan §3.7) [design: our own words].
const JOINT_ASK: &str = "SHARE QUEST PROGRESS THIS SESSION?";

/// The invitation's keys, as the input reads them (`super::input`).
const ACCEPT: &str = "Y) Accept";
const DECLINE: &str = "N) Decline";

/// Mockup 1B's measures, interface units under the view's middle: the backing's top and
/// bottom (with and without a lower line), the title's, the rule's and the lower line's
/// base, the room beside the widest line, the gap between the keys [design: mock.py's
/// `soft_backing(…, 160, …, 330, 0.85, 40)`, `big.draw(…, 215)`, `rule(tw + 40)` at 240,
/// the keys at 295, 90 apart].
const BOX_TOP: f32 = 160.0;
const BOX_BOTTOM: f32 = 330.0;
const BOX_BOTTOM_TITLE_ONLY: f32 = 255.0;
const TITLE_BASE: f32 = 215.0;
const RULE_Y: f32 = 240.0;
const LOWER_BASE: f32 = 295.0;
const BOX_SIDE: f32 = 60.0;
const KEY_GAP: f32 = 90.0;
const RULE_OVER: f32 = 20.0;
const BACKING: f32 = 0.85;
const FEATHER: f32 = 40.0;
/// The decline key a little dimmer than the accept key [design: mock.py's alpha 0.8].
const DECLINE_DIM: f32 = 0.8;
const RULE_ALPHA: f32 = 0.8;

/// The HUD's own font (rollover prompts, corner messages) [documented: Fallout_default.ini
/// `sFontFile_7`].
const HUD_FONT: &str = crate::interface::FONT_FILES[6];

/// The line under the crosshair: the invitation first, then the duel's direction, then
/// the last thing said.
fn banner(social: &Social, mates: &SocialMates, now: f32) -> Option<BannerShown> {
    if let Some(i) = social.invites.shown() {
        let name = mates.get(i.with).map_or_else(|| format!("Player {}", i.with + 1), |m| m.name.clone());
        return Some(BannerShown { title: super::invite::invite_line(&name, i.kind), lower: Lower::Keys });
    }
    if social.joint_ask {
        return Some(BannerShown { title: JOINT_ASK.into(), lower: Lower::Keys });
    }
    match &social.story {
        Some(super::StoryPrompt::Host) => return Some(BannerShown { title: STORY_HOST.into(), lower: Lower::None }),
        Some(super::StoryPrompt::Guest { host }) => return Some(BannerShown { title: format!("{} IS WAITING FOR YOU", host.to_uppercase()), lower: Lower::Line(STORY_GUEST_KEY.into()) }),
        None => {}
    }
    let said = social.banner.as_ref().filter(|(_, until)| now < *until).map(|(l, _)| l.clone());
    if let Some(d) = social.duel.as_ref() {
        let name = mates.get(d.with).map_or_else(|| format!("Player {}", d.with + 1), |m| m.name.clone());
        match d.phase {
            // The countdown is never shown, nor anything else meanwhile.
            Phase::Countdown { .. } => return None,
            Phase::Staking => {
                let (title, lower) = d.stake_lines(&name);
                return Some(BannerShown { title, lower: Lower::Line(lower) });
            }
            Phase::Positioning => {
                let dir = d.mine.hint(&name).unwrap_or_else(|| format!("In position: wait for {name}, then for the signal to draw"));
                return Some(match said.filter(|l| *l != dir && d.mine != Stance::Ready) {
                    Some(l) if l.starts_with("The duel") => BannerShown { title: l, lower: Lower::None },
                    _ => BannerShown { title: format!("DUEL WITH {}", name.to_uppercase()), lower: Lower::Line(dir) },
                });
            }
            Phase::Draw { .. } => return Some(BannerShown { title: "DRAW!".into(), lower: Lower::None }),
            _ => {}
        }
    }
    said.filter(|l| !l.is_empty()).map(|title| BannerShown { title, lower: Lower::None })
}

struct BannerKit {
    title: Face,
    hud: Face,
    rule: Handle<Image>,
}

#[allow(clippy::too_many_arguments)]
fn draw_banner(
    mut commands: Commands,
    real: Res<Time<Real>>,
    social: Res<Social>,
    mates: Res<SocialMates>,
    roots: Query<Entity, With<BannerRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    (data, formats): (Option<Res<GameData>>, Option<Res<CompressedImageFormatSupport>>),
    mut images: ResMut<Assets<Image>>,
    mut last: Local<Option<(BannerShown, i32)>>,
    mut kit: Local<Option<BannerKit>>,
) {
    // Under the trade's barter screen its line is the screen's own (`super::barter`).
    let trading = social.trade.as_ref().is_some_and(|t| t.opened && t.closing.is_none());
    let width = windows.single().map_or(1280.0, |w| 960.0 * w.width() / w.height().max(1.0));
    let now = if trading { None } else { banner(&social, &mates, real.elapsed_secs()).map(|s| (s, width.round() as i32)) };
    if now == *last {
        return;
    }
    *last = now.clone();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some((s, _)) = now else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let data = data.as_deref();
    let kit = kit.get_or_insert_with(|| BannerKit {
        title: face(FONT_1, plain_font(31.0, 12.0), data, &mut images, bc),
        hud: face(HUD_FONT, plain_font(25.0, 11.0), data, &mut images, bc),
        rule: images.add(crate::interface::fading_rule_image()),
    });
    // The game's fonts have no ellipsis: three dots instead.
    let line_of = |f: &Face, t: &str| f.font.lines(&t.trim().replace('\u{2026}', "..."), None).into_iter().next();
    let Some(title) = line_of(&kit.title, &s.title) else { return };
    let lower_w = match &s.lower {
        Lower::None => 0.0,
        Lower::Keys => kit.hud.font.width(ACCEPT) + KEY_GAP + kit.hud.font.width(DECLINE),
        Lower::Line(l) => kit.hud.font.width(l),
    };
    let widest = title.width.max(lower_w);
    let (cx, cy) = (width / 2.0, 480.0);
    let bottom = if s.lower == Lower::None { BOX_BOTTOM_TITLE_ONLY } else { BOX_BOTTOM };
    let (bw, bh) = (widest + 2.0 * BOX_SIDE, bottom - BOX_TOP);
    let backing = images.add(crate::interface::soft_backing_image(bw, bh, FEATHER, BACKING));
    let bright = amber(255.0);
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() };
    commands.spawn((BannerRoot, full, GlobalZIndex(40))).with_children(|r| {
        r.spawn((crate::interface::soft_backing_node(cx - bw / 2.0, cy + BOX_TOP, bw, bh, FEATHER), ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(backing) }));
        set_line(r, &kit.title, &title, cx - title.width / 2.0, cy + TITLE_BASE, bright);
        if s.lower == Lower::None {
            return;
        }
        let rw = widest + 2.0 * RULE_OVER;
        r.spawn((at(cx - rw / 2.0, cy + RULE_Y - 1.0, rw, 2.0), ImageNode { color: bright.with_alpha(RULE_ALPHA), image_mode: NodeImageMode::Stretch, ..ImageNode::new(kit.rule.clone()) }));
        let lower = |r: &mut ChildSpawnerCommands, t: &str, x: f32, a: f32| {
            if let Some(l) = line_of(&kit.hud, t) {
                set_line(r, &kit.hud, &l, x, cy + LOWER_BASE, bright.with_alpha(a));
            }
        };
        match &s.lower {
            Lower::Keys => {
                let left = cx - lower_w / 2.0;
                lower(r, ACCEPT, left, 1.0);
                lower(r, DECLINE, left + kit.hud.font.width(ACCEPT) + KEY_GAP, DECLINE_DIM);
            }
            Lower::Line(l) => lower(r, l, cx - lower_w / 2.0, 1.0),
            Lower::None => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coop_social::duel::Duel;
    use crate::coop_social::{Mate, SocialMates};

    fn mates() -> SocialMates {
        SocialMates { me: Some(0), interior: false, mates: vec![Mate { id: 1, name: "Veronica".into(), reference: 1, here: true, feet: [0.0; 3], yaw: 0.0, down: false, body: None, lost: false, weapon: 0, attacks: 0, sights: false, pitch: 0.0 }] }
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
        assert_eq!(banner(&s, &m, 0.0), Some(BannerShown { title: "Veronica would like to trade".into(), lower: Lower::Keys }));
        s.invites.asked.clear();
        let mut d = Duel::new(0, 1, 1, true, 0.0);
        d.step(0.0, Stance::Facing, "Veronica", 0);
        s.duel = Some(d);
        assert_eq!(banner(&s, &m, 0.0), Some(BannerShown { title: "DUEL WITH VERONICA".into(), lower: Lower::Line("Turn your back to Veronica".into()) }));
        s.duel.as_mut().unwrap().phase = Phase::Countdown { draw_at: 3.0 };
        s.banner = Some(("anything".into(), 99.0));
        assert_eq!(banner(&s, &m, 1.0), None, "the countdown is never shown");
    }

    #[test]
    fn the_story_wait_shows_under_an_invitation_with_no_decline() {
        let m = mates();
        let mut s = Social { story: Some(crate::coop_social::StoryPrompt::Host), ..Social::default() };
        assert_eq!(banner(&s, &m, 0.0), Some(BannerShown { title: "WAITING FOR PLAYERS...".into(), lower: Lower::None }));
        s.story = Some(crate::coop_social::StoryPrompt::Guest { host: "Courier".into() });
        assert_eq!(banner(&s, &m, 0.0), Some(BannerShown { title: "COURIER IS WAITING FOR YOU".into(), lower: Lower::Line("Y) Teleport to the host".into()) }));
        // An invitation goes first; the story prompt is back when it is answered.
        let mut sink = Vec::new();
        s.invites.heard(0, 1, Kind::Trade, 1 << 12 | 1, 0.0, false, &mut sink);
        assert_eq!(banner(&s, &m, 0.0).map(|b| b.lower), Some(Lower::Keys));
        s.invites.asked.clear();
        assert_eq!(banner(&s, &m, 0.0).map(|b| b.lower), Some(Lower::Line("Y) Teleport to the host".into())));
    }

    #[test]
    fn the_backing_fades_out_past_its_box_and_the_rule_towards_its_ends() {
        let img = crate::interface::soft_backing_image(400.0, 200.0, 40.0, 0.85);
        let (w, h) = (img.width() as usize, img.height() as usize);
        let alpha = |x: usize, y: usize| img.data.as_ref().unwrap()[(y * w + x) * 4 + 3];
        assert_eq!((w, h), (560, 360), "the box and 2 feathers around it");
        assert!((alpha(w / 2, h / 2) as f32 - 0.85 * 255.0).abs() < 2.0, "full in the middle");
        assert!(alpha(0, 0) < 3 && alpha(w / 2, 0) < 3, "nothing at the image's edge: no border");
        let edge = alpha(w / 2, 80);
        assert!((100..=117).contains(&edge), "about half on the box's own edge: {edge}");
        let rule = crate::interface::fading_rule_image();
        let a = |x: usize| rule.data.as_ref().unwrap()[x * 4 + 3];
        assert!(a(0) < 10 && a(63) < 10 && a(32) == 255 && a(16) == 255 && a(8) < 200);
    }
}
