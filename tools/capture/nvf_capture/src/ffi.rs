//! The parts of xNVSE's plugin interface this plugin uses, re-expressed from the
//! description in docs/spec-notes/xnvse-capture.md §2 (no xNVSE code is copied).
//! 32-bit MSVC: `extern "C"` is cdecl, pointers are 4 bytes, `bool` is one byte.

#![allow(dead_code)]

use std::ffi::{c_char, c_void};

pub const INTERFACE_CONSOLE: u32 = 1;
pub const INTERFACE_MESSAGING: u32 = 2;
pub const INTERFACE_SCRIPT: u32 = 6;
pub const INTERFACE_EVENT_MANAGER: u32 = 8;

pub const MSG_POST_LOAD_GAME: u32 = 8;
pub const MSG_MAIN_GAME_LOOP: u32 = 20;

pub const ELEMENT_INVALID: u8 = 0;
pub const ELEMENT_NUMBER: u8 = 1;
pub const ELEMENT_FORM: u8 = 2;
pub const ELEMENT_STRING: u8 = 3;
pub const ELEMENT_ARRAY: u8 = 4;

/// `major << 24 | minor << 16 | build << 4 | sub`.
pub const fn packed_version(major: u32, minor: u32, build: u32, sub: u32) -> u32 {
    ((major & 0xFF) << 24) | ((minor & 0xFF) << 16) | ((build & 0xFFF) << 4) | (sub & 0xF)
}

/// Lowest xNVSE accepted: the release whose interface the notes describe.
pub const MIN_NVSE: u32 = packed_version(6, 4, 9, 0);
pub const RUNTIME_1_4_0_525: u32 = packed_version(4, 0, 525, 0);
pub const RUNTIME_1_4_0_525_NOGORE: u32 = packed_version(4, 0, 525, 1);

#[repr(C)]
pub struct NvseInterface {
    pub nvse_version: u32,
    pub runtime_version: u32,
    pub editor_version: u32,
    pub is_editor: u32,
    pub register_command: *const c_void,
    pub set_opcode_base: *const c_void,
    pub query_interface: unsafe extern "C" fn(id: u32) -> *mut c_void,
    pub get_plugin_handle: unsafe extern "C" fn() -> u32,
    pub register_typed_command: *const c_void,
    pub get_runtime_directory: unsafe extern "C" fn() -> *const c_char,
}

#[repr(C)]
pub struct PluginInfo {
    pub info_version: u32,
    pub name: *const c_char,
    pub version: u32,
}

#[repr(C)]
pub struct Message {
    pub sender: *const c_char,
    pub kind: u32,
    pub data_len: u32,
    pub data: *mut c_void,
}

pub type EventCallback = unsafe extern "C" fn(msg: *mut Message);

#[repr(C)]
pub struct MessagingInterface {
    pub version: u32,
    pub register_listener: unsafe extern "C" fn(handle: u32, sender: *const c_char, cb: EventCallback) -> bool,
    pub dispatch: *const c_void,
}

#[repr(C)]
pub struct ConsoleInterface {
    pub version: u32,
    pub run_script_line: unsafe extern "C" fn(line: *const c_char, refr: *mut c_void) -> bool,
    /// Present from console interface version 3.
    pub run_script_line2: unsafe extern "C" fn(line: *const c_char, refr: *mut c_void, suppress: bool) -> bool,
}

#[repr(C)]
pub struct ScriptInterface {
    pub call_function: unsafe extern "C" fn(
        script: *mut c_void,
        calling: *mut c_void,
        container: *mut c_void,
        result: *mut Element,
        num_args: u8,
        ...
    ) -> bool,
    pub get_function_params: *const c_void,
    pub extract_args_ex: *const c_void,
    pub extract_format_string_args: *const c_void,
    pub call_function_alt: *const c_void,
    pub compile_script: unsafe extern "C" fn(text: *const c_char) -> *mut c_void,
    pub compile_expression: unsafe extern "C" fn(text: *const c_char) -> *mut c_void,
}

/// A plugin's handler for a game event (plugin version 9): xNVSE calls it with a null
/// reference and a pointer to an array of the event's two arguments (first, second), each
/// a form pointer or null [documented: xNVSE 6.4.9 EventManager.cpp, the native branch of
/// the handler call].
pub type NativeEventHandler = unsafe extern "C" fn(this: *mut c_void, params: *mut c_void);

/// The event manager interface (id 8), first members only [documented: xNVSE 6.4.9
/// PluginAPI.h, `NVSEEventManagerInterface`: function pointers from its start, no data
/// member before them].
#[repr(C)]
pub struct EventManagerInterface {
    pub register_event: *const c_void,
    pub dispatch_event: *const c_void,
    pub dispatch_event_alt: *const c_void,
    pub set_native_event_handler: unsafe extern "C" fn(name: *const c_char, f: NativeEventHandler) -> bool,
    pub remove_native_event_handler: unsafe extern "C" fn(name: *const c_char, f: NativeEventHandler) -> bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union ElementData {
    pub text: *const c_char,
    pub num: f64,
    /// A form element's form, an array element's array (passed back unchanged).
    pub handle: *mut c_void,
}

#[repr(C)]
pub struct Element {
    pub data: ElementData,
    pub kind: u8,
}

impl Element {
    pub fn empty() -> Element {
        Element { data: ElementData { num: 0.0 }, kind: ELEMENT_INVALID }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn versions() {
        assert_eq!(MIN_NVSE, 0x0604_0090);
        assert_eq!(RUNTIME_1_4_0_525, 0x0400_20D0);
        assert_eq!(RUNTIME_1_4_0_525_NOGORE, 0x0400_20D1);
    }

    /// Layouts as documented for 32-bit MSVC (the tests run on the i686 target).
    #[cfg(target_pointer_width = "32")]
    #[test]
    fn layouts_32_bit() {
        assert_eq!(size_of::<Element>(), 16);
        assert_eq!(align_of::<Element>(), 8);
        assert_eq!(offset_of!(Element, kind), 8);
        assert_eq!(size_of::<PluginInfo>(), 12);
        assert_eq!(size_of::<Message>(), 16);
        assert_eq!(offset_of!(NvseInterface, query_interface), 24);
        assert_eq!(offset_of!(NvseInterface, get_plugin_handle), 28);
        assert_eq!(offset_of!(NvseInterface, get_runtime_directory), 36);
        assert_eq!(offset_of!(ConsoleInterface, run_script_line2), 8);
        assert_eq!(offset_of!(ScriptInterface, compile_script), 20);
        assert_eq!(offset_of!(EventManagerInterface, set_native_event_handler), 12);
        assert_eq!(offset_of!(EventManagerInterface, remove_native_event_handler), 16);
    }
}
