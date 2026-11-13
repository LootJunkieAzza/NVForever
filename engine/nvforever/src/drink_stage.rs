//! The drinking contest's win shot (nvf-play `drink_world`, `fnv_game::drink_table::stage`):
//! while a player's win is danced, every drinker at the table sees it through the stage's
//! one camera, and a soft fill light by that camera lights the winner's front.
//!
//! - **The camera** [design]: the player camera is put on `DrinkWorld::stage`'s camera,
//!   looking at its point, each frame after every other system that places it (the
//!   third-person camera, a script's camera, and a knockdown's held view: a drinker who
//!   passed out lies held with the camera on the dancer). When the stage goes, those
//!   systems place the camera again on the next frame. While this player drinks and
//!   before a win, the camera is on `DrinkWorld::shot` the same way: the contest camera
//!   across the counter, the drinkers seen from the front (no fill light).
//! - **The fill light** [design: drink-b's co-op frames, 2026-11-12, showed the winner as a
//!   silhouette against the saloon's bright windows; the saloon's own lights over the
//!   tables are north of the table, behind the dancer. Parity, 2026-11-12: subtle, as the
//!   original's interior fill, no hotspot, nothing on the walls or floor popping on or
//!   off, faded with the dance]: a dim warm point light over the camera, [`FILL_FADE`]
//!   bright, its reach [`FILL_RADIUS`] just past the dancer, so the floor under the light
//!   and the wall behind the dancer (398 away) are out of it. It comes up over
//!   [`FADE_SECONDS`] when the stage starts and goes down as long when it ends, in the
//!   units the cells' lights use (`lights::lumens`, `display_color`). It casts no shadow,
//!   as no cell light does.
//!
//! Log lines start `drink stage:`.

use bevy::prelude::*;
use nvf_play::drink_world::DrinkWorld;
use nvf_play::{space, PlayerCamera};

/// The fill light's full brightness (a LIGH fade) and reach (game units) [design: under the
/// saloon's own lamps' 1-2; the dancer's chest is about 150 from it, the floor under it
/// 170].
pub const FILL_FADE: f32 = 0.7;
pub const FILL_RADIUS: f32 = 190.0;
/// Its colour, display values [design: a warm white, as the saloon's lamps].
pub const FILL_COLOR: [f32; 3] = [1.0, 0.9, 0.75];
/// How long it takes to come up and to go down (seconds) [design].
pub const FADE_SECONDS: f32 = 0.8;

/// The fill light: its entity, how far up it is (0..1) and where it hangs.
#[derive(Resource, Default)]
struct StageLight {
    entity: Option<Entity>,
    level: f32,
    at: [f32; 3],
}

#[derive(Component)]
struct StageFill;

pub struct DrinkStagePlugin;

impl Plugin for DrinkStagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StageLight>().add_systems(Update, (light, camera.after(nvf_play::PlaySet::Physics).after(crate::player_knockdown::follow)));
    }
}

/// The player camera on the stage's camera while a win is danced, else on the contest's
/// camera while this player drinks.
fn camera(world: Option<Res<DrinkWorld>>, mut cams: Query<&mut Transform, With<PlayerCamera>>) {
    let Some(view) = world.as_deref().and_then(|w| w.stage.or(w.shot)) else { return };
    let Ok(mut t) = cams.single_mut() else { return };
    let want = Transform::from_translation(space::to_bevy(view.camera)).looking_at(space::to_bevy(view.look_at), Vec3::Y);
    if want.translation.is_finite() && want.rotation.is_finite() {
        t.translation = want.translation;
        t.rotation = want.rotation;
    }
}

/// The fill light faded up with the stage and down after it.
fn light(mut commands: Commands, time: Res<Time>, world: Option<Res<DrinkWorld>>, mut st: ResMut<StageLight>, mut fills: Query<(&mut Transform, &mut PointLight), With<StageFill>>) {
    let view = world.as_deref().and_then(|w| w.stage);
    if view.is_none() && st.entity.is_none() {
        return;
    }
    if let Some(v) = view {
        st.at = v.light;
    }
    let target = if view.is_some() { 1.0 } else { 0.0 };
    let step = time.delta_secs() / FADE_SECONDS;
    let was = st.level;
    st.level = if target > was { (was + step).min(1.0) } else { (was - step).max(0.0) };
    let full = crate::lights::lumens(FILL_FADE, crate::sky::light_unit());
    // Smooth at both ends.
    let k = st.level * st.level * (3.0 - 2.0 * st.level);
    match st.entity {
        None => {
            let e = commands
                .spawn((
                    Name::new("Drinking contest fill light"),
                    StageFill,
                    PointLight { color: crate::lights::display_color(FILL_COLOR), intensity: full * k, range: FILL_RADIUS, shadow_maps_enabled: false, ..default() },
                    Transform::from_translation(space::to_bevy(st.at)),
                ))
                .id();
            st.entity = Some(e);
            let [x, y, z] = st.at;
            info!("drink stage: the fill light comes up at ({x:.0}, {y:.0}, {z:.0}) over {FADE_SECONDS}s, fade {FILL_FADE}, reach {FILL_RADIUS}");
        }
        Some(e) if st.level <= 0.0 && view.is_none() => {
            commands.entity(e).try_despawn();
            st.entity = None;
            info!("drink stage: the fill light is down");
        }
        Some(e) => {
            if let Ok((mut t, mut l)) = fills.get_mut(e) {
                t.translation = space::to_bevy(st.at);
                l.intensity = full * k;
            }
        }
    }
}
