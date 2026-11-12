//! The temporal upscaler choice shared by FSR 3.1 (`fsr.rs`) and DLSS (`dlss/`), #2198
//! (docs/design/dlss-upscaling.md, "Internal interfaces").
//!
//! - [`UpscalerChoice`] is what the player asks for: off, FSR at a quality or DLSS at a quality.
//!   It is the one requested setting, `Graphics::upscaler`; FSR's call sites read it through
//!   `Graphics::fsr` and write it through `Graphics::set_fsr`.
//! - [`UpscaleQuality`] is the quality list both upscalers use (`fsr::FsrQuality` is an alias;
//!   FSR's ratios stay in `fsr.rs`, DLSS's sizes come from DLSS).
//! - [`UpscalerStatus`] is what this session can do and what it draws: the **effective** choice,
//!   whether a **restart is pending**, and why a request cannot run ([`Unavailable`]).
//!   [`resolve`] is its rule [design]: Streamline is only loaded when DLSS is asked for at start,
//!   so entering or leaving DLSS takes a restart, and until then the effective choice stays.
//!   What the start-up loaded Streamline for is the session's [`Boundary`], kept apart from the
//!   latest effective choice, so a DLSS session that stops drawing DLSS still needs a restart
//!   to leave it.
//!
//! - [`prep`] is the view preparation every temporal upscaler shares (step 4): camera history
//!   and cuts, the render-size override, jitter and mip bias, the corrected and expanded depth,
//!   the spatial fallback and the alpha copy. FSR adds its own state after it; DLSS gets it
//!   without any of FSR's.
//! - [`temporal_active`] is the question the shared parts ask (HDR, prepasses, cut-outs drawn
//!   alpha-tested, the anti-aliasing lock, the ambient occlusion at the render size), where
//!   `graphics::fsr_active` stays FSR's own (its sharpening, frame generation). Both go by the
//!   effective choice ([`effective`]).
//!
//! **Start** (step 8) [design]: the requested choice is the command line's
//! ([`command_line_choice`]: `--upscaler`, `--upscaler-quality`, the older `--fsr`), then the
//! settings' INI by the settings' own precedence (`settings::upscaler_after_ini`), read in
//! `main` before the renderer exists, because Streamline must be loaded before the Vulkan
//! instance is made. Only a DLSS choice loads it ([`UpscalerStartup`]), and the test aid
//! `NVF_DLSS_PRELOAD=1` loads it with FSR or off chosen too (FSR then draws through the
//! interposer). The same INI line is applied again with the other rows at `PostStartup`; both
//! read it by the row's own rule, so they agree. [`follow_request`] then keeps the status.

pub(crate) mod prep;

use bevy::camera::Hdr;
use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass};
use bevy::prelude::*;
use bevy::render::camera::{MipBias, TemporalJitter};

/// The backend kind (`--upscaler off|fsr|dlss`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Upscaler {
    #[default]
    Off,
    Fsr,
    Dlss,
}

impl Upscaler {
    // The tests' list of every kind.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [Upscaler; 3] = [Upscaler::Off, Upscaler::Fsr, Upscaler::Dlss];

    /// The name for log lines (not a settings label: those come from the language files).
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Fsr => "FSR 3.1",
            Self::Dlss => "DLSS",
        }
    }
}

/// The qualities both upscalers offer, in the settings' order. Native AA reconstructs at 1:1
/// (DLSS: DLAA). The `clap` names (`native-aa`, `quality`, `balanced`, `performance`,
/// `ultra-performance`) are the ones `FsrQuality` had.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum UpscaleQuality {
    NativeAa,
    #[default]
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}

impl UpscaleQuality {
    // The tests' list of every quality.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [UpscaleQuality; 5] =
        [UpscaleQuality::NativeAa, UpscaleQuality::Quality, UpscaleQuality::Balanced, UpscaleQuality::Performance, UpscaleQuality::UltraPerformance];

    /// The name the `fsr-upscaling` row saves for FSR at this quality (`graphics::FSR_CHOICES`).
    pub fn name(self) -> &'static str {
        match self {
            Self::NativeAa => "Native AA",
            Self::Quality => "Quality",
            Self::Balanced => "Balanced",
            Self::Performance => "Performance",
            Self::UltraPerformance => "Ultra Performance",
        }
    }
}

/// What the player asks for: the one requested upscaler setting (`Graphics::upscaler`). One
/// value, so FSR and DLSS can never both be asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UpscalerChoice {
    #[default]
    Off,
    Fsr(UpscaleQuality),
    Dlss(UpscaleQuality),
}

impl UpscalerChoice {
    pub fn new(upscaler: Upscaler, quality: UpscaleQuality) -> Self {
        match upscaler {
            Upscaler::Off => Self::Off,
            Upscaler::Fsr => Self::Fsr(quality),
            Upscaler::Dlss => Self::Dlss(quality),
        }
    }

    /// FSR at `quality`, or off: what the old `Graphics.fsr: Option<FsrQuality>` held.
    pub fn from_fsr(quality: Option<UpscaleQuality>) -> Self {
        quality.map_or(Self::Off, Self::Fsr)
    }

    pub fn upscaler(self) -> Upscaler {
        match self {
            Self::Off => Upscaler::Off,
            Self::Fsr(_) => Upscaler::Fsr,
            Self::Dlss(_) => Upscaler::Dlss,
        }
    }

    pub fn quality(self) -> Option<UpscaleQuality> {
        match self {
            Self::Off => None,
            Self::Fsr(q) | Self::Dlss(q) => Some(q),
        }
    }

    /// The quality when this is FSR; `None` for off and for DLSS.
    pub fn fsr(self) -> Option<UpscaleQuality> {
        match self {
            Self::Fsr(q) => Some(q),
            _ => None,
        }
    }

    /// The quality when this is DLSS.
    pub fn dlss(self) -> Option<UpscaleQuality> {
        match self {
            Self::Dlss(q) => Some(q),
            _ => None,
        }
    }

    pub fn is_dlss(self) -> bool {
        matches!(self, Self::Dlss(_))
    }

    /// For log lines: `Off`, `FSR 3.1 Quality`, `DLSS Native AA`.
    pub fn describe(self) -> String {
        match self.quality() {
            None => self.upscaler().name().to_string(),
            Some(q) => format!("{} {}", self.upscaler().name(), q.name()),
        }
    }
}

/// Why an upscaler cannot run. The keys are stable (logs, tests, the settings' reasons).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unavailable {
    /// DLSS through Streamline is Windows only.
    NotWindows,
    /// The renderer's backends leave Vulkan out (`WGPU_BACKEND`), or the device is not Vulkan.
    NotVulkan,
    /// The DLSS folder lacks one of its four files (the log names them).
    FilesMissing,
    /// The DLSS folder holds an `nvngx_update.exe`, which Streamline would start unchecked
    /// (docs/spec-notes/streamline.md, "NVIDIA's updater"), or could not be inspected.
    UpdaterInFolder,
    /// The interposer's signature did not verify.
    SignatureRejected,
    /// Streamline would not load or initialise (`LoadLibrary`, exports, `slInit`, the instance).
    StreamlineRefused,
    /// A GPU Bevy might pick lacks what Streamline's device proxy needs, or the pick cannot be
    /// told (the start-up device check).
    DeviceUnsuitable,
    /// DLSS is not supported on this GPU and driver (`slIsFeatureSupported`, no NVIDIA RTX GPU).
    GpuNotSupported,
    /// DLSS stopped producing output in this session (persistent evaluate failure).
    // Set by DLSS's own pass (the design note's step 5); remove this once it does.
    #[allow(dead_code)]
    EvaluateFailed,
    /// FSR 3.1 cannot run here (not Vulkan, or the GPU lacks its formats and limits).
    FsrUnsupported,
}

impl Unavailable {
    pub fn key(self) -> &'static str {
        match self {
            Self::NotWindows => "not-windows",
            Self::NotVulkan => "not-vulkan",
            Self::FilesMissing => "files-missing",
            Self::UpdaterInFolder => "updater-in-folder",
            Self::SignatureRejected => "signature-rejected",
            Self::StreamlineRefused => "streamline-refused",
            Self::DeviceUnsuitable => "device-unsuitable",
            Self::GpuNotSupported => "gpu-not-supported",
            Self::EvaluateFailed => "evaluate-failed",
            Self::FsrUnsupported => "fsr-unsupported",
        }
    }

    /// The reason in the settings' words (English; the page translates it as it draws it,
    /// `lang/*.toml`). NVForever's own words [design].
    pub fn words(self) -> &'static str {
        match self {
            Self::NotWindows => "DLSS needs Windows",
            Self::NotVulkan => "DLSS needs the Vulkan renderer",
            Self::FilesMissing => "DLSS files missing: put them in the dlss folder beside nvforever.exe, then restart",
            Self::UpdaterInFolder => "The dlss folder holds NVIDIA's updater (nvngx_update.exe): remove it, then restart",
            Self::SignatureRejected => "The DLSS files are not signed by NVIDIA",
            Self::StreamlineRefused => "DLSS did not start (the log says why)",
            Self::DeviceUnsuitable => "A graphics card the game may use cannot run DLSS",
            Self::GpuNotSupported => "Not on this GPU or driver (DLSS needs an NVIDIA RTX card)",
            Self::EvaluateFailed => "DLSS stopped working this session; it is tried again at the next start",
            Self::FsrUnsupported => "FSR 3.1 cannot run on this GPU",
        }
    }
}

/// Whether DLSS can run in this session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DlssState {
    /// Streamline was not loaded at start (DLSS was not asked for): a restart may bring it.
    #[default]
    NotLoaded,
    /// Streamline is loaded, DLSS is supported and its functions were fetched.
    Ready,
    /// DLSS cannot run, for this reason.
    Unavailable(Unavailable),
}

/// What this session can do, filled at start and as things change ([`session_now`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// Streamline was loaded and initialised at start (the interposer made the device).
    pub streamline_loaded: bool,
    pub dlss: DlssState,
    /// Why FSR cannot run, or `None` when it can (`fsr::FsrSupport`).
    pub fsr: Option<Unavailable>,
}

/// What the start-up loaded Streamline for: the line a restart crosses [design:
/// docs/design/dlss-upscaling.md, "Restart to enter or leave DLSS"]. Fixed for the session,
/// kept apart from the latest effective choice ([`UpscalerStatus`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    /// Streamline was loaded for DLSS and DLSS drew at start, at this quality. Leaving DLSS
    /// takes a restart, also once DLSS has stopped working (then native draws until it).
    Dlss(UpscaleQuality),
    /// Streamline was loaded without DLSS drawing at start (`NVF_DLSS_PRELOAD=1`, or DLSS asked
    /// for but refused once the device was made through the interposer): FSR and off follow at
    /// once (FSR through the interposer); DLSS waits for a restart.
    Preload,
    /// Streamline was not loaded: FSR and off follow at once; DLSS waits for a restart, unless
    /// DLSS was asked for at this start already (`dlss_tried`), when a restart would do the same.
    Stock { dlss_tried: bool },
}

impl Boundary {
    /// The session's boundary from what was asked for at start and what the start-up did.
    pub fn at_start(start: UpscalerChoice, session: Session) -> Self {
        match start {
            UpscalerChoice::Dlss(q) if session.streamline_loaded && session.dlss == DlssState::Ready => Boundary::Dlss(q),
            _ if session.streamline_loaded => Boundary::Preload,
            _ => Boundary::Stock { dlss_tried: start.is_dlss() },
        }
    }
}

/// What a requested choice comes to in a session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resolution {
    /// What draws: the request, the choice kept until a restart, or `Off` (native) when the
    /// request cannot run.
    pub effective: UpscalerChoice,
    /// The request differs from what draws only until the next start.
    pub restart_pending: bool,
    /// Why the request (or the DLSS still drawing) cannot run.
    pub reason: Option<Unavailable>,
}

/// The rule from the requested choice to what draws [design: docs/design/dlss-upscaling.md,
/// "Restart to enter or leave DLSS"]. `previous` is the last effective choice (`None` at
/// start), `boundary` what the start-up loaded Streamline for ([`Boundary`]).
///
/// - A DLSS session (`Boundary::Dlss`), DLSS working: DLSS asked for, its quality follows at
///   once; FSR or off asked for, DLSS keeps drawing until a restart.
/// - A DLSS session, DLSS no longer working: native (`Off`) with the reason; FSR or off asked
///   for still wait for a restart (the restart pending and the reason stay however often it is
///   asked again).
/// - Any other session: DLSS asked for, what draws stays; a restart is pending unless DLSS is
///   known to be unavailable (then the reason: a restart would not help). FSR or off asked
///   for: follows at once (FSR only where it can run).
pub fn resolve(requested: UpscalerChoice, previous: Option<UpscalerChoice>, boundary: Boundary, session: Session) -> Resolution {
    let dlss_reason = dlss_reason(boundary, session);
    let fsr_or_off = |requested: UpscalerChoice| match requested {
        UpscalerChoice::Fsr(_) => match session.fsr {
            None => Resolution { effective: requested, restart_pending: false, reason: None },
            Some(why) => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(why) },
        },
        _ => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: None },
    };
    match boundary {
        Boundary::Dlss(start_quality) => match (dlss_reason, requested) {
            (None, UpscalerChoice::Dlss(_)) => Resolution { effective: requested, restart_pending: false, reason: None },
            (None, _) => {
                let drawing = previous.filter(|p| p.is_dlss()).unwrap_or(UpscalerChoice::Dlss(start_quality));
                Resolution { effective: drawing, restart_pending: true, reason: None }
            }
            (Some(why), UpscalerChoice::Dlss(_)) => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(why) },
            (Some(why), _) => Resolution { effective: UpscalerChoice::Off, restart_pending: true, reason: Some(why) },
        },
        Boundary::Preload | Boundary::Stock { .. } => match requested {
            UpscalerChoice::Dlss(_) => {
                let drawing = previous.filter(|p| !p.is_dlss()).unwrap_or(UpscalerChoice::Off);
                Resolution { effective: drawing, restart_pending: dlss_reason.is_none(), reason: dlss_reason }
            }
            _ => fsr_or_off(requested),
        },
    }
}

/// Why DLSS cannot run in this session, or `None` when it can or a restart may bring it.
fn dlss_reason(boundary: Boundary, session: Session) -> Option<Unavailable> {
    match session.dlss {
        DlssState::Ready => None,
        DlssState::Unavailable(why) => Some(why),
        // DLSS asked for at this start without Streamline loaded: something refused it before
        // the loader said why (another restart would do the same). Otherwise a restart may load it.
        DlssState::NotLoaded => match boundary {
            Boundary::Stock { dlss_tried: true } => Some(Unavailable::StreamlineRefused),
            _ => None,
        },
    }
}

/// The session's facts and what the requested choice comes to (docs/design/dlss-upscaling.md,
/// "Internal interfaces"). The requested choice itself stays in `Graphics::upscaler`; the
/// status is kept by [`follow_request`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpscalerStatus {
    pub session: Session,
    /// What the start-up loaded Streamline for; `None` until the first [`Self::update`] when
    /// the status was made without it ([`Self::new`]), then from that first request.
    boundary: Option<Boundary>,
    /// The last [`resolve`], `None` before the first.
    resolved: Option<Resolution>,
}

impl UpscalerStatus {
    /// A status whose boundary comes from the first request ([`Self::update`]).
    pub fn new(session: Session) -> Self {
        Self { session, boundary: None, resolved: None }
    }

    /// A status for a session that started with `start` asked for ([`UpscalerStartup`]).
    pub fn with_start(session: Session, start: UpscalerChoice) -> Self {
        Self { session, boundary: Some(Boundary::at_start(start, session)), resolved: None }
    }

    /// Applies [`resolve`] to `requested`, the previous effective choice carried over and the
    /// session's boundary kept. Returns whether anything changed.
    pub fn update(&mut self, requested: UpscalerChoice) -> bool {
        let session = self.session;
        let boundary = *self.boundary.get_or_insert_with(|| Boundary::at_start(requested, session));
        let next = resolve(requested, self.resolved.map(|r| r.effective), boundary, session);
        let changed = self.resolved != Some(next);
        self.resolved = Some(next);
        changed
    }

    /// What draws (`Off` before the first [`Self::update`]).
    pub fn effective(&self) -> UpscalerChoice {
        self.resolved.map_or(UpscalerChoice::Off, |r| r.effective)
    }

    pub fn restart_pending(&self) -> bool {
        self.resolved.is_some_and(|r| r.restart_pending)
    }

    pub fn reason(&self) -> Option<Unavailable> {
        self.resolved.and_then(|r| r.reason)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn boundary(&self) -> Option<Boundary> {
        self.boundary
    }

    /// Why DLSS cannot be asked for in this session (the settings hold its choices out of
    /// reach), by the rule [`resolve`] uses; `None` when it can, or when a restart may bring
    /// it.
    pub fn dlss_unavailable(&self) -> Option<Unavailable> {
        dlss_reason(self.boundary.unwrap_or(Boundary::Stock { dlss_tried: false }), self.session)
    }

    /// For the log: what is asked for, what draws, a restart pending and the reason.
    pub fn describe(&self, requested: UpscalerChoice) -> String {
        let mut line = format!("{} asked for, {} draws", requested.describe(), self.effective().describe());
        if self.restart_pending() {
            line.push_str(" until the next start");
        }
        if let Some(why) = self.reason() {
            line.push_str(&format!(" ({})", why.key()));
        }
        line
    }
}

/// What the start-up chose, before the renderer was made (`main`; module notes): the
/// requested choice the start resolved and whether `NVF_DLSS_PRELOAD=1` loads Streamline
/// without DLSS. The status's [`Boundary`] comes from it.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpscalerStartup {
    pub requested: UpscalerChoice,
    pub preload: bool,
}

/// The test aid that loads Streamline at start without DLSS chosen (FSR then draws through
/// the interposer). Only `1` turns it on.
pub const PRELOAD_ENV: &str = "NVF_DLSS_PRELOAD";

impl UpscalerStartup {
    /// `preload_var`: `NVF_DLSS_PRELOAD`'s value.
    pub fn new(requested: UpscalerChoice, preload_var: Option<&std::ffi::OsStr>) -> Self {
        Self { requested, preload: preload_var.is_some_and(|v| v == "1") }
    }

    /// Streamline is loaded before the renderer: DLSS chosen, or the preload aid.
    pub fn loads_streamline(&self) -> bool {
        self.requested.is_dlss() || self.preload
    }

    /// For the start's log line.
    pub fn describe(&self) -> String {
        match (self.requested.is_dlss(), self.preload) {
            (true, _) => format!("{} chosen at start: Streamline loaded before the renderer", self.requested.describe()),
            (false, true) => format!("{} chosen at start; {PRELOAD_ENV}=1: Streamline loaded before the renderer all the same", self.requested.describe()),
            (false, false) => format!("{} chosen at start: Streamline not loaded (DLSS takes a restart)", self.requested.describe()),
        }
    }
}

/// The requested choice from the command line: `--upscaler off|fsr|dlss` with
/// `--upscaler-quality <q>` (default `quality`), or the older `--fsr <mode>` (`None`: not
/// given, unlike `--fsr off`). Both given must say the same: `--fsr quality --upscaler fsr`
/// agree, `--fsr quality --upscaler dlss` or `--fsr off --upscaler dlss` do not.
pub fn command_line_choice(fsr: Option<crate::graphics::FsrMode>, upscaler: Option<Upscaler>, quality: Option<UpscaleQuality>) -> Result<UpscalerChoice, String> {
    let from_fsr = fsr.map(|m| UpscalerChoice::from_fsr(m.quality()));
    let Some(upscaler) = upscaler else {
        if quality.is_some() {
            return Err("--upscaler-quality needs --upscaler fsr or --upscaler dlss".into());
        }
        return Ok(from_fsr.unwrap_or_default());
    };
    if upscaler == Upscaler::Off && quality.is_some() {
        return Err("--upscaler off takes no --upscaler-quality".into());
    }
    let chosen = UpscalerChoice::new(upscaler, quality.or_else(|| from_fsr.and_then(UpscalerChoice::quality).filter(|_| upscaler == Upscaler::Fsr)).unwrap_or_default());
    match from_fsr {
        Some(legacy) if legacy != chosen => Err(format!(
            "--fsr asks for {} but --upscaler for {}: give one of them, or both alike",
            legacy.describe(),
            chosen.describe()
        )),
        _ => Ok(chosen),
    }
}

/// What DLSS can do now: the loader's state once it ran (`dlss::loader_state`), else what is
/// known without loading anything (not Windows, Vulkan left out of the renderer's backends).
pub fn dlss_state_now() -> DlssState {
    match crate::dlss::loader_state() {
        DlssState::NotLoaded => before_loading(cfg!(windows), vulkan_in_backends()),
        state => state,
    }
}

/// The renderer's backends include Vulkan, read as Bevy reads them (`WGPU_BACKEND`, as
/// `dlss/win.rs` `start` does).
fn vulkan_in_backends() -> bool {
    static VULKAN: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all()).contains(wgpu::Backends::VULKAN));
    *VULKAN
}

/// DLSS's state in a session that did not load Streamline: unavailable where no restart could
/// bring it, else not loaded.
fn before_loading(windows: bool, vulkan: bool) -> DlssState {
    if !windows {
        DlssState::Unavailable(Unavailable::NotWindows)
    } else if !vulkan {
        DlssState::Unavailable(Unavailable::NotVulkan)
    } else {
        DlssState::NotLoaded
    }
}

/// The session's facts now: DLSS as [`dlss_state_now`] says, FSR as `fsr::FsrSupport` says.
/// Streamline counts as loaded while DLSS is ready.
pub fn session_now(fsr_supported: bool) -> Session {
    let dlss = dlss_state_now();
    Session { streamline_loaded: dlss == DlssState::Ready, dlss, fsr: (!fsr_supported).then_some(Unavailable::FsrUnsupported) }
}

/// Keeps [`UpscalerStatus`] with the request (`Graphics::upscaler`) and the session's facts,
/// before `graphics::apply` puts the effective choice on the camera. Made on the first frame
/// (after the INI is applied at `PostStartup`), its boundary from [`UpscalerStartup`].
pub fn follow_request(
    mut commands: Commands,
    graphics: Res<crate::graphics::Graphics>,
    fsr: Option<Res<crate::fsr::FsrSupport>>,
    start: Option<Res<UpscalerStartup>>,
    status: Option<ResMut<UpscalerStatus>>,
) {
    let session = session_now(fsr.is_some_and(|s| s.0));
    let requested = graphics.upscaler;
    match status {
        None => {
            let mut made = match start {
                Some(start) => UpscalerStatus::with_start(session, start.requested),
                None => UpscalerStatus::new(session),
            };
            made.update(requested);
            info!("upscaler: {}", made.describe(requested));
            commands.insert_resource(made);
        }
        Some(mut status) => {
            let mut next = *status;
            next.session = session;
            let changed = next.update(requested);
            if changed {
                info!("upscaler: {}", next.describe(requested));
            }
            if next != *status {
                *status = next;
            }
        }
    }
}

/// DLSS on a perspective HDR camera, the counterpart of `fsr::FsrUpscaling`: the common
/// preparation ([`prep`]) takes the camera, and DLSS draws it from step 5. The graphics
/// settings put it on the main camera while DLSS is the effective choice
/// ([`UpscalerStatus::effective`]) and enforce `Msaa::Off` and no other anti-aliasing, as for
/// FSR. DLSS sharpens nothing itself: the separate sharpening pass stays available.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[require(Hdr, DepthPrepass, MotionVectorPrepass, TemporalJitter, MipBias)]
pub struct DlssUpscaling {
    pub quality: UpscaleQuality,
}

/// The temporal upscaler the main camera draws with: the status's effective choice, for every
/// backend, once there is a status; without one (before [`follow_request`] makes it, and in
/// tests) FSR's rule as it always was: FSR while it is asked for and `fsr_supported`, else off.
pub fn effective_temporal(requested_fsr: Option<UpscaleQuality>, fsr_supported: bool, status: Option<&UpscalerStatus>) -> UpscalerChoice {
    match status {
        Some(status) => status.effective(),
        None => UpscalerChoice::from_fsr(requested_fsr.filter(|_| fsr_supported)),
    }
}

/// [`effective_temporal`] for this world.
pub fn effective(world: &World) -> UpscalerChoice {
    let fsr_supported = world.get_resource::<crate::fsr::FsrSupport>().is_some_and(|s| s.0);
    effective_temporal(world.resource::<crate::graphics::Graphics>().fsr(), fsr_supported, world.get_resource::<UpscalerStatus>())
}

/// Whether a temporal upscaler (FSR or DLSS) draws the main camera on this device: the
/// question for what both need (the HDR view, the prepasses, cut-outs drawn alpha-tested, the
/// native anti-aliasing replaced). `graphics::fsr_active` is FSR's own.
pub fn temporal_active(world: &World) -> bool {
    effective(world) != UpscalerChoice::Off
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;
    use UpscaleQuality::*;

    const READY: Session = Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None };
    const STOCK: Session = Session { streamline_loaded: false, dlss: DlssState::NotLoaded, fsr: None };

    fn ok(effective: UpscalerChoice) -> Resolution {
        Resolution { effective, restart_pending: false, reason: None }
    }

    #[test]
    fn off_is_the_default_everywhere() {
        assert_eq!(Upscaler::default(), Upscaler::Off);
        assert_eq!(UpscalerChoice::default(), UpscalerChoice::Off);
        assert_eq!(UpscaleQuality::default(), Quality, "FSR's old default quality");
        assert_eq!(UpscalerStatus::default().effective(), UpscalerChoice::Off);
        assert!(!UpscalerStatus::default().restart_pending());
        assert_eq!(Session::default().dlss, DlssState::NotLoaded);
    }

    #[test]
    fn command_line_names_parse() {
        for (name, upscaler) in [("off", Upscaler::Off), ("fsr", Upscaler::Fsr), ("dlss", Upscaler::Dlss)] {
            assert_eq!(Upscaler::from_str(name, true), Ok(upscaler));
        }
        assert!(Upscaler::from_str("xess", true).is_err());
        let names: Vec<String> = UpscaleQuality::value_variants().iter().map(|q| q.to_possible_value().unwrap().get_name().to_string()).collect();
        assert_eq!(names, ["native-aa", "quality", "balanced", "performance", "ultra-performance"], "the names FsrQuality had");
        assert_eq!(UpscaleQuality::value_variants(), UpscaleQuality::ALL);
    }

    #[test]
    fn quality_names_are_the_fsr_rows_saved_names() {
        let fsr: Vec<&str> = crate::graphics::FSR_CHOICES.iter().filter(|(_, m)| m.quality().is_some()).map(|(n, _)| *n).collect();
        let ours: Vec<&str> = UpscaleQuality::ALL.iter().map(|q| q.name()).collect();
        assert_eq!(fsr, ours);
        for (name, mode) in crate::graphics::FSR_CHOICES {
            if let Some(q) = mode.quality() {
                assert_eq!(q.name(), name);
            }
        }
    }

    #[test]
    fn choice_round_trips_its_parts() {
        for upscaler in Upscaler::ALL {
            for quality in UpscaleQuality::ALL {
                let choice = UpscalerChoice::new(upscaler, quality);
                assert_eq!(choice.upscaler(), upscaler);
                assert_eq!(choice.quality(), (upscaler != Upscaler::Off).then_some(quality));
                assert_eq!(choice.fsr(), (upscaler == Upscaler::Fsr).then_some(quality));
                assert_eq!(choice.dlss(), (upscaler == Upscaler::Dlss).then_some(quality));
                assert_eq!(choice.is_dlss(), upscaler == Upscaler::Dlss);
            }
        }
        assert_eq!(UpscalerChoice::from_fsr(None), UpscalerChoice::Off);
        assert_eq!(UpscalerChoice::from_fsr(Some(Balanced)), UpscalerChoice::Fsr(Balanced));
        assert_eq!(UpscalerChoice::from_fsr(Some(Performance)).fsr(), Some(Performance));
        assert_eq!(UpscalerChoice::Off.describe(), "Off");
        assert_eq!(UpscalerChoice::Fsr(Quality).describe(), "FSR 3.1 Quality");
        assert_eq!(UpscalerChoice::Dlss(NativeAa).describe(), "DLSS Native AA");
    }

    #[test]
    fn reason_keys_are_distinct() {
        let all = [
            Unavailable::NotWindows,
            Unavailable::NotVulkan,
            Unavailable::FilesMissing,
            Unavailable::UpdaterInFolder,
            Unavailable::SignatureRejected,
            Unavailable::StreamlineRefused,
            Unavailable::DeviceUnsuitable,
            Unavailable::GpuNotSupported,
            Unavailable::EvaluateFailed,
            Unavailable::FsrUnsupported,
        ];
        let keys: std::collections::HashSet<&str> = all.iter().map(|r| r.key()).collect();
        assert_eq!(keys.len(), all.len());
    }

    /// A session that loaded Streamline for DLSS at this quality.
    fn dlss_session(q: UpscaleQuality) -> Boundary {
        Boundary::at_start(UpscalerChoice::Dlss(q), READY)
    }

    const STOCK_BOUNDARY: Boundary = Boundary::Stock { dlss_tried: false };

    #[test]
    fn the_boundary_is_what_streamline_was_loaded_for() {
        assert_eq!(Boundary::at_start(UpscalerChoice::Dlss(Balanced), READY), Boundary::Dlss(Balanced));
        assert_eq!(Boundary::at_start(UpscalerChoice::Fsr(Quality), READY), Boundary::Preload, "the preload aid");
        assert_eq!(Boundary::at_start(UpscalerChoice::Off, READY), Boundary::Preload);
        let refused_after_device = Session { streamline_loaded: true, dlss: DlssState::Unavailable(Unavailable::GpuNotSupported), fsr: None };
        assert_eq!(Boundary::at_start(UpscalerChoice::Dlss(Quality), refused_after_device), Boundary::Preload, "the interposer made the device, DLSS never drew");
        assert_eq!(Boundary::at_start(UpscalerChoice::Fsr(Quality), STOCK), STOCK_BOUNDARY);
        let missing = Session { dlss: DlssState::Unavailable(Unavailable::FilesMissing), ..STOCK };
        assert_eq!(Boundary::at_start(UpscalerChoice::Dlss(Quality), missing), Boundary::Stock { dlss_tried: true });
    }

    #[test]
    fn at_start_the_request_runs_when_its_backend_can() {
        let at_start = |requested: UpscalerChoice, session: Session| resolve(requested, None, Boundary::at_start(requested, session), session);
        assert_eq!(at_start(UpscalerChoice::Off, STOCK), ok(UpscalerChoice::Off));
        assert_eq!(at_start(UpscalerChoice::Fsr(Quality), STOCK), ok(UpscalerChoice::Fsr(Quality)));
        assert_eq!(at_start(UpscalerChoice::Dlss(Balanced), READY), ok(UpscalerChoice::Dlss(Balanced)));
        let no_fsr = Session { fsr: Some(Unavailable::FsrUnsupported), ..STOCK };
        assert_eq!(at_start(UpscalerChoice::Fsr(Quality), no_fsr), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::FsrUnsupported) });
        let missing = Session { dlss: DlssState::Unavailable(Unavailable::FilesMissing), ..STOCK };
        assert_eq!(at_start(UpscalerChoice::Dlss(Quality), missing), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::FilesMissing) }, "native, not FSR");
        assert_eq!(at_start(UpscalerChoice::Dlss(Quality), STOCK), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::StreamlineRefused) });
        // FSR with Streamline loaded at start (the preload test aid).
        assert_eq!(at_start(UpscalerChoice::Fsr(Performance), READY), ok(UpscalerChoice::Fsr(Performance)));
    }

    #[test]
    fn leaving_dlss_waits_for_a_restart_and_its_quality_follows_at_once() {
        let running = Some(UpscalerChoice::Dlss(Quality));
        let session = dlss_session(Quality);
        assert_eq!(resolve(UpscalerChoice::Dlss(Performance), running, session, READY), ok(UpscalerChoice::Dlss(Performance)));
        for requested in [UpscalerChoice::Off, UpscalerChoice::Fsr(Quality)] {
            assert_eq!(resolve(requested, running, session, READY), Resolution { effective: UpscalerChoice::Dlss(Quality), restart_pending: true, reason: None });
        }
        // The first request already another (a test run's command line kept over the INI): the
        // start's DLSS quality draws until the restart.
        assert_eq!(resolve(UpscalerChoice::Fsr(Quality), None, dlss_session(Balanced), READY), Resolution { effective: UpscalerChoice::Dlss(Balanced), restart_pending: true, reason: None });
    }

    #[test]
    fn entering_dlss_keeps_what_draws_until_a_restart() {
        for previous in [UpscalerChoice::Off, UpscalerChoice::Fsr(Balanced)] {
            assert_eq!(resolve(UpscalerChoice::Dlss(Quality), Some(previous), STOCK_BOUNDARY, STOCK), Resolution { effective: previous, restart_pending: true, reason: None });
            assert_eq!(resolve(UpscalerChoice::Dlss(Quality), Some(previous), Boundary::Preload, READY), Resolution { effective: previous, restart_pending: true, reason: None }, "preload: DLSS still takes a restart");
            let refused = Session { dlss: DlssState::Unavailable(Unavailable::NotVulkan), ..STOCK };
            assert_eq!(resolve(UpscalerChoice::Dlss(Quality), Some(previous), STOCK_BOUNDARY, refused), Resolution { effective: previous, restart_pending: false, reason: Some(Unavailable::NotVulkan) }, "a restart would not help");
        }
    }

    #[test]
    fn fsr_and_off_stay_live_without_a_dlss_session() {
        for (boundary, session) in [(STOCK_BOUNDARY, STOCK), (Boundary::Preload, READY), (Boundary::Stock { dlss_tried: true }, STOCK)] {
            assert_eq!(resolve(UpscalerChoice::Off, Some(UpscalerChoice::Fsr(Quality)), boundary, session), ok(UpscalerChoice::Off));
            assert_eq!(resolve(UpscalerChoice::Fsr(UltraPerformance), Some(UpscalerChoice::Off), boundary, session), ok(UpscalerChoice::Fsr(UltraPerformance)));
        }
    }

    #[test]
    fn dlss_that_stops_working_falls_back_to_native() {
        let failed = Session { dlss: DlssState::Unavailable(Unavailable::EvaluateFailed), ..READY };
        let running = Some(UpscalerChoice::Dlss(Quality));
        let session = dlss_session(Quality);
        assert_eq!(resolve(UpscalerChoice::Dlss(Quality), running, session, failed), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::EvaluateFailed) });
        assert_eq!(resolve(UpscalerChoice::Fsr(Quality), running, session, failed), Resolution { effective: UpscalerChoice::Off, restart_pending: true, reason: Some(Unavailable::EvaluateFailed) });
    }

    /// Review P2 (2026-11-12): after DLSS fails, every later update keeps the restart pending
    /// and the reason; FSR never starts in a DLSS session.
    #[test]
    fn repeated_updates_after_a_dlss_failure_keep_the_restart_and_the_reason() {
        let mut status = UpscalerStatus::with_start(READY, UpscalerChoice::Dlss(Quality));
        assert!(status.update(UpscalerChoice::Dlss(Quality)));
        assert_eq!(status.effective(), UpscalerChoice::Dlss(Quality));
        status.session.dlss = DlssState::Unavailable(Unavailable::EvaluateFailed);
        assert!(status.update(UpscalerChoice::Dlss(Quality)));
        assert_eq!((status.effective(), status.restart_pending(), status.reason()), (UpscalerChoice::Off, false, Some(Unavailable::EvaluateFailed)));
        for _ in 0..3 {
            for requested in [UpscalerChoice::Off, UpscalerChoice::Fsr(Quality), UpscalerChoice::Fsr(Balanced)] {
                status.update(requested);
                assert_eq!(status.effective(), UpscalerChoice::Off, "{requested:?}: native, not FSR, in a DLSS session");
                assert!(status.restart_pending(), "{requested:?}: leaving DLSS still waits for the restart");
                assert_eq!(status.reason(), Some(Unavailable::EvaluateFailed));
            }
            status.update(UpscalerChoice::Dlss(Performance));
            assert_eq!((status.effective(), status.restart_pending(), status.reason()), (UpscalerChoice::Off, false, Some(Unavailable::EvaluateFailed)), "back to DLSS: nothing pending, the reason stays");
        }
        assert_eq!(status.boundary(), Some(Boundary::Dlss(Quality)));
        assert_eq!(status.dlss_unavailable(), Some(Unavailable::EvaluateFailed));
    }

    #[test]
    fn dlss_is_unavailable_to_the_settings_by_the_same_rule() {
        assert_eq!(UpscalerStatus::new(STOCK).dlss_unavailable(), None, "a restart may load it");
        assert_eq!(UpscalerStatus::new(READY).dlss_unavailable(), None);
        assert_eq!(UpscalerStatus::with_start(STOCK, UpscalerChoice::Dlss(Quality)).dlss_unavailable(), Some(Unavailable::StreamlineRefused), "tried at this start");
        let missing = Session { dlss: DlssState::Unavailable(Unavailable::FilesMissing), ..STOCK };
        assert_eq!(UpscalerStatus::new(missing).dlss_unavailable(), Some(Unavailable::FilesMissing));
    }

    /// Review P2 (2026-11-12): with a status, FSR keeps drawing while DLSS waits for a restart.
    #[test]
    fn the_camera_draws_the_status_effective_choice_for_every_backend() {
        // No status (before follow_request makes it, and tests without one): FSR's rule.
        assert_eq!(effective_temporal(Some(Quality), true, None), UpscalerChoice::Fsr(Quality));
        assert_eq!(effective_temporal(Some(Quality), false, None), UpscalerChoice::Off, "unsupported FSR draws natively");
        assert_eq!(effective_temporal(None, true, None), UpscalerChoice::Off);
        // DLSS drawing: DLSS, whatever FSR request waits for the restart.
        let mut status = UpscalerStatus::new(READY);
        status.update(UpscalerChoice::Dlss(Balanced));
        assert_eq!(effective_temporal(None, true, Some(&status)), UpscalerChoice::Dlss(Balanced));
        status.update(UpscalerChoice::Fsr(Quality));
        assert!(status.restart_pending());
        assert_eq!(effective_temporal(Some(Quality), true, Some(&status)), UpscalerChoice::Dlss(Balanced), "never FSR and DLSS at once");
        // FSR drawing, DLSS asked for: the request's FSR quality is gone (Graphics::fsr is
        // None) but FSR keeps drawing until the restart, as the status says.
        let mut stock = UpscalerStatus::new(STOCK);
        stock.update(UpscalerChoice::Fsr(Balanced));
        assert_eq!(effective_temporal(Some(Balanced), true, Some(&stock)), UpscalerChoice::Fsr(Balanced));
        stock.update(UpscalerChoice::Dlss(Quality));
        assert!(stock.restart_pending());
        assert_eq!(effective_temporal(None, true, Some(&stock)), UpscalerChoice::Fsr(Balanced), "restart pending: FSR keeps drawing");
        stock.update(UpscalerChoice::Off);
        assert_eq!(effective_temporal(None, true, Some(&stock)), UpscalerChoice::Off);
    }

    #[test]
    fn status_carries_the_effective_choice_over() {
        let mut status = UpscalerStatus::new(READY);
        assert!(status.update(UpscalerChoice::Dlss(Quality)));
        assert!(!status.update(UpscalerChoice::Dlss(Quality)), "nothing changed");
        assert!(status.update(UpscalerChoice::Off));
        assert_eq!(status.effective(), UpscalerChoice::Dlss(Quality));
        assert!(status.restart_pending());
        assert!(status.update(UpscalerChoice::Dlss(Balanced)));
        assert_eq!(status.effective(), UpscalerChoice::Dlss(Balanced));
        assert!(!status.restart_pending());
        assert_eq!(status.reason(), None);
        assert!(status.describe(UpscalerChoice::Dlss(Balanced)).contains("DLSS Balanced draws"));
    }

    #[test]
    fn a_status_made_with_the_start_keeps_its_boundary_whatever_comes_first() {
        // The start chose FSR with the preload aid; the first request seen is DLSS: still a
        // preload session, so DLSS waits for a restart.
        let mut status = UpscalerStatus::with_start(READY, UpscalerChoice::Fsr(Quality));
        status.update(UpscalerChoice::Dlss(Quality));
        assert_eq!(status.boundary(), Some(Boundary::Preload));
        assert!(status.restart_pending());
        assert_eq!(status.effective(), UpscalerChoice::Off);
    }

    #[test]
    fn dlss_unavailable_before_loading_only_where_no_restart_brings_it() {
        assert_eq!(before_loading(false, true), DlssState::Unavailable(Unavailable::NotWindows));
        assert_eq!(before_loading(true, false), DlssState::Unavailable(Unavailable::NotVulkan));
        assert_eq!(before_loading(true, true), DlssState::NotLoaded);
        let session = Session { streamline_loaded: false, dlss: before_loading(true, false), fsr: None };
        assert_eq!(resolve(UpscalerChoice::Dlss(Quality), Some(UpscalerChoice::Off), STOCK_BOUNDARY, session).reason, Some(Unavailable::NotVulkan));
        assert_eq!(session_now(false).fsr, Some(Unavailable::FsrUnsupported));
        assert_eq!(session_now(true).fsr, None);
    }

    #[test]
    fn the_command_line_choice_and_its_conflicts() {
        use crate::graphics::FsrMode;
        let cli = command_line_choice;
        assert_eq!(cli(None, None, None), Ok(UpscalerChoice::Off), "nothing given: off");
        assert_eq!(cli(Some(FsrMode::Balanced), None, None), Ok(UpscalerChoice::Fsr(Balanced)), "--fsr as before");
        assert_eq!(cli(Some(FsrMode::Off), None, None), Ok(UpscalerChoice::Off));
        assert_eq!(cli(None, Some(Upscaler::Dlss), None), Ok(UpscalerChoice::Dlss(Quality)), "quality by default");
        assert_eq!(cli(None, Some(Upscaler::Dlss), Some(UltraPerformance)), Ok(UpscalerChoice::Dlss(UltraPerformance)));
        assert_eq!(cli(None, Some(Upscaler::Fsr), Some(NativeAa)), Ok(UpscalerChoice::Fsr(NativeAa)));
        assert_eq!(cli(None, Some(Upscaler::Off), None), Ok(UpscalerChoice::Off));
        // Both selectors given and agreeing.
        assert_eq!(cli(Some(FsrMode::Performance), Some(Upscaler::Fsr), None), Ok(UpscalerChoice::Fsr(Performance)), "--upscaler fsr takes --fsr's quality");
        assert_eq!(cli(Some(FsrMode::Performance), Some(Upscaler::Fsr), Some(Performance)), Ok(UpscalerChoice::Fsr(Performance)));
        assert_eq!(cli(Some(FsrMode::Off), Some(Upscaler::Off), None), Ok(UpscalerChoice::Off));
        // Conflicts: an explicit --fsr against --upscaler, --fsr off included.
        for (fsr, upscaler, quality) in [
            (Some(FsrMode::Quality), Some(Upscaler::Dlss), None),
            (Some(FsrMode::Off), Some(Upscaler::Dlss), None),
            (Some(FsrMode::Off), Some(Upscaler::Fsr), None),
            (Some(FsrMode::Quality), Some(Upscaler::Off), None),
            (Some(FsrMode::Quality), Some(Upscaler::Fsr), Some(Balanced)),
        ] {
            assert!(cli(fsr, upscaler, quality).is_err(), "{fsr:?} {upscaler:?} {quality:?}");
        }
        assert!(cli(None, None, Some(Quality)).unwrap_err().contains("--upscaler-quality"));
        assert!(cli(Some(FsrMode::Quality), None, Some(Quality)).is_err(), "--upscaler-quality goes with --upscaler only");
        assert!(cli(None, Some(Upscaler::Off), Some(Quality)).is_err());
    }

    #[test]
    fn streamline_is_loaded_at_start_for_dlss_or_the_preload_aid_only() {
        use std::ffi::OsStr;
        assert!(UpscalerStartup::new(UpscalerChoice::Dlss(Quality), None).loads_streamline());
        assert!(!UpscalerStartup::new(UpscalerChoice::Fsr(Quality), None).loads_streamline());
        assert!(!UpscalerStartup::new(UpscalerChoice::Off, None).loads_streamline());
        assert!(UpscalerStartup::new(UpscalerChoice::Fsr(Quality), Some(OsStr::new("1"))).loads_streamline());
        assert!(UpscalerStartup::new(UpscalerChoice::Off, Some(OsStr::new("1"))).preload);
        assert!(!UpscalerStartup::new(UpscalerChoice::Off, Some(OsStr::new("yes"))).preload, "only 1 turns it on");
        assert!(UpscalerStartup::new(UpscalerChoice::Dlss(Quality), None).describe().contains("Streamline loaded"));
    }

    #[test]
    fn every_reason_has_words() {
        for why in [
            Unavailable::NotWindows,
            Unavailable::NotVulkan,
            Unavailable::FilesMissing,
            Unavailable::UpdaterInFolder,
            Unavailable::SignatureRejected,
            Unavailable::StreamlineRefused,
            Unavailable::DeviceUnsuitable,
            Unavailable::GpuNotSupported,
            Unavailable::EvaluateFailed,
            Unavailable::FsrUnsupported,
        ] {
            assert!(!why.words().is_empty() && !why.words().ends_with('.'), "{why:?}");
        }
    }
}
