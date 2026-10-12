//! Test aid for the Pip-Boy: `NVF_PLAY_TEST_PIPBOY=<seconds>:<step>[,...]`, steps run in
//! time order (seconds after the start, as the other aids count them):
//!
//! - `open`: press Tab (the inventory opens and the Pip-Boy comes up);
//! - `close`: put it away;
//! - `page=stats|items|data`: show a page;
//! - `tab=<0-4>`: show a tab of the page shown;
//! - `key=<down|up|use|drop>`: a key of the ITEMS list;
//! - `point=<x>:<y>`: put the pointer on that point of the canvas (1024 x 768 from its
//!   top left), through the model's screen where it is shown, as the mouse would be;
//!   `point=off` gives the pointer back to the mouse;
//! - `click=<x>:<y>`: the same, then a left click there;
//! - `click=stats|items|data`: a left click on the page's button on the device.
//!
//! Each step is logged. Screenshots are `NVF_PLAY_TEST_SHOTS`'.

use super::{AidTarget, Page, PipBoyAid, PipBoyInput};
use crate::menu::model::Command;
use crate::menu::{MenuInput, OpenInventory};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    Open,
    Close,
    Page(Page),
    Tab(usize),
    Key(Command),
    Point(Option<Vec2>),
    Click(AidTarget),
}

fn page(p: &str) -> Option<Page> {
    match p.trim() {
        "stats" => Some(Page::Stats),
        "items" => Some(Page::Items),
        "data" => Some(Page::Data),
        _ => None,
    }
}

fn canvas(p: &str) -> Option<Vec2> {
    let (x, y) = p.split_once(':')?;
    Some(Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

#[derive(Resource)]
struct Script(Vec<(f32, Step)>);

fn step(s: &str) -> Option<Step> {
    let s = s.trim();
    Some(match s {
        "open" => Step::Open,
        "close" => Step::Close,
        _ => match s.split_once('=')? {
            ("page", p) => Step::Page(page(p)?),
            ("point", "off") => Step::Point(None),
            ("point", p) => Step::Point(Some(canvas(p)?)),
            ("click", p) => Step::Click(page(p).map(AidTarget::Button).or_else(|| canvas(p).map(AidTarget::Canvas))?),
            ("tab", t) => Step::Tab(t.trim().parse().ok().filter(|t| *t < 5)?),
            ("key", k) => Step::Key(match k.trim() {
                "down" => Command::Down,
                "up" => Command::Up,
                "use" => Command::One,
                "drop" => Command::Drop,
                _ => return None,
            }),
            _ => return None,
        },
    })
}

fn parse(v: &str) -> Vec<(f32, Step)> {
    let mut out: Vec<(f32, Step)> = v
        .split(',')
        .filter_map(|item| {
            let (t, s) = item.split_once(':')?;
            let parsed = t.trim().parse().ok().zip(step(s));
            if parsed.is_none() {
                warn!("NVF_PLAY_TEST_PIPBOY: cannot read {item:?}");
            }
            parsed
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

pub(super) fn setup(app: &mut App) {
    let Ok(v) = std::env::var("NVF_PLAY_TEST_PIPBOY") else { return };
    app.insert_resource(Script(parse(&v))).add_systems(Update, run.in_set(crate::PlaySet::Input));
}

fn run(
    time: crate::pause::AidTime,
    mut script: ResMut<Script>,
    mut open: MessageWriter<OpenInventory>,
    mut menu: MessageWriter<MenuInput>,
    mut pip: MessageWriter<PipBoyInput>,
    mut aid: ResMut<PipBoyAid>,
) {
    let now = time.elapsed_secs();
    while let Some(&(t, s)) = script.0.first() {
        if t > now {
            break;
        }
        script.0.remove(0);
        info!("test aid: pipboy {s:?} at {now:.1} s");
        match s {
            Step::Open => {
                open.write(OpenInventory);
            }
            Step::Close => {
                menu.write(MenuInput(Command::Close));
            }
            Step::Page(p) => {
                pip.write(PipBoyInput::Page(p));
            }
            Step::Tab(t) => {
                pip.write(PipBoyInput::Tab(t));
            }
            Step::Key(c) => {
                menu.write(MenuInput(c));
            }
            Step::Point(p) => {
                aid.target = p.map(AidTarget::Canvas);
            }
            Step::Click(t) => {
                aid.target = Some(t);
                aid.click = 3;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_read_in_time_order() {
        let s = parse("9:close, 3:open,4:page=stats, 5:tab=2, 6:key=down, 7:tab=9, x:open, 8:click=data, 8.5:click=100:650, 8.7:point=off, 8.8:point=1:2");
        assert_eq!(
            s,
            vec![
                (3.0, Step::Open),
                (4.0, Step::Page(Page::Stats)),
                (5.0, Step::Tab(2)),
                (6.0, Step::Key(Command::Down)),
                (8.0, Step::Click(AidTarget::Button(Page::Data))),
                (8.5, Step::Click(AidTarget::Canvas(Vec2::new(100.0, 650.0)))),
                (8.7, Step::Point(None)),
                (8.8, Step::Point(Some(Vec2::new(1.0, 2.0)))),
                (9.0, Step::Close)
            ]
        );
    }
}
