//! Download DLSS…'s dialog, over the settings page as the hand report's form is
//! (`reports/form.rs`, `nvf_play::pause::FormOver`: the menu under it takes no key or click):
//!
//! 1. **The licence**: what is fetched and under which licence, NVIDIA's licence text in a
//!    box that scrolls (PgUp, PgDn, the arrow keys, Home, End, the mouse wheel), and I accept,
//!    Open on GitHub (the release's page) and Cancel. Only I accept starts the download
//!    (`dlss::download::LicenceAccepted::by_player`); Cancel or Esc closes with nothing done.
//! 2. **The progress**: the step, the file (n of 4), the megabytes fetched of those to fetch
//!    and a bar; Hide (Esc) closes the dialog and the download goes on, Cancel download stops
//!    it. The choice on the Upscaler row shows the dialog again.
//! 3. **The result**: ready (a restart loads DLSS), or why not in our words with the log's
//!    words under them, and the manual way: Open release page, Open dlss folder (made first),
//!    Try again, Close.
//!
//! Drawn with the menus' fonts (`nvf_play::start_menu::Faces`: font 7 for the title, font 3
//! for the rest), in the interface's amber [design: the settings page's look].

use super::{AidPlan, AID};
use crate::dlss::download::{self, Failure, LicenceAccepted, Phase, Progress};
use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use fnv_util::words::{tr, trf};
use nvf_assets::GameData;
use nvf_play::interface::{amber, bc_supported, set_line, u, Face};
use nvf_play::pause::FormOver;
use nvf_play::start_menu::Faces;
use nvf_play::ui_pointer::{Button as UiButton, PointerState};

/// The panel's width and its text's, interface units (the view is 960 high) [design].
const PANEL_WIDTH: f32 = 1120.0;
const TEXT_WIDTH: f32 = PANEL_WIDTH - 48.0;
/// The licence lines the box shows at once [design].
const VIEW_LINES: usize = 16;
/// How often the progress page is drawn again while bytes come in, seconds [design].
const PROGRESS_REDRAW: f64 = 0.25;

/// The test aid's plan (`super` module notes).
#[derive(Resource, Debug)]
pub(super) struct Aid(pub AidPlan);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Closed,
    Licence,
    Progress,
    Result,
}

/// What a click on a node of the dialog does.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Accept,
    OpenGithub,
    Decline,
    Hide,
    StopDownload,
    OpenRelease,
    OpenFolder,
    Again,
    Close,
}

#[derive(Component)]
struct DialogRoot;

#[derive(Resource, Default)]
struct Dialog {
    page: Page,
    /// The menus' fonts, loaded at the first open.
    faces: Option<Faces>,
    /// The licence set in lines at the box's width.
    lines: Vec<fnv_font::Line>,
    /// The first licence line shown.
    scroll: usize,
    /// The download this dialog shows (started here, or found running).
    download: Option<download::Handle>,
    /// Its progress when last drawn, and when.
    drawn: Option<Progress>,
    drawn_at: f64,
    /// The last download's end was shown: the next open starts at the licence.
    end_seen: bool,
    opened_at: f64,
    started_at: f64,
    ended_at: f64,
    /// The test aid's press on the licence page, its cancel, done.
    aid_pressed: bool,
    aid_cancelled: bool,
    dirty: bool,
    /// Closed this frame: [`FormOver`] is cleared next frame, so the key that closed it does
    /// nothing in the menu under it.
    closing: bool,
}

impl Dialog {
    fn max_scroll(&self) -> usize {
        self.lines.len().saturating_sub(VIEW_LINES)
    }

    fn scroll_by(&mut self, lines: isize) {
        let next = (self.scroll as isize + lines).clamp(0, self.max_scroll() as isize) as usize;
        if next != self.scroll {
            self.scroll = next;
            self.dirty = true;
        }
    }

    fn progress(&self) -> Option<Progress> {
        self.download.as_ref().map(download::Handle::progress)
    }

    fn close(&mut self, why: &str) {
        if self.page == Page::Closed {
            return;
        }
        info!("DLSS download: dialog {why}");
        self.page = Page::Closed;
        self.closing = true;
        self.dirty = true;
    }

    /// What a press of `act` does on the page shown.
    fn act(&mut self, act: Act, now: f64) {
        match (act, self.page) {
            (Act::Accept, Page::Licence) => {
                self.download = Some(download::start(LicenceAccepted::by_player()));
                self.page = Page::Progress;
                self.end_seen = false;
                self.started_at = now;
                self.drawn = None;
            }
            (Act::OpenGithub, Page::Licence) | (Act::OpenRelease, Page::Result) => super::open_in_shell(download::RELEASE_PAGE.into()),
            (Act::Decline, Page::Licence) => {
                info!("DLSS download: licence not accepted; nothing downloaded");
                self.close("closed");
            }
            (Act::Hide, Page::Progress) => self.close("hidden; the download goes on"),
            (Act::StopDownload, Page::Progress) => {
                if let Some(d) = &self.download {
                    info!("DLSS download: cancel asked");
                    d.cancel();
                }
            }
            (Act::OpenFolder, Page::Result) => super::open_dlss_folder(),
            (Act::Again, Page::Result) => {
                self.page = Page::Licence;
                self.scroll = 0;
                self.download = None;
                self.opened_at = now;
                self.aid_pressed = false;
                info!("DLSS download: licence shown again ({} lines)", self.lines.len());
            }
            (Act::Close, Page::Result) => self.close("closed"),
            _ => return,
        }
        self.dirty = true;
    }

    /// Esc: Cancel on the licence, Hide on the progress, Close on the result.
    fn escape(&mut self, now: f64) {
        let act = match self.page {
            Page::Licence => Act::Decline,
            Page::Progress => Act::Hide,
            Page::Result => Act::Close,
            Page::Closed => return,
        };
        self.act(act, now);
    }
}

pub(super) fn add(app: &mut App) {
    app.init_resource::<Dialog>()
        .add_systems(PreUpdate, clear_form_over.run_if(resource_exists::<FormOver>))
        .add_systems(Update, (open, keys, wheel, clicks, test_aid, follow, draw).chain().run_if(resource_exists::<FormOver>));
}

fn clear_form_over(mut d: ResMut<Dialog>, mut over: ResMut<FormOver>) {
    if d.closing {
        d.closing = false;
        over.set_if_neq(FormOver(false));
    }
}

/// The choice was picked (`super::Asked`): the dialog opens on the page that fits.
#[allow(clippy::too_many_arguments)]
fn open(
    mut commands: Commands,
    asked: Option<Res<super::Asked>>,
    mut d: ResMut<Dialog>,
    mut over: ResMut<FormOver>,
    real: Res<Time<Real>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
) {
    if asked.is_none() {
        return;
    }
    commands.remove_resource::<super::Asked>();
    if d.page != Page::Closed {
        return;
    }
    if d.faces.is_none() {
        let faces = Faces::load(data.as_deref(), &mut images, bc_supported(formats.as_deref()));
        d.lines = faces.small.font.lines(&super::licence_for_display(), Some(TEXT_WIDTH - 24.0));
        d.faces = Some(faces);
    }
    let current = download::current();
    let phase = current.as_ref().map(|h| h.progress().phase);
    d.page = match &phase {
        Some(p) if !p.finished() => Page::Progress,
        Some(_) if !d.end_seen => Page::Result,
        _ => Page::Licence,
    };
    d.download = if d.page == Page::Licence { None } else { current };
    let now = real.elapsed_secs_f64();
    d.opened_at = now;
    d.ended_at = now;
    d.aid_pressed = false;
    d.drawn = None;
    d.dirty = true;
    match d.page {
        Page::Licence => {
            d.scroll = 0;
            info!("DLSS download: licence shown ({} lines); nothing is downloaded unless I accept is pressed", d.lines.len());
        }
        Page::Progress => info!("DLSS download: dialog shown again over the running download"),
        _ => info!("DLSS download: dialog shown again with the last download's result"),
    }
    over.set_if_neq(FormOver(true));
}

fn keys(mut typed: MessageReader<KeyboardInput>, mut d: ResMut<Dialog>, real: Res<Time<Real>>) {
    let pressed: Vec<Key> = typed.read().filter(|k| k.state.is_pressed()).map(|k| k.logical_key.clone()).collect();
    if d.page == Page::Closed {
        return;
    }
    let now = real.elapsed_secs_f64();
    let page = VIEW_LINES as isize - 1;
    let all = d.lines.len() as isize;
    for key in pressed {
        match (d.page, key) {
            (_, Key::Escape) => {
                d.escape(now);
                return;
            }
            (Page::Licence, Key::PageDown) => d.scroll_by(page),
            (Page::Licence, Key::PageUp) => d.scroll_by(-page),
            (Page::Licence, Key::ArrowDown) => d.scroll_by(1),
            (Page::Licence, Key::ArrowUp) => d.scroll_by(-1),
            (Page::Licence, Key::Home) => d.scroll_by(-all),
            (Page::Licence, Key::End) => d.scroll_by(all),
            _ => {}
        }
    }
}

fn wheel(mut wheel: MessageReader<MouseWheel>, mut d: ResMut<Dialog>) {
    let lines: f32 = wheel
        .read()
        .map(|w| match w.unit {
            MouseScrollUnit::Line => w.y * 3.0,
            MouseScrollUnit::Pixel => w.y / 20.0,
        })
        .sum();
    if d.page == Page::Licence && lines.abs() >= 0.5 {
        d.scroll_by(-(lines.round() as isize));
    }
}

fn clicks(pressed: Query<(&PointerState, &Act), Changed<PointerState>>, mut d: ResMut<Dialog>, real: Res<Time<Real>>) {
    if let Some(act) = pressed.iter().find(|(s, _)| **s == PointerState::Pressed).map(|(_, a)| *a) {
        d.act(act, real.elapsed_secs_f64());
    }
}

/// The test aid's presses (`super` module notes).
fn test_aid(aid: Option<Res<Aid>>, mut d: ResMut<Dialog>, real: Res<Time<Real>>) {
    let Some(aid) = aid else { return };
    let now = real.elapsed_secs_f64();
    match d.page {
        Page::Licence if !d.aid_pressed && now - d.opened_at >= 1.0 => {
            d.aid_pressed = true;
            if aid.0.accept {
                info!("test aid {AID}: I accept pressed");
                d.act(Act::Accept, now);
            } else if aid.0.cancel {
                info!("test aid {AID}: Cancel pressed");
                d.act(Act::Decline, now);
            }
        }
        Page::Progress => {
            if let Some(at) = aid.0.cancel_at.filter(|at| !d.aid_cancelled && now - d.started_at >= *at) {
                d.aid_cancelled = true;
                info!("test aid {AID}: Cancel download pressed {at} s after the start");
                d.act(Act::StopDownload, now);
            }
        }
        Page::Result if now - d.ended_at >= 1.0 => {
            info!("test aid {AID}: Close pressed");
            d.act(Act::Close, now);
        }
        _ => {}
    }
}

/// The download followed: the progress drawn again as it moves, the result once it ends.
fn follow(mut d: ResMut<Dialog>, real: Res<Time<Real>>) {
    if !matches!(d.page, Page::Progress | Page::Result) {
        return;
    }
    let Some(p) = d.progress() else { return };
    let now = real.elapsed_secs_f64();
    if d.page == Page::Progress && p.phase.finished() {
        d.page = Page::Result;
        d.ended_at = now;
        d.dirty = true;
        info!("DLSS download: result shown ({})", result_key(&p.phase));
    }
    if d.page == Page::Result {
        d.end_seen = true;
    } else if d.drawn.as_ref() != Some(&p) && now - d.drawn_at >= PROGRESS_REDRAW {
        d.dirty = true;
    }
}

/// The result in a word, for the log.
fn result_key(phase: &Phase) -> &'static str {
    match phase {
        Phase::Done(_) => "ready",
        Phase::Cancelled => "cancelled",
        _ => "failed",
    }
}

/// The step of a running download in the dialog's words.
fn phase_words(phase: &Phase) -> String {
    match phase {
        Phase::Connecting => tr("Connecting to GitHub...").to_string(),
        Phase::Directory => tr("Reading the archive's list of files...").to_string(),
        Phase::File { name, index, of } => trf("File {index} of {of}: {name}", &[("index", index), ("of", of), ("name", name)]),
        Phase::Checking => tr("Checking the files...").to_string(),
        Phase::Installing => tr("Putting the files in the dlss folder...").to_string(),
        Phase::Done(_) | Phase::Failed(_) | Phase::Cancelled => String::new(),
    }
}

/// Why a download failed, in the dialog's words (the log has `Failure`'s own).
fn failure_words(failure: &Failure) -> String {
    match failure {
        Failure::Offline(_) => tr("No answer from GitHub: check the connection, then try again.").to_string(),
        Failure::Http(status) => trf("GitHub answered with an error (HTTP {status}).", &[("status", status)]),
        Failure::NoRanges | Failure::WrongSize(_) | Failure::NotAZip(_) | Failure::EntryMissing(_) | Failure::Redirect(_) => {
            tr("What GitHub sent was not NVIDIA's expected archive; nothing was kept.").to_string()
        }
        Failure::Mismatch { name, .. } => trf("{name} did not match the tested release; nothing was kept.", &[("name", name)]),
        Failure::Signature { name, .. } => trf("{name} failed its signature check; nothing was kept.", &[("name", name)]),
        Failure::FolderHolds(name) => trf("The dlss folder already holds {name}, which was left as it is: remove it, or copy the files by hand.", &[("name", name)]),
        Failure::Write(_) => tr("The files could not be written to the dlss folder.").to_string(),
        Failure::NotWindows => tr("DLSS needs Windows").to_string(),
        // A panic in the download thread (the core catches it and ends the download here).
        Failure::Internal(_) => tr("The download stopped on an internal error; the log says more.").to_string(),
    }
}

/// Megabytes with one decimal.
fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000.0)
}

/// Lines of `text` wrapped at `width`, as a node of their height.
fn block(p: &mut ChildSpawnerCommands, face: &Face, text: &str, width: f32, color: Color) {
    let lines = face.font.lines(text, Some(width));
    let step = face.font.size;
    p.spawn(Node { width: u(width), height: u(step * lines.len() as f32), flex_shrink: 0.0, ..default() }).with_children(|b| {
        for (i, line) in lines.iter().enumerate() {
            set_line(b, face, line, 0.0, step * (i as f32 + 0.8), color);
        }
    });
}

/// A button with `label`, taken by a click when `on`.
fn button(p: &mut ChildSpawnerCommands, face: &Face, label: &str, act: Act, on: bool) {
    let (bright, faint) = (amber(255.0), amber(90.0));
    let line = face.font.lines(label, None).into_iter().next().unwrap_or_default();
    let h = face.font.size;
    let mut b = p.spawn((
        Node { padding: UiRect::axes(u(14.0), u(4.0)), border: UiRect::all(Val::Px(1.0)), ..default() },
        BorderColor::all(if on { bright } else { faint }),
        BackgroundColor(if on { amber(40.0) } else { Color::NONE }),
    ));
    if on {
        b.insert((UiButton, act));
    }
    b.with_children(|b| {
        b.spawn(Node { width: u(line.width), height: u(h), ..default() }).with_children(|t| set_line(t, face, &line, 0.0, h * 0.8, if on { bright } else { faint }));
    });
}

/// A row of buttons, right-aligned.
fn buttons(p: &mut ChildSpawnerCommands, face: &Face, list: &[(String, Act, bool)]) {
    p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: u(16.0), justify_content: JustifyContent::FlexEnd, margin: UiRect::top(u(6.0)), ..default() }).with_children(|r| {
        for (label, act, on) in list {
            button(r, face, label, *act, *on);
        }
    });
}

/// `label` with its key, as the menus show it.
fn with_key(label: &str, key: &str) -> String {
    format!("{label}  {key}")
}

/// The manual way, for a download that did not happen.
fn manual_way() -> &'static str {
    tr("Or get the files by hand: open the release page, download streamline-sdk-v2.14.16.zip and copy sl.interposer.dll, sl.common.dll, sl.dlss.dll and nvngx_dlss.dll from its bin\\x64 folder into the dlss folder (never nvngx_update.exe), then restart.")
}

/// Draw the dialog again when it changed.
fn draw(mut commands: Commands, mut d: ResMut<Dialog>, roots: Query<Entity, With<DialogRoot>>, real: Res<Time<Real>>) {
    if !std::mem::take(&mut d.dirty) {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    if d.page == Page::Closed {
        return;
    }
    let progress = d.progress();
    d.drawn = progress.clone();
    d.drawn_at = real.elapsed_secs_f64();
    let d = &*d;
    let Some(faces) = &d.faces else { return };
    let (bright, dim, faint) = (amber(255.0), amber(170.0), amber(90.0));
    let full = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
    commands.spawn((DialogRoot, full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)), GlobalZIndex(180))).with_children(|root| {
        let panel = Node { width: u(PANEL_WIDTH), flex_direction: FlexDirection::Column, padding: UiRect::all(u(24.0)), row_gap: u(10.0), border: UiRect::all(Val::Px(2.0)), ..default() };
        root.spawn((panel, BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.95)), BorderColor::all(dim))).with_children(|p| {
            let (title, small) = (&faces.entry, &faces.small);
            match d.page {
                Page::Licence => {
                    block(p, title, tr("Download DLSS"), TEXT_WIDTH, bright);
                    block(p, small, tr("NVForever fetches four files from NVIDIA's Streamline 2.14.16 release on GitHub: sl.interposer.dll, sl.common.dll and sl.dlss.dll, under Streamline's MIT licence, and nvngx_dlss.dll, under NVIDIA's licence below. Nothing is downloaded unless you accept it."), TEXT_WIDTH, dim);
                    block(p, small, tr("NVIDIA's licence for nvngx_dlss.dll"), TEXT_WIDTH, bright);
                    let step = small.font.size;
                    let shown = &d.lines[d.scroll.min(d.lines.len())..(d.scroll + VIEW_LINES).min(d.lines.len())];
                    p.spawn((
                        Node { width: u(TEXT_WIDTH), height: u(step * VIEW_LINES as f32 + 12.0), border: UiRect::all(Val::Px(1.0)), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() },
                        BorderColor::all(faint),
                    ))
                    .with_children(|b| {
                        for (i, line) in shown.iter().enumerate() {
                            set_line(b, small, line, 10.0, 6.0 + step * (i as f32 + 0.8), bright);
                        }
                    });
                    let last = (d.scroll + VIEW_LINES).min(d.lines.len());
                    let total = d.lines.len();
                    block(p, small, &trf("Lines {first} to {last} of {total}. PgUp, PgDn, the arrow keys or the mouse wheel scroll.", &[("first", &(d.scroll + 1).min(total)), ("last", &last), ("total", &total)]), TEXT_WIDTH, dim);
                    block(p, small, tr("I accept starts the download; the game goes on meanwhile."), TEXT_WIDTH, dim);
                    buttons(p, small, &[(tr("I accept").to_string(), Act::Accept, true), (tr("Open on GitHub").to_string(), Act::OpenGithub, true), (with_key(tr("Cancel"), "Esc"), Act::Decline, true)]);
                }
                Page::Progress => {
                    block(p, title, tr("Downloading DLSS"), TEXT_WIDTH, bright);
                    let p_now = progress.unwrap_or(Progress { phase: Phase::Connecting, fetched: 0, to_fetch: 0 });
                    block(p, small, &phase_words(&p_now.phase), TEXT_WIDTH, bright);
                    if p_now.to_fetch > 0 {
                        block(p, small, &trf("{fetched} MB of {total} MB", &[("fetched", &megabytes(p_now.fetched.min(p_now.to_fetch))), ("total", &megabytes(p_now.to_fetch))]), TEXT_WIDTH, dim);
                    }
                    let share = super::percent(&p_now) as f32;
                    p.spawn((Node { width: u(TEXT_WIDTH), height: u(16.0), border: UiRect::all(Val::Px(1.0)), flex_shrink: 0.0, ..default() }, BorderColor::all(dim))).with_children(|bar| {
                        bar.spawn((Node { width: Val::Percent(share), height: Val::Percent(100.0), ..default() }, BackgroundColor(amber(200.0))));
                    });
                    block(p, small, tr("The game goes on meanwhile. Hide keeps the download running; its choice on the Upscaler row shows this again."), TEXT_WIDTH, dim);
                    buttons(p, small, &[(with_key(tr("Hide"), "Esc"), Act::Hide, true), (tr("Cancel download").to_string(), Act::StopDownload, true)]);
                }
                Page::Result => {
                    let phase = progress.map(|p| p.phase);
                    match &phase {
                        Some(Phase::Done(folder)) => {
                            block(p, title, tr("DLSS is ready"), TEXT_WIDTH, bright);
                            block(p, small, tr("DLSS is ready: restart NVForever to use it."), TEXT_WIDTH, bright);
                            block(p, small, &trf("Installed in {folder}.", &[("folder", &folder.display())]), TEXT_WIDTH, dim);
                            buttons(p, small, &[(with_key(tr("Close"), "Esc"), Act::Close, true)]);
                        }
                        other => {
                            match other {
                                Some(Phase::Failed(failure)) => {
                                    block(p, title, tr("DLSS was not downloaded"), TEXT_WIDTH, bright);
                                    block(p, small, &failure_words(failure), TEXT_WIDTH, bright);
                                    block(p, small, &trf("Details: {why}", &[("why", failure)]), TEXT_WIDTH, faint);
                                }
                                _ => {
                                    block(p, title, tr("Download cancelled"), TEXT_WIDTH, bright);
                                    block(p, small, tr("Nothing was written."), TEXT_WIDTH, bright);
                                }
                            }
                            block(p, small, manual_way(), TEXT_WIDTH, dim);
                            buttons(
                                p,
                                small,
                                &[
                                    (tr("Open release page").to_string(), Act::OpenRelease, true),
                                    (tr("Open dlss folder").to_string(), Act::OpenFolder, true),
                                    (tr("Try again").to_string(), Act::Again, true),
                                    (with_key(tr("Close"), "Esc"), Act::Close, true),
                                ],
                            );
                        }
                    }
                }
                Page::Closed => {}
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog(lines: usize) -> Dialog {
        Dialog { page: Page::Licence, lines: vec![fnv_font::Line::default(); lines], ..Default::default() }
    }

    #[test]
    fn the_licence_scrolls_within_its_lines() {
        let mut d = dialog(40);
        d.scroll_by(-5);
        assert_eq!(d.scroll, 0);
        d.scroll_by(VIEW_LINES as isize - 1);
        assert_eq!(d.scroll, VIEW_LINES - 1);
        d.scroll_by(1000);
        assert_eq!(d.scroll, 40 - VIEW_LINES, "the last line at the box's bottom");
        let mut short = dialog(3);
        short.scroll_by(10);
        assert_eq!(short.scroll, 0, "a licence that fits does not scroll");
    }

    #[test]
    fn closing_without_accepting_starts_nothing() {
        let mut d = dialog(40);
        d.escape(0.0);
        assert_eq!(d.page, Page::Closed);
        assert!(d.closing && d.download.is_none());
        // A press that does not belong to the page shown does nothing.
        let mut d = dialog(40);
        d.act(Act::StopDownload, 0.0);
        d.act(Act::Close, 0.0);
        assert_eq!(d.page, Page::Licence);
        assert!(d.download.is_none());
    }

    #[test]
    fn every_failure_has_the_dialogs_own_words() {
        let all = [
            Failure::Offline("x".into()),
            Failure::Http(404),
            Failure::NoRanges,
            Failure::WrongSize(1),
            Failure::NotAZip("x".into()),
            Failure::EntryMissing("bin/x64/a.dll"),
            Failure::Mismatch { name: "a.dll", sha256: "00".into() },
            Failure::Signature { name: "a.dll", why: "x".into() },
            Failure::FolderHolds("nvngx_update.exe".into()),
            Failure::Write("x".into()),
            Failure::Redirect("x".into()),
            Failure::NotWindows,
        ];
        for f in all {
            let words = failure_words(&f);
            assert!(!words.is_empty() && !words.contains('{'), "{f:?}: {words}");
        }
        assert_eq!(failure_words(&Failure::Http(404)), "GitHub answered with an error (HTTP 404).");
        assert_eq!(phase_words(&Phase::File { name: "sl.dlss.dll", index: 3, of: 4 }), "File 3 of 4: sl.dlss.dll");
        assert_eq!(megabytes(58_956_912), "59.0");
    }
}
