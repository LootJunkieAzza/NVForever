use super::{original_units, set_glyph, set_marked, u, Face};
use bevy::prelude::*;
use fnv_font::{Font, Glyph, Line, Placed};
use fnv_util::text::CodePage;

#[derive(Clone, Component, Debug, PartialEq)]
enum Ink {
    Latin,
    Cyrillic,
    Single,
}

fn bitmap_face(image: Handle<Image>) -> Face {
    let mut glyphs = vec![Glyph::default(); 256];
    glyphs[b' ' as usize].after = 3.25;
    glyphs[b'A' as usize] = Glyph {
        uv: [0.125, 0.25, 0.25, 0.5],
        width: 7.25,
        height: 9.5,
        before: 0.5,
        after: 1.25,
        ascent: 8.25,
    };
    glyphs[0xc6] = Glyph {
        uv: [0.25, 0.125, 0.5, 0.375],
        width: 8.5,
        height: 11.0,
        before: 0.75,
        after: 0.5,
        ascent: 8.0,
    };
    glyphs[0xdf] = Glyph {
        uv: [0.5, 0.25, 0.625, 0.625],
        width: 6.5,
        height: 8.75,
        before: -0.25,
        after: 1.75,
        ascent: 7.5,
    };
    Face {
        font: Font {
            size: 20.0,
            texture: String::new(),
            glyphs,
            extra: default(),
        },
        image: Some((image, Vec2::new(128.0, 64.0))),
        vector: None,
    }
}

fn cyrillic_line(font: &Font) -> Line {
    // Place Windows-1251 slots as the game layout would, without changing the process code page.
    let page = CodePage::Windows1251;
    let zhe = page.code_of('Ж').unwrap();
    let ya = page.code_of('Я').unwrap();
    let ya_x = font.glyph(zhe).advance() + font.glyph(b' ').advance();
    let final_x = ya_x + font.glyph(ya).advance();
    Line {
        glyphs: vec![
            Placed {
                code: zhe,
                ch: 'Ж',
                x: 0.0,
            },
            Placed {
                code: ya,
                ch: 'Я',
                x: ya_x,
            },
            Placed {
                code: zhe,
                ch: 'Ж',
                x: final_x,
            },
        ],
        width: final_x + font.glyph(zhe).advance(),
    }
}

#[test]
fn bitmap_latin_and_cyrillic_keep_legacy_quads_and_marks() {
    let _units = original_units();
    let image = Assets::<Image>::default().add(Image::default());
    let face = bitmap_face(image.clone());
    let latin = face.font.lines("A AA", None).remove(0);
    let cyrillic = cyrillic_line(&face.font);
    let placements = |line: &Line| {
        line.glyphs
            .iter()
            .map(|g| (g.code, g.x))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        placements(&latin),
        [(b'A', 0.0), (b'A', 12.25), (b'A', 21.25)]
    );
    assert_eq!(
        placements(&cyrillic),
        [(0xc6, 0.0), (0xdf, 13.0), (0xc6, 21.0)]
    );
    assert_eq!((latin.width, cyrillic.width), (30.25, 30.75));

    let color = Color::srgb(0.25, 0.5, 0.75);
    let mut world = World::new();
    let root = world
        .commands()
        .spawn_empty()
        .with_children(|p| {
            set_marked(p, &face, &latin, 10.25, 32.5, color, Ink::Latin);
            set_marked(p, &face, &cyrillic, 40.5, 48.75, color, Ink::Cyrillic);
            p.spawn_empty().with_children(|p| {
                let space = Placed {
                    code: b' ',
                    ch: ' ',
                    x: 9.75,
                };
                for placed in [
                    &latin.glyphs[0],
                    &cyrillic.glyphs[0],
                    &space,
                    &cyrillic.glyphs[1],
                ] {
                    set_glyph(
                        p,
                        &face,
                        placed,
                        4.5 + placed.x * 1.5,
                        18.25,
                        1.5,
                        color,
                        Ink::Single,
                    );
                }
            });
        })
        .id();
    world.flush();

    let children = |parent| {
        world
            .get::<Children>(parent)
            .unwrap()
            .iter()
            .collect::<Vec<_>>()
    };
    let [latin_anchor, cyrillic_anchor, single] = children(root)[..] else {
        panic!("expected two line anchors and a group of individually placed glyphs");
    };
    for (anchor, base) in [(latin_anchor, 32.5), (cyrillic_anchor, 48.75)] {
        let node = world.get::<Node>(anchor).unwrap();
        assert_eq!(
            [node.left, node.top, node.width, node.height],
            [u(0.0), u(base), u(0.0), u(0.0)]
        );
        assert!(world.get::<LayoutConfig>(anchor).unwrap().use_rounding);
        assert_eq!(
            world.get::<UiTransform>(anchor).unwrap().translation,
            Val2::px(0.0, 0.25)
        );
        assert!(world.get::<Ink>(anchor).is_none());
    }
    let a = Rect::new(16.0, 16.0, 32.0, 32.0);
    let zhe = Rect::new(32.0, 8.0, 64.0, 24.0);
    let ya = Rect::new(64.0, 16.0, 80.0, 40.0);
    for (parent, mark, expected) in [
        (
            latin_anchor,
            Ink::Latin,
            [
                ([10.75, -8.25, 7.25, 9.5], a),
                ([23.0, -8.25, 7.25, 9.5], a),
                ([32.0, -8.25, 7.25, 9.5], a),
            ],
        ),
        (
            cyrillic_anchor,
            Ink::Cyrillic,
            [
                ([41.25, -8.0, 8.5, 11.0], zhe),
                ([53.25, -7.5, 6.5, 8.75], ya),
                ([62.25, -8.0, 8.5, 11.0], zhe),
            ],
        ),
        (
            single,
            Ink::Single,
            [
                ([5.25, 5.875, 10.875, 14.25], a),
                ([5.625, 6.25, 12.75, 16.5], zhe),
                ([23.625, 7.0, 9.75, 13.125], ya),
            ],
        ),
    ] {
        let glyphs = children(parent);
        assert_eq!(glyphs.len(), expected.len());
        for (glyph, (quad, rect)) in glyphs.into_iter().zip(expected) {
            let node = world.get::<Node>(glyph).unwrap();
            assert_eq!(node.position_type, PositionType::Absolute);
            assert_eq!([node.left, node.top, node.width, node.height], quad.map(u));
            let sprite = world.get::<ImageNode>(glyph).unwrap();
            assert_eq!(sprite.rect, Some(rect));
            assert_eq!(sprite.image, image);
            assert_eq!(sprite.color, color);
            assert!(matches!(sprite.image_mode, NodeImageMode::Stretch));
            assert!(!world.get::<LayoutConfig>(glyph).unwrap().use_rounding);
            assert_eq!(world.get::<Ink>(glyph), Some(&mark));
        }
    }
}
