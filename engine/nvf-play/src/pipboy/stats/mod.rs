//! The STATS page (docs/spec-notes/pipboy.md §3): Status (CND, RAD, EFF), S.P.E.C.I.A.L.,
//! Skills, Perks and General, laid out as `menus\main\stats_menu.xml`.
//!
//! Built so far: the heading. The cards' places are the file's [documented]; their
//! figures are not read yet (`--`).

use super::frame::{self, Align, Card, Ctx, Kit, TEXT};
use super::{Page, PageBody};
use crate::interface::amber;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

/// The heading's cards: x and width [documented: stats_menu.xml `stats_*_info`].
const LVL: (f32, f32) = (270.0, 100.0);
const HP: (f32, f32) = (380.0, 155.0);
const AP: (f32, f32) = (545.0, 135.0);
const XP: (f32, f32) = (690.0, 205.0);

/// Fill the STATS page's body when it is new.
pub fn draw(
    mut commands: Commands,
    bodies: Query<(Entity, &PageBody), Added<PageBody>>,
    mut kit: ResMut<Kit>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
) {
    let Some((body, b)) = bodies.iter().find(|(_, b)| b.page == Page::Stats) else { return };
    let tab = b.tab;
    let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
    commands.entity(body).with_children(|p| {
        let cards = [Card::new(LVL.0, LVL.1, "LVL", "--"), Card::new(HP.0, HP.1, "HP", "--"), Card::new(AP.0, AP.1, "AP", "--"), Card::new(XP.0, XP.1, "XP", "--")];
        frame::heading(p, &mut c, Page::Stats.title(), &cards);
        let name = Page::Stats.tabs()[tab];
        c.text(p, 2, &format!("{name}: not built yet"), 512.0, 360.0, Align::Centre, amber(TEXT));
    });
}
