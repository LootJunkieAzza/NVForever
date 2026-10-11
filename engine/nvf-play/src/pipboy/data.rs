//! The DATA page (docs/spec-notes/pipboy.md §5): Local Map, World Map, Quests, Misc
//! (notes) and Radio, laid out as `menus\main\map_menu.xml`.
//!
//! Built so far: the heading's title. The location and the date and time in its cards
//! are not drawn yet.

use super::frame::{self, Align, Ctx, Kit, TEXT};
use super::{Page, PageBody};
use crate::interface::amber;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use nvf_assets::GameData;

/// Fill the DATA page's body when it is new.
pub fn draw(
    mut commands: Commands,
    bodies: Query<(Entity, &PageBody), Added<PageBody>>,
    mut kit: ResMut<Kit>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
) {
    let Some((body, b)) = bodies.iter().find(|(_, b)| b.page == Page::Data) else { return };
    let tab = b.tab;
    let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
    commands.entity(body).with_children(|p| {
        frame::heading(p, &mut c, Page::Data.title(), &[]);
        let name = Page::Data.tabs()[tab];
        c.text(p, 2, &format!("{name}: not built yet"), 512.0, 360.0, Align::Centre, amber(TEXT));
    });
}
