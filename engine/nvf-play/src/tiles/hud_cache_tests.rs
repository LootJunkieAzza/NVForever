use super::*;
use fnv_ui::MemSource;

#[test]
fn layout_visible_prunes_hidden_subtrees_without_measuring_hidden_text() {
    let xml = r#"<menu name="M">
      <rect name="Shown"><visible>&true;</visible><text name="T1"><string>hi</string></text></rect>
      <rect name="Hidden"><visible>&false;</visible><text name="T2"><string>never</string></text></rect>
    </menu>"#;
    let src = MemSource::default().with("m.xml", xml);
    let mut ui = Ui::new();
    let root = ui.load_menu(&src, "m.xml").unwrap();
    ui.update();

    let calls = std::cell::Cell::new(0usize);
    let measure = |_font: f64, text: &str, _wrap: Option<f64>| {
        assert_ne!(text, "never", "hidden subtree text must not be measured");
        calls.set(calls.get() + 1);
        (10.0, 10.0)
    };
    let items = layout::layout_visible(&ui, root, &measure, false);
    assert_eq!(calls.get(), 1);
    assert!(items.iter().all(|i| i.visible));
    assert_eq!(items.len(), 2, "Shown rect + Shown/T1 text");
}

#[test]
fn runtime_images_revision_advances_on_set_touch_and_remove() {
    let mut images = crate::tiles::runtime_images::RuntimeImages::default();
    let r0 = images.revision();
    images.set("hud_icon", Handle::default(), Vec2::splat(16.0));
    let r1 = images.revision();
    assert!(r1 > r0);
    assert!(images.touch("hud_icon"));
    let r2 = images.revision();
    assert!(r2 > r1);
    assert!(images.remove("hud_icon"));
    let r3 = images.revision();
    assert!(r3 > r2);
    assert!(!images.remove("hud_icon"));
    assert_eq!(images.revision(), r3);
}

#[test]
fn ui_instance_id_separates_recreated_ui_trees_and_pad_glyphs_key_tracks_loads() {
    use bevy::ecs::system::RunSystemOnce;

    let a = Ui::new();
    let b = Ui::new();
    assert_ne!(a.instance_id(), b.instance_id(), "new Ui instances must never alias");
    assert_eq!(a.shape(), b.shape());
    assert_eq!(a.value_gen(), b.value_gen());

    // PadGlyphs state_key must change when glyph images load or change.
    let mut glyphs = crate::pad::PadGlyphs::default();
    let k0 = glyphs.state_key();
    let mut images = Assets::<Image>::default();
    let h1 = images.add(Image::default());
    let h2 = images.add(Image::default());
    glyphs.set_test_glyph(0, h1, 1.0);
    let k1 = glyphs.state_key();
    assert_ne!(k1, k0, "loading a glyph must change PadGlyphs::state_key");
    glyphs.set_test_glyph(0, h2, 1.5);
    assert_ne!(glyphs.state_key(), k1, "changing glyph handle/aspect must change state_key");

    // FontKit::write_widths must not alias between two fresh Ui instances with identical
    // shape/value_gen (e.g. across a save load).
    let mut world = World::new();
    world.init_resource::<TileFonts>();
    world.init_resource::<Assets<Image>>();
    let xml = r#"<menu name="HUDMainMenu"><text name="ModClock"><string>12:00</string></text></menu>"#;
    let src = MemSource::default().with("hud.xml", xml);
    let mut ui1 = Ui::new();
    ui1.load_menu(&src, "hud.xml").unwrap();
    let mut ui2 = Ui::new();
    ui2.load_menu(&src, "hud.xml").unwrap();
    assert_eq!(ui1.shape(), ui2.shape());
    assert_eq!(ui1.value_gen(), ui2.value_gen());
    assert_ne!(ui1.instance_id(), ui2.instance_id());

    world
        .run_system_once(move |mut kit: FontKit| {
            kit.write_widths(&mut ui1);
            let k1 = kit.fonts.widths_last_gen.unwrap();
            let u1 = kit.fonts.unmeasured_cache.as_ref().unwrap().0;
            assert_eq!(k1.0, ui1.instance_id());
            assert_eq!(u1, ui1.instance_id());

            kit.write_widths(&mut ui2);
            let k2 = kit.fonts.widths_last_gen.unwrap();
            let u2 = kit.fonts.unmeasured_cache.as_ref().unwrap().0;
            assert_eq!(k2.0, ui2.instance_id());
            assert_eq!(u2, ui2.instance_id());
            assert_ne!(k1, k2);
        })
        .unwrap();
}

#[test]
fn runtime_images_set_recipient_axes_invalidates_hud_cache_without_changing_stamp() {
    let xml = r#"<menu name="HUDMainMenu"><image name="LocalMap"><filename>nvf:local_map</filename></image></menu>"#;
    let src = MemSource::default().with("hud.xml", xml);
    let mut ui = Ui::new();
    let menu = ui.load_menu(&src, "hud.xml").unwrap();
    let recipient = ui.tile(menu).children[0];

    let mut images = crate::tiles::runtime_images::RuntimeImages::default();
    images.set("local_map", Handle::default(), Vec2::splat(64.0));
    let stamp0 = images.stamp("local_map");
    let rev0 = images.revision();
    let pres0 = images.presentation_revision();
    let key0 = HudCacheKey::build(&ui, menu, false, false, 0, 0, 0, Some(&images), 0, 1.0);

    // Updating recipient axes rotates the HUD minimap without bumping texture revision or stamp,
    // so presentation_revision and HudCacheKey must advance while stamp() stays unchanged.
    assert!(images.set_recipient_axes("local_map", recipient, 0.0, 1.0));
    assert_eq!(images.stamp("local_map"), stamp0);
    assert_eq!(images.revision(), rev0);
    let pres1 = images.presentation_revision();
    assert!(pres1 > pres0);
    let key1 = HudCacheKey::build(&ui, menu, false, false, 0, 0, 0, Some(&images), 0, 1.0);
    assert_ne!(key1, key0);

    // Idempotent axes update leaves presentation_revision and HudCacheKey stable.
    assert!(images.set_recipient_axes("local_map", recipient, 0.0, 1.0));
    assert_eq!(images.presentation_revision(), pres1);
    assert_eq!(HudCacheKey::build(&ui, menu, false, false, 0, 0, 0, Some(&images), 0, 1.0), key1);

    // Another rotation advances presentation_revision and invalidates HudCacheKey again.
    assert!(images.set_recipient_axes("local_map", recipient, -1.0, 0.0));
    assert_eq!(images.stamp("local_map"), stamp0);
    assert!(images.presentation_revision() > pres1);
    assert_ne!(HudCacheKey::build(&ui, menu, false, false, 0, 0, 0, Some(&images), 0, 1.0), key1);
}
