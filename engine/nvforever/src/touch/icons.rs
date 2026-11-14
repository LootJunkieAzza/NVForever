//! Original geometric control icons, rasterized at startup; no external artwork.
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
#[derive(Clone, Copy)]
enum Shape {
    Line(Vec2, Vec2),
    Circle(Vec2, f32),
}
fn line(shapes: &mut Vec<Shape>, a: (f32, f32), b: (f32, f32)) {
    shapes.push(Shape::Line(Vec2::from(a), Vec2::from(b)));
}
fn circle(shapes: &mut Vec<Shape>, x: f32, y: f32, r: f32) {
    shapes.push(Shape::Circle(Vec2::new(x, y), r));
}
fn arrow(s: &mut Vec<Shape>, a: (f32, f32), b: (f32, f32)) {
    line(s, a, b);
    let p = Vec2::from(b);
    let d = (Vec2::from(a) - p).normalize_or_zero();
    let n = Vec2::new(-d.y, d.x);
    line(s, b, (p + d * 12.0 + n * 9.0).into());
    line(s, b, (p + d * 12.0 - n * 9.0).into());
}
pub fn image(index: usize) -> Image {
    let mut s = Vec::new();
    match index {
        0 => {
            line(&mut s, (24., 16.), (24., 48.));
            line(&mut s, (40., 16.), (40., 48.));
        }
        1 => {
            for (a, b) in [
                ((17., 20.), (47., 20.)),
                ((47., 20.), (47., 44.)),
                ((47., 44.), (17., 44.)),
                ((17., 44.), (17., 20.)),
                ((24., 27.), (39., 27.)),
                ((24., 34.), (35., 34.)),
                ((25., 13.), (39., 13.)),
                ((25., 51.), (39., 51.)),
            ] {
                line(&mut s, a, b);
            }
        }
        2 => {
            for (a, b) in [
                ((14., 26.), (14., 14.)),
                ((14., 14.), (26., 14.)),
                ((38., 14.), (50., 14.)),
                ((50., 14.), (50., 26.)),
                ((50., 38.), (50., 50.)),
                ((50., 50.), (38., 50.)),
                ((26., 50.), (14., 50.)),
                ((14., 50.), (14., 38.)),
            ] {
                line(&mut s, a, b);
            }
            circle(&mut s, 32., 32., 7.);
        }
        3 => {
            for i in 0..18 {
                let a = i as f32 * 0.26;
                let b = a + 0.26;
                line(
                    &mut s,
                    (32. + 18. * a.cos(), 32. + 18. * a.sin()),
                    (32. + 18. * b.cos(), 32. + 18. * b.sin()),
                );
            }
            arrow(&mut s, (25., 15.), (37., 14.));
        }
        4 => {
            circle(&mut s, 32., 15., 4.);
            line(&mut s, (32., 22.), (32., 38.));
            line(&mut s, (32., 26.), (18., 18.));
            line(&mut s, (32., 26.), (46., 18.));
            line(&mut s, (32., 38.), (21., 51.));
            line(&mut s, (32., 38.), (43., 51.));
        }
        5 => {
            for (a, b) in [
                ((19., 44.), (16., 32.)),
                ((16., 32.), (21., 29.)),
                ((21., 29.), (26., 35.)),
                ((26., 35.), (26., 17.)),
                ((26., 17.), (31., 17.)),
                ((31., 17.), (31., 29.)),
                ((31., 29.), (35., 22.)),
                ((35., 22.), (39., 23.)),
                ((39., 23.), (39., 30.)),
                ((39., 30.), (43., 27.)),
                ((43., 27.), (47., 30.)),
                ((47., 30.), (45., 45.)),
                ((45., 45.), (39., 51.)),
                ((39., 51.), (25., 51.)),
                ((25., 51.), (19., 44.)),
            ] {
                line(&mut s, a, b);
            }
        }
        6 => {
            circle(&mut s, 32., 32., 15.);
            for (a, b) in [
                ((32., 8.), (32., 24.)),
                ((32., 40.), (32., 56.)),
                ((8., 32.), (24., 32.)),
                ((40., 32.), (56., 32.)),
            ] {
                line(&mut s, a, b);
            }
        }
        7 => {
            for (a, b) in [
                ((25., 46.), (25., 24.)),
                ((25., 24.), (32., 14.)),
                ((32., 14.), (39., 24.)),
                ((39., 24.), (39., 46.)),
                ((39., 46.), (25., 46.)),
                ((25., 39.), (39., 39.)),
                ((17., 21.), (12., 16.)),
                ((47., 21.), (52., 16.)),
                ((32., 7.), (32., 3.)),
            ] {
                line(&mut s, a, b);
            }
        }
        8 => {
            circle(&mut s, 38., 17., 4.);
            for (a, b) in [
                ((35., 24.), (26., 32.)),
                ((26., 32.), (37., 39.)),
                ((37., 39.), (47., 49.)),
                ((26., 32.), (18., 44.)),
                ((18., 44.), (33., 49.)),
                ((32., 27.), (43., 31.)),
            ] {
                line(&mut s, a, b);
            }
        }
        9 => arrow(&mut s, (32., 49.), (32., 15.)),
        10 => arrow(&mut s, (32., 15.), (32., 49.)),
        11 => arrow(&mut s, (49., 32.), (15., 32.)),
        12 => arrow(&mut s, (15., 32.), (49., 32.)),
        13 => {
            line(&mut s, (15., 32.), (27., 44.));
            line(&mut s, (27., 44.), (49., 18.));
        }
        14 => {
            arrow(&mut s, (45., 32.), (17., 32.));
        }
        15 => {
            arrow(&mut s, (14., 24.), (49., 24.));
            arrow(&mut s, (49., 42.), (14., 42.));
        }
        16 => {
            arrow(&mut s, (32., 49.), (32., 17.));
            line(&mut s, (17., 11.), (47., 11.));
        }
        17 => {
            arrow(&mut s, (32., 15.), (32., 47.));
            line(&mut s, (17., 53.), (47., 53.));
        }
        18 => {
            circle(&mut s, 32., 32., 13.);
            circle(&mut s, 32., 32., 4.);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.;
                line(
                    &mut s,
                    (32. + 16. * a.cos(), 32. + 16. * a.sin()),
                    (32. + 23. * a.cos(), 32. + 23. * a.sin()),
                );
            }
        }
        _ => {
            circle(&mut s, 32., 32., 22.);
            for (a, b) in [
                ((25., 22.), (29., 17.)),
                ((29., 17.), (37., 17.)),
                ((37., 17.), (41., 23.)),
                ((41., 23.), (38., 29.)),
                ((38., 29.), (32., 33.)),
                ((32., 33.), (32., 38.)),
            ] {
                line(&mut s, a, b);
            }
            circle(&mut s, 32., 46., 1.);
        }
    }
    let mut data = vec![0; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..64 {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let distance = s
                .iter()
                .map(|shape| match *shape {
                    Shape::Line(a, b) => {
                        let d = b - a;
                        let t = ((p - a).dot(d) / d.length_squared().max(0.001)).clamp(0., 1.);
                        p.distance(a + d * t)
                    }
                    Shape::Circle(c, r) => (p.distance(c) - r).abs(),
                })
                .fold(f32::MAX, f32::min);
            let alpha = ((2.2 - distance).clamp(0., 1.) * 255.) as u8;
            data[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].copy_from_slice(&[255, 255, 255, alpha]);
        }
    }
    Image::new(
        Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}
