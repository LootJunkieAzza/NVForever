// Copyright 2016-2018 Mateusz Sieczko and other GilRs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
#![cfg_attr(target_os = "windows", allow(dead_code))]

mod parser;

use crate::ev::{self, Axis, AxisOrBtn, Button};
use gilrs_core::native_ev_codes as nec;
use gilrs_core::EvCode;

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult, Write as _};
use std::fs;
use std::path::Path;

use fnv::FnvHashMap;
use uuid::Uuid;
use vec_map::VecMap;

use self::parser::{Error as ParserError, ErrorKind as ParserErrorKind, Parser, Token};

/// Platform name used by SDL mappings
#[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd"))]
const SDL_PLATFORM_NAME: &str = "Linux";
#[cfg(target_os = "macos")]
const SDL_PLATFORM_NAME: &str = "Mac OS X";
#[cfg(target_os = "windows")]
const SDL_PLATFORM_NAME: &str = "Windows";
#[cfg(all(
    not(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd")),
    not(target_os = "macos"),
    not(target_os = "windows")
))]
const SDL_PLATFORM_NAME: &str = "Unknown";

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
/// Store mappings from one `EvCode` (`u16`) to another.
///
/// This struct is internal, `MappingData` is exported in public interface as `Mapping`.
pub struct Mapping {
    mappings: FnvHashMap<EvCode, AxisOrBtn>,
    name: String,
    default: bool,
    hats_mapped: u8,
}

impl Mapping {
    pub fn new() -> Self {
        Mapping {
            mappings: FnvHashMap::default(),
            name: String::new(),
            default: false,
            hats_mapped: 0,
        }
    }

    pub fn default(gamepad: &gilrs_core::Gamepad) -> Self {
        use self::Axis as Ax;
        use self::AxisOrBtn::*;

        macro_rules! fnv_map {
            ( $( $key:expr => $elem:expr ),* ) => {
                {
                    let mut map = FnvHashMap::default();
                    $(
                        map.insert($key, $elem);
                    )*

                    map
                }
            };
        }

        let mut mappings = fnv_map![
            nec::BTN_SOUTH => Btn(Button::South),
            nec::BTN_EAST => Btn(Button::East),
            nec::BTN_C => Btn(Button::C),
            nec::BTN_NORTH => Btn(Button::North),
            nec::BTN_WEST => Btn(Button::West),
            nec::BTN_Z => Btn(Button::Z),
            nec::BTN_LT => Btn(Button::LeftTrigger),
            nec::BTN_RT => Btn(Button::RightTrigger),
            nec::BTN_LT2 => Btn(Button::LeftTrigger2),
            nec::BTN_RT2 => Btn(Button::RightTrigger2),
            nec::BTN_SELECT => Btn(Button::Select),
            nec::BTN_START => Btn(Button::Start),
            nec::BTN_MODE => Btn(Button::Mode),
            nec::BTN_LTHUMB => Btn(Button::LeftThumb),
            nec::BTN_RTHUMB => Btn(Button::RightThumb),
            nec::BTN_DPAD_UP => Btn(Button::DPadUp),
            nec::BTN_DPAD_DOWN => Btn(Button::DPadDown),
            nec::BTN_DPAD_LEFT => Btn(Button::DPadLeft),
            nec::BTN_DPAD_RIGHT => Btn(Button::DPadRight),

            nec::AXIS_LT => Btn(Button::LeftTrigger),
            nec::AXIS_RT => Btn(Button::RightTrigger),
            nec::AXIS_LT2 => Btn(Button::LeftTrigger2),
            nec::AXIS_RT2 => Btn(Button::RightTrigger2),

            nec::AXIS_LSTICKX => Axis(Ax::LeftStickX),
            nec::AXIS_LSTICKY => Axis(Ax::LeftStickY),
            nec::AXIS_LEFTZ => Axis(Ax::LeftZ),
            nec::AXIS_RSTICKX => Axis(Ax::RightStickX),
            nec::AXIS_RSTICKY => Axis(Ax::RightStickY),
            nec::AXIS_RIGHTZ => Axis(Ax::RightZ),
            nec::AXIS_DPADX => Axis(Ax::DPadX),
            nec::AXIS_DPADY => Axis(Ax::DPadY)
        ];

        // Remove all mappings that don't have corresponding element in gamepad. Partial fix to #83
        let axes = [
            nec::AXIS_DPADX,
            nec::AXIS_DPADY,
            nec::AXIS_LEFTZ,
            nec::AXIS_LSTICKX,
            nec::AXIS_LSTICKY,
            nec::AXIS_RSTICKX,
            nec::AXIS_RSTICKY,
            nec::AXIS_LT,
            nec::AXIS_LT2,
            nec::AXIS_RT,
            nec::AXIS_RT2,
            nec::AXIS_RIGHTZ,
        ];
        let btns = [
            nec::BTN_SOUTH,
            nec::BTN_NORTH,
            nec::BTN_WEST,
            nec::BTN_WEST,
            nec::BTN_C,
            nec::BTN_Z,
            nec::BTN_LT,
            nec::BTN_LT2,
            nec::BTN_RT,
            nec::BTN_RT2,
            nec::BTN_SELECT,
            nec::BTN_START,
            nec::BTN_MODE,
            nec::BTN_LTHUMB,
            nec::BTN_RTHUMB,
            nec::BTN_DPAD_DOWN,
            nec::BTN_DPAD_LEFT,
            nec::BTN_DPAD_RIGHT,
            nec::BTN_DPAD_UP,
        ];

        for axis in &axes {
            if !gamepad.axes().contains(axis) {
                mappings.remove(axis);
            }
        }

        for btn in &btns {
            if !gamepad.buttons().contains(btn) {
                mappings.remove(btn);
            }
        }

        Mapping {
            mappings,
            name: String::new(),
            default: true,
            hats_mapped: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn from_data(
        data: &MappingData,
        buttons: &[EvCode],
        axes: &[EvCode],
        name: &str,
        uuid: Uuid,
    ) -> Result<(Self, String), MappingError> {
        use crate::constants::*;

        if !Self::is_name_valid(name) {
            return Err(MappingError::InvalidName);
        }

        let mut mappings = FnvHashMap::default();
        let mut sdl_mappings = format!("{},{},", uuid.as_simple(), name);

        {
            let mut add_button = |ident, ev_code, mapped_btn| {
                Self::add_button(
                    ident,
                    ev_code,
                    mapped_btn,
                    buttons,
                    &mut sdl_mappings,
                    &mut mappings,
                )
            };

            for (button, &ev_code) in &data.buttons {
                match button as u16 {
                    BTN_SOUTH => add_button("a", ev_code, Button::South)?,
                    BTN_EAST => add_button("b", ev_code, Button::East)?,
                    BTN_WEST => add_button("x", ev_code, Button::West)?,
                    BTN_NORTH => add_button("y", ev_code, Button::North)?,
                    BTN_LT => add_button("leftshoulder", ev_code, Button::LeftTrigger)?,
                    BTN_RT => add_button("rightshoulder", ev_code, Button::RightTrigger)?,
                    BTN_LT2 => add_button("lefttrigger", ev_code, Button::LeftTrigger2)?,
                    BTN_RT2 => add_button("righttrigger", ev_code, Button::RightTrigger2)?,
                    BTN_SELECT => add_button("back", ev_code, Button::Select)?,
                    BTN_START => add_button("start", ev_code, Button::Start)?,
                    BTN_MODE => add_button("guide", ev_code, Button::Mode)?,
                    BTN_LTHUMB => add_button("leftstick", ev_code, Button::LeftThumb)?,
                    BTN_RTHUMB => add_button("rightstick", ev_code, Button::RightThumb)?,
                    BTN_DPAD_UP => add_button("dpup", ev_code, Button::DPadUp)?,
                    BTN_DPAD_DOWN => add_button("dpdown", ev_code, Button::DPadDown)?,
                    BTN_DPAD_LEFT => add_button("dpleft", ev_code, Button::DPadLeft)?,
                    BTN_DPAD_RIGHT => add_button("dpright", ev_code, Button::DPadRight)?,
                    BTN_C => add_button("c", ev_code, Button::C)?,
                    BTN_Z => add_button("z", ev_code, Button::Z)?,
                    BTN_UNKNOWN => return Err(MappingError::UnknownElement),
                    _ => unreachable!(),
                }
            }
        }

        {
            let mut add_axis = |ident, ev_code, mapped_axis| {
                Self::add_axis(
                    ident,
                    ev_code,
                    mapped_axis,
                    axes,
                    &mut sdl_mappings,
                    &mut mappings,
                )
            };

            for (axis, &ev_code) in &data.axes {
                match axis as u16 {
                    AXIS_LSTICKX => add_axis("leftx", ev_code, Axis::LeftStickX)?,
                    AXIS_LSTICKY => add_axis("lefty", ev_code, Axis::LeftStickY)?,
                    AXIS_RSTICKX => add_axis("rightx", ev_code, Axis::RightStickX)?,
                    AXIS_RSTICKY => add_axis("righty", ev_code, Axis::RightStickY)?,
                    AXIS_LEFTZ => add_axis("leftz", ev_code, Axis::LeftZ)?,
                    AXIS_RIGHTZ => add_axis("rightz", ev_code, Axis::RightZ)?,
                    AXIS_UNKNOWN => return Err(MappingError::UnknownElement),
                    _ => unreachable!(),
                }
            }
        }

        let mapping = Mapping {
            mappings,
            name: name.to_owned(),
            default: false,
            hats_mapped: 0,
        };

        Ok((mapping, sdl_mappings))
    }

    pub fn parse_sdl_mapping(
        line: &str,
        buttons: &[EvCode],
        axes: &[EvCode],
    ) -> Result<Self, ParseSdlMappingError> {
        let mut mapping = Mapping::new();
        let mut parser = Parser::new(line);

        let mut uuid: Option<Uuid> = None;
        while let Some(token) = parser.next_token() {
            if let Err(ref e) = token {
                if e.kind() == &ParserErrorKind::EmptyValue {
                    continue;
                }
            }

            let token = token?;

            match token {
                Token::Platform(platform) => {
                    if platform != SDL_PLATFORM_NAME {
                        warn!("Mappings for different platform – {}", platform);
                    }
                }
                Token::Uuid(v) => uuid = Some(v),

                Token::Name(name) => mapping.name = name.to_owned(),
                Token::AxisMapping { from, to, .. } => {
                    let axis = axes.get(from as usize).cloned();
                    if let Some(axis) = axis {
                        mapping.mappings.insert(axis, to);
                    } else {
                        warn!(
                            "SDL-mapping {} {}: Unknown axis a{}",
                            uuid.unwrap(),
                            mapping.name,
                            from
                        )
                    }
                }
                Token::ButtonMapping { from, to, .. } => {
                    let btn = buttons.get(from as usize).cloned();

                    if let Some(btn) = btn {
                        mapping.mappings.insert(btn, to);
                    } else {
                        warn!(
                            "SDL-mapping {} {}: Unknown button b{}",
                            uuid.unwrap(),
                            mapping.name,
                            from
                        )
                    }
                }
                Token::HatMapping {
                    hat, direction, to, ..
                } => {
                    if hat != 0 {
                        warn!(
                            "Hat mappings are only supported for dpads (requested to map hat \
                             {}.{} to {:?}",
                            hat, direction, to
                        );
                    } else {
                        // We  don't have anything like "hat" in gilrs, so let's jus assume that
                        // user want to map dpad axes.
                        //
                        // We have to add mappings for axes AND buttons, because axis_dpad_to_button
                        // filter may transform event to button event.
                        let (from_axis, from_btn) = match direction {
                            1 => (nec::AXIS_DPADY, nec::BTN_DPAD_UP),
                            4 => (nec::AXIS_DPADY, nec::BTN_DPAD_DOWN),
                            2 => (nec::AXIS_DPADX, nec::BTN_DPAD_RIGHT),
                            8 => (nec::AXIS_DPADX, nec::BTN_DPAD_LEFT),
                            0 => continue, // FIXME: I have no idea what 0 means here
                            _ => return Err(ParseSdlMappingError::UnknownHatDirection),
                        };

                        if to.is_button() {
                            match to {
                                AxisOrBtn::Btn(Button::DPadLeft | Button::DPadRight) => {
                                    mapping
                                        .mappings
                                        .insert(from_axis, AxisOrBtn::Axis(Axis::DPadX));
                                }
                                AxisOrBtn::Btn(Button::DPadUp | Button::DPadDown) => {
                                    mapping
                                        .mappings
                                        .insert(from_axis, AxisOrBtn::Axis(Axis::DPadY));
                                }
                                _ => (),
                            }
                            mapping.mappings.insert(from_btn, to);
                        } else {
                            mapping.mappings.insert(from_axis, to);
                        }

                        mapping.hats_mapped |= direction as u8;
                    }
                }
            }
        }

        Ok(mapping)
    }

    fn add_button(
        ident: &str,
        ev_code: EvCode,
        mapped_btn: Button,
        buttons: &[EvCode],
        sdl_mappings: &mut String,
        mappings: &mut FnvHashMap<EvCode, AxisOrBtn>,
    ) -> Result<(), MappingError> {
        let n_btn = buttons
            .iter()
            .position(|&x| x == ev_code)
            .ok_or(MappingError::InvalidCode(ev::Code(ev_code)))?;
        let _ = write!(sdl_mappings, "{}:b{},", ident, n_btn);
        mappings.insert(ev_code, AxisOrBtn::Btn(mapped_btn));
        Ok(())
    }

    fn add_axis(
        ident: &str,
        ev_code: EvCode,
        mapped_axis: Axis,
        axes: &[EvCode],
        sdl_mappings: &mut String,
        mappings: &mut FnvHashMap<EvCode, AxisOrBtn>,
    ) -> Result<(), MappingError> {
        let n_axis = axes
            .iter()
            .position(|&x| x == ev_code)
            .ok_or(MappingError::InvalidCode(ev::Code(ev_code)))?;
        let _ = write!(sdl_mappings, "{}:a{},", ident, n_axis);
        mappings.insert(ev_code, AxisOrBtn::Axis(mapped_axis));
        Ok(())
    }

    fn is_name_valid(name: &str) -> bool {
        !name.chars().any(|x| x == ',')
    }

    pub fn map(&self, code: &EvCode) -> Option<AxisOrBtn> {
        self.mappings.get(code).cloned()
    }

    pub fn map_rev(&self, el: &AxisOrBtn) -> Option<EvCode> {
        self.mappings.iter().find(|x| x.1 == el).map(|x| *x.0)
    }

    pub fn is_default(&self) -> bool {
        self.default
    }

    /// Return bit field with mapped hats. Only for mappings created from SDL format this function
    /// can return non-zero value.
    pub fn hats_mapped(&self) -> u8 {
        self.hats_mapped
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParseSdlMappingError {
    UnknownHatDirection,
    ParseError(ParserError),
}

impl From<ParserError> for ParseSdlMappingError {
    fn from(f: ParserError) -> Self {
        ParseSdlMappingError::ParseError(f)
    }
}

impl Error for ParseSdlMappingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        if let ParseSdlMappingError::ParseError(ref err) = self {
            Some(err)
        } else {
            None
        }
    }
}

impl Display for ParseSdlMappingError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
        match self {
            ParseSdlMappingError::UnknownHatDirection => {
                fmt.write_str("hat direction wasn't 1, 2, 4 or 8")
            }
            ParseSdlMappingError::ParseError(_) => fmt.write_str("parsing error"),
        }
    }
}

// NVForever patch (#4858): 3-tier SDL2/SDL3-compatible GUID lookup (exact -> CRC-masked ->
// firmware-version-masked for hardware VID/PID GUIDs) and user override files
// (`NVF_GAMECONTROLLERDB_FILE`, `SDL_GAMECONTROLLERCONFIG_FILE`, `SDL_GAMECONTROLLERCONFIG`).
const NVF_GAMECONTROLLERDB_FILE_ENV: &str = "NVF_GAMECONTROLLERDB_FILE";
const SDL_GAMECONTROLLERCONFIG_FILE_ENV: &str = "SDL_GAMECONTROLLERCONFIG_FILE";
const SDL_GAMECONTROLLERCONFIG_ENV: &str = "SDL_GAMECONTROLLERCONFIG";

/// Returns `true` when `bytes` encodes a standard hardware `(bus, vid, pid, version)` SDL GUID
/// (non-zero VID and PID, zero reserved fields, and zero SDL driver signature in bytes 14..16).
/// Rejects generic/zero-VID entries, ASCII name-based GUIDs, Android hash GUIDs, and SDL HIDAPI
/// driver-tagged GUIDs so fallback matching never crosses unrelated controllers or drivers.
fn is_hardware_vid_pid_guid(bytes: &[u8; 16]) -> bool {
    let vid = u16::from_le_bytes([bytes[4], bytes[5]]);
    let pid = u16::from_le_bytes([bytes[8], bytes[9]]);
    vid != 0
        && pid != 0
        && bytes[6] == 0
        && bytes[7] == 0
        && bytes[10] == 0
        && bytes[11] == 0
        && bytes[14] == 0
        && bytes[15] == 0
}

fn strip_crc(uuid: Uuid) -> Option<Uuid> {
    let mut bytes = *uuid.as_bytes();
    if !is_hardware_vid_pid_guid(&bytes) {
        return None;
    }
    bytes[2] = 0;
    bytes[3] = 0;
    Some(Uuid::from_bytes(bytes))
}

fn strip_crc_and_version(uuid: Uuid) -> Option<Uuid> {
    let mut bytes = *uuid.as_bytes();
    if !is_hardware_vid_pid_guid(&bytes) {
        return None;
    }
    bytes[2] = 0;
    bytes[3] = 0;
    bytes[12] = 0;
    bytes[13] = 0;
    Some(Uuid::from_bytes(bytes))
}

fn guid_version(uuid: Uuid) -> u16 {
    let bytes = uuid.as_bytes();
    u16::from_le_bytes([bytes[12], bytes[13]])
}

#[derive(Debug, Default)]
struct MappingLayer {
    exact: HashMap<Uuid, String>,
    by_crc_masked: HashMap<Uuid, String>,
    by_vid_pid: HashMap<Uuid, Vec<(u16, String)>>,
}

impl MappingLayer {
    fn insert_for_platform(&mut self, s: &str, platform: &str) {
        for raw_line in s.lines() {
            let mapping = raw_line.trim();
            if mapping.is_empty() || mapping.starts_with('#') {
                continue;
            }

            let pat = "platform:";
            if let Some(offset) = mapping.find(pat).map(|o| o + pat.len()) {
                let rest = &mapping[offset..];
                let end = rest.find(',').unwrap_or(rest.len());

                if &rest[..end] != platform {
                    continue;
                }
            }

            if let Some(uuid) = mapping
                .split(',')
                .next()
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                self.exact.insert(uuid, mapping.to_owned());
                if let Some(masked) = strip_crc(uuid) {
                    self.by_crc_masked.insert(masked, mapping.to_owned());
                }
                if let Some(bvp) = strip_crc_and_version(uuid) {
                    let ver = guid_version(uuid);
                    let entries = self.by_vid_pid.entry(bvp).or_default();
                    if let Some(existing) = entries.iter_mut().find(|(v, _)| *v == ver) {
                        existing.1 = mapping.to_owned();
                    } else {
                        entries.push((ver, mapping.to_owned()));
                    }
                }
            }
        }
    }

    fn get(&self, uuid: Uuid) -> Option<&str> {
        // Tier 1: exact 16-byte UUID match.
        if let Some(mapping) = self.exact.get(&uuid) {
            return Some(mapping.as_str());
        }

        // Tier 2: CRC-masked match (`bytes[2..4]` zeroed) for SDL 2.26+ CRC16 GUIDs.
        if let Some(masked) = strip_crc(uuid) {
            if let Some(mapping) = self.by_crc_masked.get(&masked) {
                return Some(mapping.as_str());
            }
        }

        // Tier 3: firmware-version-masked `(bus, vid, pid)` match (`bytes[2..4]` and
        // `bytes[12..14]` zeroed). Prefer a version-agnostic `0` entry; otherwise pick the
        // nearest firmware version (breaking ties toward the higher version, then string order
        // for determinism).
        if let Some(bvp) = strip_crc_and_version(uuid) {
            if let Some(candidates) = self.by_vid_pid.get(&bvp) {
                let query_ver = guid_version(uuid);
                return candidates
                    .iter()
                    .min_by(|(va, sa), (vb, sb)| {
                        (
                            *va != 0,
                            va.abs_diff(query_ver),
                            std::cmp::Reverse(*va),
                            sa.as_str(),
                        )
                            .cmp(&(
                                *vb != 0,
                                vb.abs_diff(query_ver),
                                std::cmp::Reverse(*vb),
                                sb.as_str(),
                            ))
                    })
                    .map(|(_, s)| s.as_str());
            }
        }

        None
    }

    fn len(&self) -> usize {
        self.exact.len()
    }
}

#[derive(Debug)]
pub struct MappingDb {
    included: MappingLayer,
    api: MappingLayer,
    nvf_file: MappingLayer,
    sdl_file: MappingLayer,
    sdl_inline: MappingLayer,
}

impl MappingDb {
    pub fn new() -> Self {
        MappingDb {
            included: MappingLayer::default(),
            api: MappingLayer::default(),
            nvf_file: MappingLayer::default(),
            sdl_file: MappingLayer::default(),
            sdl_inline: MappingLayer::default(),
        }
    }

    pub fn add_included_mappings(&mut self) {
        self.included.insert_for_platform(
            include_str!(concat!(env!("OUT_DIR"), "/gamecontrollerdb.txt")),
            SDL_PLATFORM_NAME,
        );
    }

    pub fn add_env_mappings(&mut self) {
        let nvf_file = env::var_os(NVF_GAMECONTROLLERDB_FILE_ENV).filter(|v| !v.is_empty());
        let sdl_file = env::var_os(SDL_GAMECONTROLLERCONFIG_FILE_ENV).filter(|v| !v.is_empty());
        let sdl_inline = env::var(SDL_GAMECONTROLLERCONFIG_ENV).ok();
        self.add_env_mappings_from(
            nvf_file.as_deref().map(Path::new),
            sdl_file.as_deref().map(Path::new),
            sdl_inline.as_deref(),
            SDL_PLATFORM_NAME,
        );
    }

    fn load_mapping_file(var_name: &str, path: &Path, layer: &mut MappingLayer, platform: &str) {
        match fs::read_to_string(path) {
            Ok(content) => layer.insert_for_platform(&content, platform),
            Err(err) => {
                warn!(
                    "Failed to read controller mappings from {var_name} ({}): {err}",
                    path.display()
                );
            }
        }
    }

    fn add_env_mappings_from(
        &mut self,
        nvf_file: Option<&Path>,
        sdl_file: Option<&Path>,
        sdl_inline: Option<&str>,
        platform: &str,
    ) {
        if let Some(path) = nvf_file {
            Self::load_mapping_file(
                NVF_GAMECONTROLLERDB_FILE_ENV,
                path,
                &mut self.nvf_file,
                platform,
            );
        }

        if let Some(path) = sdl_file {
            Self::load_mapping_file(
                SDL_GAMECONTROLLERCONFIG_FILE_ENV,
                path,
                &mut self.sdl_file,
                platform,
            );
        }

        if let Some(mapping) = sdl_inline {
            self.sdl_inline.insert_for_platform(mapping, platform);
        }
    }

    pub fn insert(&mut self, s: &str) {
        self.api.insert_for_platform(s, SDL_PLATFORM_NAME);
    }

    pub fn get(&self, uuid: Uuid) -> Option<&str> {
        self.sdl_inline
            .get(uuid)
            .or_else(|| self.sdl_file.get(uuid))
            .or_else(|| self.nvf_file.get(uuid))
            .or_else(|| self.api.get(uuid))
            .or_else(|| self.included.get(uuid))
    }

    pub fn len(&self) -> usize {
        self.included.len()
            + self.api.len()
            + self.nvf_file.len()
            + self.sdl_file.len()
            + self.sdl_inline.len()
    }
}

/// Stores data used to map gamepad buttons and axes.
///
/// After you add all mappings, use
/// [`Gamepad::set_mapping(…)`](struct.Gamepad.html#method.set_mapping) to change mapping of
/// existing gamepad.
///
/// See `examples/mapping.rs` for more detailed example.
#[derive(Debug, Clone, Default)]
// Re-exported as Mapping
pub struct MappingData {
    buttons: VecMap<EvCode>,
    axes: VecMap<EvCode>,
}

impl MappingData {
    /// Creates new `Mapping`.
    pub fn new() -> Self {
        MappingData {
            buttons: VecMap::with_capacity(18),
            axes: VecMap::with_capacity(11),
        }
    }

    /// Returns `EvCode` associated with button index.
    pub fn button(&self, idx: Button) -> Option<ev::Code> {
        self.buttons.get(idx as usize).cloned().map(ev::Code)
    }

    /// Returns `EvCode` associated with axis index.
    pub fn axis(&self, idx: Axis) -> Option<ev::Code> {
        self.axes.get(idx as usize).cloned().map(ev::Code)
    }

    /// Inserts new button mapping.
    pub fn insert_btn(&mut self, from: ev::Code, to: Button) -> Option<ev::Code> {
        self.buttons.insert(to as usize, from.0).map(ev::Code)
    }

    /// Inserts new axis mapping.
    pub fn insert_axis(&mut self, from: ev::Code, to: Axis) -> Option<ev::Code> {
        self.axes.insert(to as usize, from.0).map(ev::Code)
    }

    /// Removes button and returns associated `NativEvCode`.
    pub fn remove_button(&mut self, idx: Button) -> Option<ev::Code> {
        self.buttons.remove(idx as usize).map(ev::Code)
    }

    /// Removes axis and returns associated `NativEvCode`.
    pub fn remove_axis(&mut self, idx: Axis) -> Option<ev::Code> {
        self.axes.remove(idx as usize).map(ev::Code)
    }
}

/// The error type for functions related to gamepad mapping.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MappingError {
    /// Gamepad does not have element referenced by `EvCode`.
    InvalidCode(ev::Code),
    /// Name contains comma (',').
    InvalidName,
    /// This function is not implemented for current platform.
    NotImplemented,
    /// Gamepad is not connected.
    NotConnected,
    /// Same gamepad element is referenced by axis and button.
    DuplicatedEntry,
    /// `Mapping` with `Button::Unknown` or `Axis::Unknown`.
    UnknownElement,
    /// `Mapping` have button or axis that are not present in SDL2.
    NotSdl2Compatible,
}

impl Error for MappingError {}

impl Display for MappingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let sbuf;
        let s = match self {
            MappingError::InvalidCode(code) => {
                sbuf = format!("gamepad does not have element with {}", code);
                sbuf.as_ref()
            }
            MappingError::InvalidName => "name can not contain comma",
            MappingError::NotImplemented => {
                "current platform does not implement setting custom mappings"
            }
            MappingError::NotConnected => "gamepad is not connected",
            MappingError::DuplicatedEntry => {
                "same gamepad element is referenced by axis and button"
            }
            MappingError::UnknownElement => "Button::Unknown and Axis::Unknown are not allowed",
            MappingError::NotSdl2Compatible => "one of buttons or axes is not compatible with SDL2",
        };

        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ev::{Axis, Button};
    use gilrs_core::native_ev_codes as nec;
    use gilrs_core::EvCode;
    use uuid::Uuid;
    // Do not include platform, mapping from (with UUID modified)
    // https://github.com/gabomdq/SDL_GameControllerDB/blob/master/gamecontrollerdb.txt
    const TEST_STR: &str = "03000000260900008888000000010001,GameCube {WiseGroup USB \
                            box},a:b0,b:b2,y:b3,x:b1,start:b7,rightshoulder:b6,dpup:h0.1,dpleft:\
                            h0.8,dpdown:h0.4,dpright:h0.2,leftx:a0,lefty:a1,rightx:a2,righty:a3,\
                            lefttrigger:a4,righttrigger:a5,";

    const BUTTONS: [EvCode; 15] = [
        nec::BTN_SOUTH,
        nec::BTN_EAST,
        nec::BTN_C,
        nec::BTN_NORTH,
        nec::BTN_WEST,
        nec::BTN_Z,
        nec::BTN_LT,
        nec::BTN_RT,
        nec::BTN_LT2,
        nec::BTN_RT2,
        nec::BTN_SELECT,
        nec::BTN_START,
        nec::BTN_MODE,
        nec::BTN_LTHUMB,
        nec::BTN_RTHUMB,
    ];

    const AXES: [EvCode; 12] = [
        nec::AXIS_LSTICKX,
        nec::AXIS_LSTICKY,
        nec::AXIS_LEFTZ,
        nec::AXIS_RSTICKX,
        nec::AXIS_RSTICKY,
        nec::AXIS_RIGHTZ,
        nec::AXIS_DPADX,
        nec::AXIS_DPADY,
        nec::AXIS_RT,
        nec::AXIS_LT,
        nec::AXIS_RT2,
        nec::AXIS_LT2,
    ];

    #[test]
    fn mapping() {
        Mapping::parse_sdl_mapping(TEST_STR, &BUTTONS, &AXES).unwrap();
    }

    #[test]
    fn from_data() {
        let uuid = Uuid::nil();
        let name = "Best Gamepad";
        let buttons = BUTTONS.iter().cloned().map(ev::Code).collect::<Vec<_>>();
        let axes = AXES.iter().cloned().map(ev::Code).collect::<Vec<_>>();

        let mut data = MappingData::new();
        data.insert_axis(axes[0], Axis::LeftStickX);
        data.insert_axis(axes[1], Axis::LeftStickY);
        data.insert_axis(axes[2], Axis::LeftZ);
        data.insert_axis(axes[3], Axis::RightStickX);
        data.insert_axis(axes[4], Axis::RightStickY);
        data.insert_axis(axes[5], Axis::RightZ);

        data.insert_btn(buttons[0], Button::South);
        data.insert_btn(buttons[1], Button::East);
        data.insert_btn(buttons[3], Button::North);
        data.insert_btn(buttons[4], Button::West);
        data.insert_btn(buttons[5], Button::Select);
        data.insert_btn(buttons[6], Button::Start);
        data.insert_btn(buttons[7], Button::DPadDown);
        data.insert_btn(buttons[8], Button::DPadLeft);
        data.insert_btn(buttons[9], Button::RightThumb);

        let (mappings, sdl_mappings) =
            Mapping::from_data(&data, &BUTTONS, &AXES, name, uuid).unwrap();
        let sdl_mappings = Mapping::parse_sdl_mapping(&sdl_mappings, &BUTTONS, &AXES).unwrap();
        assert_eq!(mappings, sdl_mappings);

        let incorrect_mappings = Mapping::from_data(&data, &BUTTONS, &AXES, "Inval,id name", uuid);
        assert_eq!(Err(MappingError::InvalidName), incorrect_mappings);

        data.insert_btn(ev::Code(nec::BTN_DPAD_RIGHT), Button::DPadRight);
        let incorrect_mappings = Mapping::from_data(&data, &BUTTONS, &AXES, name, uuid);
        assert_eq!(
            Err(MappingError::InvalidCode(ev::Code(nec::BTN_DPAD_RIGHT))),
            incorrect_mappings
        );

        data.insert_btn(ev::Code(BUTTONS[3]), Button::Unknown);
        let incorrect_mappings = Mapping::from_data(&data, &BUTTONS, &AXES, name, uuid);
        assert_eq!(Err(MappingError::UnknownElement), incorrect_mappings);
    }

    #[test]
    fn with_mappings() {
        let mappings = format!(
            "\nShould be ignored\nThis also should,be ignored\n\n{}",
            TEST_STR
        );
        let mut db = MappingDb::new();
        db.add_included_mappings();
        db.insert(&mappings);

        assert_eq!(
            Some(TEST_STR),
            db.get(Uuid::parse_str("03000000260900008888000000010001").unwrap())
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn gamesir_hid_3537_100e_windows_mapping() {
        let mut db = MappingDb::new();
        db.add_included_mappings();

        for uuid_str in [
            "03000000-3735-0000-0e10-000000000000",
            "05000000-3735-0000-0e10-000000000000",
        ] {
            let uuid = Uuid::parse_str(uuid_str).unwrap();
            let line = db
                .get(uuid)
                .unwrap_or_else(|| panic!("expected bundled Windows mapping for {uuid_str}"));
            let parsed = Mapping::parse_sdl_mapping(line, &BUTTONS, &AXES).unwrap();
            assert_eq!(parsed.name(), "GameSir Controller");
            assert_eq!(parsed.map(&BUTTONS[0]), Some(AxisOrBtn::Btn(Button::South)));
            assert_eq!(parsed.map(&BUTTONS[1]), Some(AxisOrBtn::Btn(Button::East)));
            assert_eq!(parsed.map(&BUTTONS[3]), Some(AxisOrBtn::Btn(Button::West)));
            assert_eq!(parsed.map(&BUTTONS[4]), Some(AxisOrBtn::Btn(Button::North)));
            assert_eq!(
                parsed.map(&BUTTONS[6]),
                Some(AxisOrBtn::Btn(Button::LeftTrigger))
            );
            assert_eq!(
                parsed.map(&BUTTONS[7]),
                Some(AxisOrBtn::Btn(Button::RightTrigger))
            );
            assert_eq!(
                parsed.map(&BUTTONS[8]),
                Some(AxisOrBtn::Btn(Button::LeftTrigger2))
            );
            assert_eq!(
                parsed.map(&BUTTONS[9]),
                Some(AxisOrBtn::Btn(Button::RightTrigger2))
            );
            assert_eq!(
                parsed.map(&BUTTONS[10]),
                Some(AxisOrBtn::Btn(Button::Select))
            );
            assert_eq!(
                parsed.map(&BUTTONS[11]),
                Some(AxisOrBtn::Btn(Button::Start))
            );
            assert_eq!(parsed.map(&BUTTONS[12]), Some(AxisOrBtn::Btn(Button::Mode)));
            assert_eq!(
                parsed.map(&BUTTONS[13]),
                Some(AxisOrBtn::Btn(Button::LeftThumb))
            );
            assert_eq!(
                parsed.map(&BUTTONS[14]),
                Some(AxisOrBtn::Btn(Button::RightThumb))
            );
            assert_eq!(
                parsed.map(&AXES[0]),
                Some(AxisOrBtn::Axis(Axis::LeftStickX))
            );
            assert_eq!(
                parsed.map(&AXES[1]),
                Some(AxisOrBtn::Axis(Axis::LeftStickY))
            );
            assert_eq!(
                parsed.map(&AXES[3]),
                Some(AxisOrBtn::Axis(Axis::RightStickX))
            );
            assert_eq!(
                parsed.map(&AXES[4]),
                Some(AxisOrBtn::Axis(Axis::RightStickY))
            );
            assert_eq!(parsed.map(&AXES[2]), None);
            assert_eq!(parsed.map(&AXES[5]), None);
        }
    }

    const XPAD_BUTTONS: [EvCode; 11] = [
        nec::BTN_SOUTH,
        nec::BTN_EAST,
        nec::BTN_NORTH,
        nec::BTN_WEST,
        nec::BTN_LT,
        nec::BTN_RT,
        nec::BTN_SELECT,
        nec::BTN_START,
        nec::BTN_MODE,
        nec::BTN_LTHUMB,
        nec::BTN_RTHUMB,
    ];

    fn assert_nacon_gc100_mapping(line: &str) {
        let parsed = Mapping::parse_sdl_mapping(line, &XPAD_BUTTONS, &AXES).unwrap();
        assert_eq!(parsed.name(), "Nacon GC100");
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[0]),
            Some(AxisOrBtn::Btn(Button::South))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[1]),
            Some(AxisOrBtn::Btn(Button::East))
        );
        // Linux xpad reports physical Xbox X on b2 (BTN_NORTH/BTN_X) and Y on b3 (BTN_WEST/BTN_Y).
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[2]),
            Some(AxisOrBtn::Btn(Button::West))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[3]),
            Some(AxisOrBtn::Btn(Button::North))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[4]),
            Some(AxisOrBtn::Btn(Button::LeftTrigger))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[5]),
            Some(AxisOrBtn::Btn(Button::RightTrigger))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[6]),
            Some(AxisOrBtn::Btn(Button::Select))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[7]),
            Some(AxisOrBtn::Btn(Button::Start))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[8]),
            Some(AxisOrBtn::Btn(Button::Mode))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[9]),
            Some(AxisOrBtn::Btn(Button::LeftThumb))
        );
        assert_eq!(
            parsed.map(&XPAD_BUTTONS[10]),
            Some(AxisOrBtn::Btn(Button::RightThumb))
        );
        assert_eq!(
            parsed.map(&AXES[0]),
            Some(AxisOrBtn::Axis(Axis::LeftStickX))
        );
        assert_eq!(
            parsed.map(&AXES[1]),
            Some(AxisOrBtn::Axis(Axis::LeftStickY))
        );
        assert_eq!(
            parsed.map(&AXES[2]),
            Some(AxisOrBtn::Btn(Button::LeftTrigger2))
        );
        assert_eq!(
            parsed.map(&AXES[3]),
            Some(AxisOrBtn::Axis(Axis::RightStickX))
        );
        assert_eq!(
            parsed.map(&AXES[4]),
            Some(AxisOrBtn::Axis(Axis::RightStickY))
        );
        assert_eq!(
            parsed.map(&AXES[5]),
            Some(AxisOrBtn::Btn(Button::RightTrigger2))
        );
        assert_eq!(parsed.hats_mapped(), 0x0f);
    }

    #[test]
    fn nacon_gc100xf_0110_resolves_via_version_fallback_on_linux() {
        let mut db = MappingDb::new();
        db.included.insert_for_platform(
            include_str!("../../SDL_GameControllerDB/gamecontrollerdb.txt"),
            "Linux",
        );

        let uuid_0110 = Uuid::parse_str("03000000-8532-0000-0706-000010010000").unwrap();
        let line = db
            .get(uuid_0110)
            .expect("expected Nacon GC-100XF 0x0110 to resolve via 0x0112 Linux sibling");
        assert!(
            line.starts_with("03000000853200000706000012010000,Nacon GC100,"),
            "unexpected mapping line: {line}"
        );
        assert_nacon_gc100_mapping(line);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bundled_db_maps_nacon_gc100xf_0110_on_linux() {
        let mut db = MappingDb::new();
        db.add_included_mappings();

        let uuid_0110 = Uuid::parse_str("03000000-8532-0000-0706-000010010000").unwrap();
        let line = db
            .get(uuid_0110)
            .expect("expected bundled Linux DB to map Nacon GC-100XF 0x0110");
        assert_nacon_gc100_mapping(line);
    }

    #[test]
    fn sdl_crc_tagged_bundled_entry_matches_zero_crc_uuid() {
        let mut db = MappingDb::new();
        db.included.insert_for_platform(
            include_str!("../../SDL_GameControllerDB/gamecontrollerdb.txt"),
            "Mac OS X",
        );

        // Bundled entry is CRC-tagged `0300004b4c0500005f0e000000010000` (PS5 Access Controller).
        let zero_crc = Uuid::parse_str("03000000-4c05-0000-5f0e-000000010000").unwrap();
        let line = db
            .get(zero_crc)
            .expect("expected CRC-masked match for PS5 Access Controller");
        assert!(
            line.starts_with("0300004b4c0500005f0e000000010000,PS5 Access Controller,"),
            "unexpected line: {line}"
        );
    }

    #[test]
    fn exact_crc_and_version_tier_precedence() {
        let mut db = MappingDb::new();
        let entries = "\
03000000341200007856000010010000,Ver0110,a:b0,platform:Linux,
0300abcd341200007856000012010000,CrcVer0112,a:b1,platform:Linux,
03000000341200007856000014010000,Ver0114,a:b2,platform:Linux,
";
        db.included.insert_for_platform(entries, "Linux");

        // Tier 1: exact match wins.
        let exact = Uuid::parse_str("03000000-3412-0000-7856-000010010000").unwrap();
        assert!(db.get(exact).unwrap().contains(",Ver0110,"));

        // Tier 2: CRC-masked match wins over version fallback.
        let crc_zero = Uuid::parse_str("03000000-3412-0000-7856-000012010000").unwrap();
        assert!(db.get(crc_zero).unwrap().contains(",CrcVer0112,"));

        // Tier 3: equidistant between 0x0110 and 0x0112 -> tie breaks to higher version (0x0112).
        let mid = Uuid::parse_str("03000000-3412-0000-7856-000011010000").unwrap();
        assert!(db.get(mid).unwrap().contains(",CrcVer0112,"));

        // Tier 3: nearest version (0x0115 is closest to 0x0114).
        let newer = Uuid::parse_str("03000000-3412-0000-7856-000015010000").unwrap();
        assert!(db.get(newer).unwrap().contains(",Ver0114,"));

        // Tier 3: wildcard version 0 takes precedence over non-zero siblings.
        db.included.insert_for_platform(
            "03000000341200007856000000000000,WildcardVer0,a:b3,platform:Linux,",
            "Linux",
        );
        assert!(db.get(newer).unwrap().contains(",WildcardVer0,"));
        // Exact and CRC-masked matches still beat wildcard version 0.
        assert!(db.get(exact).unwrap().contains(",Ver0110,"));
        assert!(db.get(crc_zero).unwrap().contains(",CrcVer0112,"));
    }

    #[test]
    fn crc_and_version_guards_reject_generic_name_and_driver_sig_guids() {
        let mut db = MappingDb::new();
        let entries = "\
0300abcd000000007856000010010000,ZeroVid,a:b0,platform:Linux,
0300abcd341200000000000010010000,ZeroPid,a:b0,platform:Linux,
0500abcd4d4f435554452d3035335800,NameGuid,a:b0,platform:Linux,
030000007e0500000920000000026803,HidapiTagged,a:b0,platform:Linux,
05000000853200000706000012010000,WrongBusBt,a:b0,platform:Linux,
";
        db.included.insert_for_platform(entries, "Linux");

        // Zero VID or PID never matches via CRC or version fallback.
        assert_eq!(
            db.get(Uuid::parse_str("03000000-0000-0000-7856-000010010000").unwrap()),
            None
        );
        assert_eq!(
            db.get(Uuid::parse_str("03000000-3412-0000-0000-000010010000").unwrap()),
            None
        );
        // ASCII name-based GUID (non-zero bytes 6..8 / 10..12) never matches via CRC strip.
        assert_eq!(
            db.get(Uuid::parse_str("05000000-4d4f-4355-5445-2d3035335800").unwrap()),
            None
        );
        // HIDAPI driver-signature GUID (bytes 14..16 = 0x6803) never matches standard evdev (0000).
        assert_eq!(
            db.get(Uuid::parse_str("03000000-7e05-0000-0920-000000020000").unwrap()),
            None
        );
        // Bluetooth bus (0x0005) never matches USB bus (0x0003).
        assert_eq!(
            db.get(Uuid::parse_str("03000000-8532-0000-0706-000010010000").unwrap()),
            None
        );
        // Exact match on non-hardware GUIDs still works in Tier 1.
        assert!(db
            .get(Uuid::parse_str("03000000-7e05-0000-0920-000000026803").unwrap())
            .unwrap()
            .contains(",HidapiTagged,"));
    }

    #[test]
    fn env_file_and_config_override_precedence() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "gilrs-env-mappings-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let nvf_file_crc = dir.join("nvf-crc.txt");
        let nvf_file_exact = dir.join("nvf-exact.txt");
        let sdl_file_crc = dir.join("sdl-crc.txt");
        let sdl_file_exact = dir.join("sdl-exact.txt");
        let missing_file = dir.join("does-not-exist.txt");

        // CRLF + comments + omitted trailing comma on platform field.
        fs::write(
            &nvf_file_crc,
            "# user comment\r\n0300abcd853200000706000010010000,FromNvfFileCrc,a:b0,platform:Linux\r\n",
        )
        .unwrap();
        fs::write(
            &nvf_file_exact,
            "03000000853200000706000010010000,FromNvfFileExact,a:b0,platform:Linux,\n",
        )
        .unwrap();
        fs::write(
            &sdl_file_crc,
            "0300beef853200000706000010010000,FromSdlFileCrc,a:b1,platform:Linux,\n",
        )
        .unwrap();
        fs::write(
            &sdl_file_exact,
            "03000000853200000706000010010000,FromSdlFileExact,a:b1,platform:Linux,\n",
        )
        .unwrap();

        let query = Uuid::parse_str("03000000-8532-0000-0706-000010010000").unwrap();
        let bundled_exact =
            "03000000853200000706000010010000,BundledExact,a:b9,platform:Linux,";
        let api_exact = "03000000853200000706000010010000,ApiExact,a:b8,platform:Linux,";

        // 1. API mapping (`insert`) overrides bundled exact; NVF_GAMECONTROLLERDB_FILE
        //    (even CRC-tagged with CRLF) overrides both bundled exact and API exact.
        let mut db = MappingDb::new();
        db.included.insert_for_platform(bundled_exact, "Linux");
        db.api.insert_for_platform(api_exact, "Linux");
        assert!(db.get(query).unwrap().contains(",ApiExact,"));
        db.add_env_mappings_from(Some(&nvf_file_crc), Some(&missing_file), None, "Linux");
        assert!(db.get(query).unwrap().contains(",FromNvfFileCrc,"));

        // 2. CRC-tagged SDL_GAMECONTROLLERCONFIG_FILE overrides zero-CRC exact in
        //    NVF_GAMECONTROLLERDB_FILE.
        let mut db = MappingDb::new();
        db.included.insert_for_platform(bundled_exact, "Linux");
        db.add_env_mappings_from(Some(&nvf_file_exact), Some(&sdl_file_crc), None, "Linux");
        assert!(db.get(query).unwrap().contains(",FromSdlFileCrc,"));

        // 3. CRC-tagged SDL_GAMECONTROLLERCONFIG overrides zero-CRC exact in both
        //    SDL_GAMECONTROLLERCONFIG_FILE and NVF_GAMECONTROLLERDB_FILE.
        let mut db = MappingDb::new();
        db.included.insert_for_platform(bundled_exact, "Linux");
        db.add_env_mappings_from(
            Some(&nvf_file_exact),
            Some(&sdl_file_exact),
            Some("0300cafe853200000706000010010000,FromInlineCrc,a:b2,platform:Linux,"),
            "Linux",
        );
        assert!(db.get(query).unwrap().contains(",FromInlineCrc,"));

        // 4. Version-wildcard (0x0000) SDL_GAMECONTROLLERCONFIG also overrides zero-CRC exact in
        //    both SDL_GAMECONTROLLERCONFIG_FILE and NVF_GAMECONTROLLERDB_FILE.
        let mut db = MappingDb::new();
        db.included.insert_for_platform(bundled_exact, "Linux");
        db.add_env_mappings_from(
            Some(&nvf_file_exact),
            Some(&sdl_file_exact),
            Some("03000000853200000706000000000000,FromInlineWildcard,a:b3,platform:Linux,"),
            "Linux",
        );
        assert!(db.get(query).unwrap().contains(",FromInlineWildcard,"));

        let _ = fs::remove_dir_all(&dir);
    }
}
