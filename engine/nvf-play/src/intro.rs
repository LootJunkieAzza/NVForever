//! The intro film before a new game (`Video\FNVIntro.bik`, the owner's choice of film,
//! 2026-10-15): chosen New Game on the main menu (`crate::main_menu`) plays it, then the
//! opening begins. Any key or a click skips it. The game stays held under it as under the
//! main menu (`crate::pause::Paused`).
//!
//! The file is Bink video 1 (`BIKi`), 1280×720 at 30 frames a second with 48 kHz stereo
//! sound [observed: its header and a decode of its first 300 frames, 2026-10-15]; the
//! original's INI names another film for its start (`sIntroMovie=Fallout INTRO Vsk.bik`,
//! not in the owner's `Data\Video`) [documented: Fallout_default.ini]. It is decoded by the
//! `infinitier_bik_decoder` crate (docs/design/dependencies.md) on a thread of its own,
//! read straight from the loose file (it is 353 MB), a few frames ahead; each frame is
//! shown when its time has come by the wall clock since the film started (late frames are
//! dropped), the sound plays from a queue the decoder fills. The picture is fitted inside
//! the window, black around it.
//!
//! Built with the cargo feature `intro-video`, on by default (its decoder is
//! GPL-3.0-or-later, as NVForever is: owner decision 2026-10-15). A build without it
//! (`--no-default-features`) goes straight to the opening (`intro: not built in`).
//!
//! Log: `intro: playing <file> (<w>x<h>, <n> frames)`, `intro: skipped at <s> s` or
//! `intro: ended after <s> s`, `intro: <file>: <why>` when it cannot play (the game then
//! starts at once). Test aids: `NVF_PLAY_TEST_INTRO_SKIP=<seconds>` skips after that long;
//! `NVF_PLAY_TEST_INTRO_SHOT=<seconds>` saves a screenshot `intro.png` in the working
//! folder at that time.

use crate::pause::Paused;
use crate::{PlaySet, PlayState};
#[cfg(feature = "intro-video")]
use bevy::asset::RenderAssetUsages;
use bevy::audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;
#[cfg(feature = "intro-video")]
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
#[cfg(feature = "intro-video")]
use infinitier_bik_decoder::{BikOutputFormat, BikPixels, BikStreamingDecoder};
#[cfg(feature = "intro-video")]
use nvf_assets::GameData;
use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "intro-video")]
use std::sync::mpsc::sync_channel;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The film [owner, 2026-10-15].
pub const FILM: &str = "video\\fnvintro.bik";
/// Frames decoded ahead of the one shown.
#[cfg(feature = "intro-video")]
const AHEAD: usize = 8;

/// Play the intro, then start the game (sent by the main menu's New Game).
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayIntro;

/// What the decoder thread shares: the sound queue, its format, and stop / done flags.
struct Shared {
    sound: Mutex<VecDeque<i16>>,
    format: Mutex<Option<(u8, u32)>>,
    stop: AtomicBool,
    done: AtomicBool,
}

/// A frame: its number and its pixels (RGBA).
type Frame = (u32, Vec<u8>);

struct Playing {
    frames: Mutex<Receiver<Frame>>,
    shared: Arc<Shared>,
    image: Handle<Image>,
    size: Vec2,
    frame_secs: f64,
    /// Real time the first frame was shown.
    started: Option<f64>,
    /// The next frame waiting for its time.
    waiting: Option<Frame>,
    sound: Option<Entity>,
    root: Entity,
}

#[derive(Resource, Default)]
pub struct Intro {
    playing: Option<Playing>,
    skip_after: Option<f32>,
    shot_at: Option<f32>,
}

impl Intro {
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }
}

/// The film's sound, played from the queue the decoder fills.
#[derive(Asset, TypePath, Clone)]
pub struct IntroSound {
    shared: Arc<Shared>,
    channels: ChannelCount,
    rate: SampleRate,
}

pub struct IntroDecoder(IntroSound);

impl Iterator for IntroDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = self.0.shared.sound.lock().ok()?.pop_front();
        match s {
            Some(v) => Some(v as f32 / 32768.0),
            // Waiting for the decoder: silence; once it is done and the queue empty, the end.
            None if self.0.shared.done.load(Ordering::Relaxed) || self.0.shared.stop.load(Ordering::Relaxed) => None,
            None => Some(0.0),
        }
    }
}

impl Source for IntroDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        self.0.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.0.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for IntroSound {
    type Decoder = IntroDecoder;
    fn decoder(&self) -> IntroDecoder {
        IntroDecoder(self.clone())
    }
}

#[cfg(feature = "intro-video")]
#[derive(Component)]
struct IntroRoot;
#[derive(Component)]
struct IntroPicture;

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        let aid = |name: &str| std::env::var(name).ok().and_then(|v| v.trim().parse().ok());
        app.insert_resource(Intro { playing: None, skip_after: aid("NVF_PLAY_TEST_INTRO_SKIP"), shot_at: aid("NVF_PLAY_TEST_INTRO_SHOT") })
            .add_message::<PlayIntro>()
            .add_audio_source::<IntroSound>()
            .add_systems(Update, (start, play).chain().in_set(PlaySet::Ui));
    }
}

/// Let the held new game run (what New Game does once the film is over).
fn begin_game(paused: &mut Paused, next: &mut NextState<PlayState>) {
    paused.0 = false;
    next.set(PlayState::Playing);
    info!("intro: the opening begins");
}

/// Without the decoder: straight to the opening.
#[cfg(not(feature = "intro-video"))]
fn start(mut asked: MessageReader<PlayIntro>, mut paused: ResMut<Paused>, mut next: ResMut<NextState<PlayState>>) {
    if asked.read().last().is_some() {
        info!("intro: not built in (cargo feature intro-video); {FILM} is not played");
        begin_game(&mut paused, &mut next);
    }
}

#[cfg(feature = "intro-video")]
#[allow(clippy::too_many_arguments)]
fn start(
    mut asked: MessageReader<PlayIntro>,
    mut intro: ResMut<Intro>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut paused: ResMut<Paused>,
    mut next: ResMut<NextState<PlayState>>,
) {
    if asked.read().last().is_none() || intro.playing.is_some() {
        return;
    }
    let opened = data.as_deref().ok_or_else(|| "no game data".to_string()).and_then(|d| d.vfs.loose_path(FILM).ok_or_else(|| "not a loose file in the game's Data folder".to_string())).and_then(|path| {
        let f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let d = BikStreamingDecoder::new(std::io::BufReader::with_capacity(1 << 20, f), "FNVIntro.bik").map_err(|e| e.to_string())?.with_output_format(BikOutputFormat::Rgba);
        Ok((path, d))
    });
    let (path, mut dec) = match opened {
        Ok(x) => x,
        Err(e) => {
            warn!("intro: {FILM}: {e}");
            begin_game(&mut paused, &mut next);
            return;
        }
    };
    let (w, h, frames, frame_us) = (dec.width(), dec.height(), dec.frame_count(), dec.frame_duration_us());
    info!("intro: playing {} ({w}x{h}, {frames} frames)", path.display());
    let shared = Arc::new(Shared { sound: Mutex::new(VecDeque::new()), format: Mutex::new(None), stop: AtomicBool::new(false), done: AtomicBool::new(false) });
    let (tx, rx) = sync_channel::<Frame>(AHEAD);
    let s = shared.clone();
    std::thread::Builder::new()
        .name("nvf-intro".into())
        .spawn(move || {
            let mut i = 0u32;
            while !s.stop.load(Ordering::Relaxed) {
                match dec.next_frame() {
                    Ok(Some(fr)) => {
                        for a in fr.audio {
                            if let Ok(mut f) = s.format.lock() {
                                f.get_or_insert((a.channels, a.sample_rate));
                            }
                            if let Ok(mut q) = s.sound.lock() {
                                q.extend(a.samples);
                            }
                        }
                        if let BikPixels::Rgba(px) = fr.video.pixels {
                            if tx.send((i, px)).is_err() {
                                break;
                            }
                        }
                        i += 1;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        warn!("intro: frame {i}: {e}");
                        break;
                    }
                }
            }
            s.done.store(true, Ordering::Relaxed);
        })
        .expect("start the intro's decoder");
    let image = images.add(Image::new_fill(Extent3d { width: w, height: h, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD));
    let root = commands
        .spawn((IntroRoot, Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::BLACK), GlobalZIndex(250)))
        .with_children(|r| {
            r.spawn((IntroPicture, ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) }, Node { position_type: PositionType::Absolute, ..default() }));
        })
        .id();
    intro.playing = Some(Playing { frames: Mutex::new(rx), shared, image, size: Vec2::new(w as f32, h as f32), frame_secs: frame_us.max(1) as f64 / 1e6, started: None, waiting: None, sound: None, root });
}

/// The part of the window a picture of `size` takes when fitted inside it.
pub fn contain(size: Vec2, window: Vec2) -> (Vec2, Vec2) {
    if size.x <= 0.0 || size.y <= 0.0 {
        return (window, Vec2::ZERO);
    }
    let fitted = size * (window.x / size.x).min(window.y / size.y);
    (fitted, (window - fitted) / 2.0)
}

#[allow(clippy::too_many_arguments)]
fn play(
    real: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut intro: ResMut<Intro>,
    mut images: ResMut<Assets<Image>>,
    mut sounds: ResMut<Assets<IntroSound>>,
    mut pictures: Query<&mut Node, With<IntroPicture>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut paused: ResMut<Paused>,
    mut next: ResMut<NextState<PlayState>>,
) {
    let skip_after = intro.skip_after;
    let shot_at = intro.shot_at;
    let Some(p) = intro.playing.as_mut() else { return };
    let now = real.elapsed_secs_f64();
    // The sound starts with the first frame shown, once its format is known.
    if p.sound.is_none() {
        let format = p.shared.format.lock().ok().and_then(|f| *f);
        if let Some((ch, rate)) = format {
            let (Some(channels), Some(rate)) = (NonZero::new(u16::from(ch.max(1))), NonZero::new(rate.max(1))) else { return };
            let h = sounds.add(IntroSound { shared: p.shared.clone(), channels, rate });
            p.sound = Some(commands.spawn((AudioPlayer::<IntroSound>(h), PlaybackSettings::DESPAWN)).id());
        }
    }
    let started = *p.started.get_or_insert(now);
    let elapsed = now - started;
    let due = (elapsed / p.frame_secs) as u32;
    // The newest frame whose time has come; older ones are dropped.
    let mut show: Option<Frame> = None;
    let mut ended = false;
    loop {
        let f = match p.waiting.take() {
            Some(f) => f,
            None => match p.frames.lock().map_err(|_| TryRecvError::Disconnected).and_then(|r| r.try_recv()) {
                Ok(f) => f,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            },
        };
        if f.0 <= due {
            show = Some(f);
        } else {
            p.waiting = Some(f);
            break;
        }
    }
    if let Some((_, px)) = show {
        if let Some(mut img) = images.get_mut(&p.image) {
            img.data = Some(px);
        }
    }
    if let (Ok(w), Ok(mut node)) = (windows.single(), pictures.single_mut()) {
        let (size, offset) = contain(p.size, Vec2::new(w.width(), w.height()));
        node.width = Val::Px(size.x);
        node.height = Val::Px(size.y);
        node.left = Val::Px(offset.x);
        node.top = Val::Px(offset.y);
    }
    if let Some(t) = shot_at.filter(|t| elapsed >= f64::from(*t)) {
        info!("intro: screenshot intro.png at {t} s");
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk("intro.png"));
        intro.shot_at = None;
        return;
    }
    let by_aid = skip_after.is_some_and(|s| elapsed >= f64::from(s));
    // Not the key or click that chose New Game, a moment before.
    let pressed = elapsed > 0.3 && (keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some());
    let skipped = pressed || by_aid;
    if !(skipped || (ended && p.waiting.is_none())) {
        return;
    }
    if skipped {
        info!("intro: skipped at {elapsed:.1} s{}", if by_aid { " (test aid NVF_PLAY_TEST_INTRO_SKIP)" } else { "" });
    } else {
        info!("intro: ended after {elapsed:.1} s");
    }
    p.shared.stop.store(true, Ordering::Relaxed);
    if let Some(e) = p.sound.take() {
        if let Ok(mut e) = commands.get_entity(e) {
            e.despawn();
        }
    }
    commands.entity(p.root).despawn();
    intro.playing = None;
    begin_game(&mut paused, &mut next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_film_fits_inside_the_window_with_black_around_it() {
        let (s, o) = contain(Vec2::new(1280.0, 720.0), Vec2::new(1280.0, 800.0));
        assert_eq!(s, Vec2::new(1280.0, 720.0));
        assert_eq!(o, Vec2::new(0.0, 40.0));
        let (s, o) = contain(Vec2::new(1280.0, 720.0), Vec2::new(2560.0, 1080.0));
        assert_eq!(s, Vec2::new(1920.0, 1080.0));
        assert_eq!(o, Vec2::new(320.0, 0.0));
    }
}
