use super::*;
use bevy::{asset::RenderAssetUsages, camera::{visibility::RenderLayers, ImageRenderTarget, RenderTarget},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    window::PrimaryWindow};
use crate::{dialogue_menu::DialogueMenu, dialogue_touch::{TouchDialogue, appearance::{Appearance, Theme}}};

#[derive(Clone, PartialEq)]
struct Key {
    kind: DeviceKind,
    window: Entity,
    display: Rect,
    density: f32,
    scale: f32,
    list: Option<crate::dialogue::ListId>,
}

#[derive(Resource, Default)]
struct Owned {
    key: Option<Key>,
    generation: u64,
    owner: Option<Entity>,
    camera: Option<Entity>,
    image: Option<Handle<Image>>,
    /// A speaker-side panel whose published glass could not fit the real text.
    refused: Option<Rect>,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Owned>().add_systems(Update, prepare
        .after(crate::PlaySet::Scripts).after(crate::dialogue_split::Resolve)
        .before(crate::PlaySet::Ui));
}

/// The side placement and the facts that decide whether its smaller glass works.
type Placement<'w> = (
    Res<'w, crate::dialogue_split::Frame>,
    Option<Res<'w, Request>>,
    Option<Res<'w, DeviceView>>,
    Option<Res<'w, crate::dialogue_touch::device_canvas::ArtCache>>,
);

#[allow(clippy::too_many_arguments)]
fn prepare(
    mut commands: Commands,
    appearance: Res<Appearance>, touch: Res<TouchDialogue>, menu: Res<DialogueMenu>,
    state: Option<Res<State<crate::PlayState>>>, paused: Option<Res<crate::pause::Paused>>,
    script: Option<Res<crate::script::ScriptState>>, creator: Option<Res<crate::creator::CreatorMenu>>,
    safe: Res<crate::touch_safe_area::SafeInsets>, scale: Option<Res<UiScale>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut images: Option<ResMut<Assets<Image>>>, mut owned: ResMut<Owned>,
    (split, request, view, art): Placement<'_>,
) {
    let kind = match appearance.theme {
        Theme::RetroConsole => Some(DeviceKind::Terminal),
        Theme::WristTerminal => Some(DeviceKind::PipBoy),
        _ => None,
    };
    // The speaker-side panel is preferred. When the published model there is
    // too small for the real device text and 48-unit rows, the same model takes
    // the whole safe area instead of dropping straight to Normal.
    if let (Some(panel), Some(request), Some(view), Some(art)) = (split.panel, request.as_deref(), view.as_deref(), art.as_deref()) {
        if owned.owner == Some(request.stamp.owner) && owned.generation == request.stamp.generation
            && request.stamp.display == crate::dialogue_split::device_display(panel, request.stamp.density)
            && view.matches(request) && art.ready(request.stamp.kind)
            && !crate::dialogue_touch::device_canvas::usable(request, view, &menu, &touch, Some(art)) {
            owned.refused = Some(panel);
        }
    }
    let key = kind.filter(|_| touch.enabled && menu.view.is_some()
        && crate::dialogue_touch::available(state.as_deref(), paused.as_deref(), script.as_deref(), creator.as_deref()))
        .and_then(|kind| {
            let (entity, window) = windows.single().ok()?;
            let density = window.scale_factor();
            let safe=crate::touch_safe_area::SafeInsets::from_array([safe.left,safe.top,safe.right,safe.bottom])?;
            let minimum=Vec2::new(safe.left,safe.top);
            let maximum=Vec2::new(window.width()-safe.right,window.height()-safe.bottom);
            if !minimum.is_finite() || !maximum.is_finite() || !maximum.cmpgt(minimum).all() {return None;}
            let logical = split.panel.filter(|panel| owned.refused != Some(*panel))
                .unwrap_or(Rect::from_corners(minimum,maximum));
            let display = crate::dialogue_split::device_display(logical, density);
            let scale = scale.as_ref().map_or(1.0, |s| s.0);
            (positive_rect(display) && density.is_finite() && density > 0.0
                && scale.is_finite() && scale > 0.0 && window.focused)
                .then(|| Key {kind, window: entity, display, density, scale, list: menu.list.clone()})
        });
    if key.is_none() { owned.refused = None; }
    if key == owned.key { return; }
    owned.generation = owned.generation.wrapping_add(1);
    owned.key = key.clone();
    let Some(key) = key else {
        if let Some(camera) = owned.camera.take() { commands.entity(camera).despawn(); }
        if let Some(owner) = owned.owner.take() { commands.entity(owner).despawn(); }
        if let Some(image) = owned.image.take() {
            if let Some(images) = images.as_deref_mut() { images.remove(image.id()); }
        }
        commands.remove_resource::<Request>();
        return;
    };
    let Some(images) = images.as_deref_mut() else {
        // A model presentation requires a real render provider; a core-only
        // synthetic App cannot turn a requested model into a flat approximation.
        owned.key = None;
        return;
    };
    let target = UVec2::new(1280, 960);
    let image = owned.image.get_or_insert_with(|| {
        let mut image = Image::new_fill(Extent3d {width: target.x, height: target.y,
            depth_or_array_layers: 1}, TextureDimension::D2, &[0, 0, 0, 255],
            TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        images.add(image)
    }).clone();
    let camera = *owned.camera.get_or_insert_with(|| commands.spawn((
        Name::new("Dialogue device content camera"), Camera2d,
        Camera { order: -4, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(ImageRenderTarget::from(image.clone())), RenderLayers::none(),
    )).id());
    let owner = *owned.owner.get_or_insert_with(|| commands.spawn(Name::new("Dialogue device surface owner")).id());
    let stamp = Stamp {owner, generation: owned.generation, window: key.window,
        kind: key.kind, display: key.display, density: key.density};
    let size = match key.kind {
        DeviceKind::Terminal => crate::terminal::screen::CANVAS,
        DeviceKind::PipBoy => crate::pipboy::CANVAS,
    };
    commands.insert_resource(Request {stamp, image, camera, target,
        canvas: Rect::from_corners(Vec2::ZERO, size)});
}
