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
//!
//! Nothing selects DLSS through the settings or the command line yet, and nothing reads the
//! status (the design note's steps 4, 5 and 8 wire them); the types and the rule are here so
//! those steps share them.

// The status and the reasons are wired by later steps (module comment); remove this then.
#![allow(dead_code)]

use bevy::prelude::*;

/// The backend kind (`--upscaler off|fsr|dlss` once the command line takes it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Upscaler {
    #[default]
    Off,
    Fsr,
    Dlss,
}

impl Upscaler {
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

/// What this session can do, filled at start and as things change (later steps).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// Streamline was loaded and initialised at start (the interposer made the device).
    pub streamline_loaded: bool,
    pub dlss: DlssState,
    /// Why FSR cannot run, or `None` when it can (`fsr::FsrSupport`).
    pub fsr: Option<Unavailable>,
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
/// "Restart to enter or leave DLSS"]. `previous` is the last effective choice, `None` at start.
///
/// - At start: the request runs if its backend can, else native (`Off`) with the reason.
/// - DLSS drawing, DLSS asked for: its quality follows at once.
/// - DLSS drawing, FSR or off asked for: DLSS keeps drawing until a restart.
/// - DLSS drawing but no longer able to: native with the reason (leaving DLSS still waits for a
///   restart while Streamline is loaded).
/// - Not drawing DLSS, DLSS asked for: what draws stays; a restart is pending unless DLSS is
///   known to be unavailable (then the reason).
/// - Not drawing DLSS, FSR or off asked for: follows at once (FSR only where it can run).
pub fn resolve(requested: UpscalerChoice, previous: Option<UpscalerChoice>, session: Session) -> Resolution {
    let dlss_reason = match session.dlss {
        DlssState::Ready => None,
        DlssState::Unavailable(why) => Some(why),
        // Asked for DLSS without Streamline loaded: at start that means the start-up did not
        // load it, which only happens when something refused it; later, a restart may.
        DlssState::NotLoaded => Some(Unavailable::StreamlineRefused),
    };
    let fsr_or_off = |requested: UpscalerChoice| match requested {
        UpscalerChoice::Fsr(_) => match session.fsr {
            None => Resolution { effective: requested, restart_pending: false, reason: None },
            Some(why) => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(why) },
        },
        _ => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: None },
    };
    match previous {
        None => match requested {
            UpscalerChoice::Dlss(_) => match dlss_reason {
                None => Resolution { effective: requested, restart_pending: false, reason: None },
                Some(why) => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(why) },
            },
            _ => fsr_or_off(requested),
        },
        Some(UpscalerChoice::Dlss(previous_quality)) => match (dlss_reason, requested) {
            (None, UpscalerChoice::Dlss(_)) => Resolution { effective: requested, restart_pending: false, reason: None },
            (None, _) => Resolution { effective: UpscalerChoice::Dlss(previous_quality), restart_pending: true, reason: None },
            (Some(why), UpscalerChoice::Dlss(_)) => Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(why) },
            (Some(why), _) => Resolution { effective: UpscalerChoice::Off, restart_pending: true, reason: Some(why) },
        },
        Some(previous) => match requested {
            UpscalerChoice::Dlss(_) => match session.dlss {
                DlssState::Unavailable(why) => Resolution { effective: previous, restart_pending: false, reason: Some(why) },
                DlssState::NotLoaded | DlssState::Ready => Resolution { effective: previous, restart_pending: true, reason: None },
            },
            _ => fsr_or_off(requested),
        },
    }
}

/// The session's facts and what the requested choice comes to (docs/design/dlss-upscaling.md,
/// "Internal interfaces"). The requested choice itself stays in `Graphics::upscaler`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpscalerStatus {
    pub session: Session,
    /// The last [`resolve`], `None` before the first.
    resolved: Option<Resolution>,
}

impl UpscalerStatus {
    pub fn new(session: Session) -> Self {
        Self { session, resolved: None }
    }

    /// Applies [`resolve`] to `requested`, the previous effective choice carried over. Returns
    /// whether anything changed.
    pub fn update(&mut self, requested: UpscalerChoice) -> bool {
        let next = resolve(requested, self.resolved.map(|r| r.effective), self.session);
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

    #[test]
    fn at_start_the_request_runs_when_its_backend_can() {
        assert_eq!(resolve(UpscalerChoice::Off, None, STOCK), ok(UpscalerChoice::Off));
        assert_eq!(resolve(UpscalerChoice::Fsr(Quality), None, STOCK), ok(UpscalerChoice::Fsr(Quality)));
        assert_eq!(resolve(UpscalerChoice::Dlss(Balanced), None, READY), ok(UpscalerChoice::Dlss(Balanced)));
        let no_fsr = Session { fsr: Some(Unavailable::FsrUnsupported), ..STOCK };
        assert_eq!(resolve(UpscalerChoice::Fsr(Quality), None, no_fsr), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::FsrUnsupported) });
        let missing = Session { dlss: DlssState::Unavailable(Unavailable::FilesMissing), ..STOCK };
        assert_eq!(resolve(UpscalerChoice::Dlss(Quality), None, missing), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::FilesMissing) }, "native, not FSR");
        assert_eq!(resolve(UpscalerChoice::Dlss(Quality), None, STOCK).effective, UpscalerChoice::Off);
        // FSR with Streamline loaded at start (the preload test path of a later step).
        assert_eq!(resolve(UpscalerChoice::Fsr(Performance), None, READY), ok(UpscalerChoice::Fsr(Performance)));
    }

    #[test]
    fn leaving_dlss_waits_for_a_restart_and_its_quality_follows_at_once() {
        let running = Some(UpscalerChoice::Dlss(Quality));
        assert_eq!(resolve(UpscalerChoice::Dlss(Performance), running, READY), ok(UpscalerChoice::Dlss(Performance)));
        for requested in [UpscalerChoice::Off, UpscalerChoice::Fsr(Quality)] {
            assert_eq!(resolve(requested, running, READY), Resolution { effective: UpscalerChoice::Dlss(Quality), restart_pending: true, reason: None });
        }
    }

    #[test]
    fn entering_dlss_keeps_what_draws_until_a_restart() {
        for previous in [UpscalerChoice::Off, UpscalerChoice::Fsr(Balanced)] {
            assert_eq!(resolve(UpscalerChoice::Dlss(Quality), Some(previous), STOCK), Resolution { effective: previous, restart_pending: true, reason: None });
            let refused = Session { dlss: DlssState::Unavailable(Unavailable::NotVulkan), ..STOCK };
            assert_eq!(resolve(UpscalerChoice::Dlss(Quality), Some(previous), refused), Resolution { effective: previous, restart_pending: false, reason: Some(Unavailable::NotVulkan) }, "a restart would not help");
        }
    }

    #[test]
    fn fsr_and_off_stay_live_without_dlss() {
        assert_eq!(resolve(UpscalerChoice::Off, Some(UpscalerChoice::Fsr(Quality)), STOCK), ok(UpscalerChoice::Off));
        assert_eq!(resolve(UpscalerChoice::Fsr(UltraPerformance), Some(UpscalerChoice::Off), STOCK), ok(UpscalerChoice::Fsr(UltraPerformance)));
    }

    #[test]
    fn dlss_that_stops_working_falls_back_to_native() {
        let failed = Session { dlss: DlssState::Unavailable(Unavailable::EvaluateFailed), ..READY };
        let running = Some(UpscalerChoice::Dlss(Quality));
        assert_eq!(resolve(UpscalerChoice::Dlss(Quality), running, failed), Resolution { effective: UpscalerChoice::Off, restart_pending: false, reason: Some(Unavailable::EvaluateFailed) });
        assert_eq!(resolve(UpscalerChoice::Fsr(Quality), running, failed), Resolution { effective: UpscalerChoice::Off, restart_pending: true, reason: Some(Unavailable::EvaluateFailed) });
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
    }
}
