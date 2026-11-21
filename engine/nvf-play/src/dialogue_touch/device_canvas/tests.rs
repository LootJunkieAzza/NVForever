use super::*;
use crate::dialogue_device::ScreenMap;
use crate::dialogue_touch::compact_tests::{layout_scene, rows, send, settle};
use std::sync::Arc;

struct Glass { window: Rect, canvas: Rect }
impl ScreenMap for Glass {
    fn canvas_at(&self, point: Vec2) -> Option<Vec2> {
        self.window.contains(point).then(|| self.canvas.min
            + (point - self.window.min) * self.canvas.size() / self.window.size())
    }
    fn window_at(&self, point: Vec2) -> Option<Vec2> {
        self.canvas.contains(point).then(|| self.window.min
            + (point - self.canvas.min) * self.window.size() / self.canvas.size())
    }
}

#[test]
fn wrist_terminal_uses_published_glass_for_actual_painted_choice_taps_without_an_atlas() {
    let (mut app, window) = layout_scene(true, Vec2::new(792.0, 363.0), 1.0);
    app.init_resource::<Assets<Image>>().init_resource::<crate::pad::PadGlyphs>();
    app.world_mut().resource_mut::<TouchDialogue>().confirm = false;
    let i = app.world().resource::<crate::settings::Settings>().index(appearance::THEME_KEY).unwrap();
    assert!(crate::settings::pick(app.world_mut(), i, "Wrist Terminal"));
    settle(&mut app);
    let request = app.world().resource::<Request>().clone();
    let glass = Rect::from_center_size(request.stamp.display.center(), Vec2::new(320.0, 240.0));
    let camera = app.world_mut().spawn(Camera3d::default()).id();
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(Image::transparent());
    app.insert_resource(DeviceView { stamp: request.stamp, image, camera,
        scale: 240.0 / request.canvas.height(), map: Arc::new(Glass { window: glass, canvas: request.canvas }) });
    settle(&mut app);
    let world = app.world_mut();
    let mut output = world.query_filtered::<(&ImageNode, &ComputedNode), With<Display>>();
    assert!(output.single(world).unwrap().1.size().min_element() > 0.0);
    let canvas_point = rows(&mut app)[0].1.center();
    let point = app.world().resource::<DeviceView>().window_at(&request, canvas_point).unwrap();
    send(&mut app, window, 41, TouchPhase::Started, point);
    send(&mut app, window, 41, TouchPhase::Ended, point);
    app.update();
    assert_eq!(app.world().resource::<TouchFrame>().asked.pick, Some(0));
}
