//! The checks before Streamline may make the Vulkan instance and device (#2198, the plan's
//! step 3): nothing here loads Streamline, and nothing that fails here reaches
//! `RawVulkanInitSettings::set_vulkan_library`.
//!
//! - **The updater** [design]: Streamline starts an `nvngx_update.exe` lying beside the
//!   interposer in place of the driver's own, without checking its signature, and does so even
//!   with both over-the-air flags cleared [documented: `src/ota.cpp:194-230, 232-246`;
//!   `src/pluginManager.cpp:961-969`; `docs/spec-notes/streamline.md`, "NVIDIA's updater"]. So the
//!   DLSS folder is listed before the interposer is loaded and again just before `slInit`; an
//!   entry of that name in any case refuses Streamline, and so does a folder that cannot be
//!   listed (fail closed). [`check_folder`].
//! - **Every DLL in the folder** [design: from the second review of #2206]: Streamline loads
//!   every `sl.*.dll` in its plugin folder before it asks whether the host wants that feature
//!   [documented: `src/pluginManager.cpp:541-560` lists them, `:722-745` loads each, then checks
//!   whether it was requested], and checks each with the same signature helper whose limit is
//!   `security.rs`'s open point (`src/secureLoadLibrary.cpp:31-41`, `include/sl_security.h:442`).
//!   And since `win.rs` adds the folder to the process's DLL search, any DLL there could stand in
//!   for one loaded by name. So in the same pass every entry whose name ends in `.dll` (any case)
//!   must be either a file of Streamline 2.14.16's release `bin\x64` with that file's SHA-256
//!   ([`RELEASE_DLLS`], so a player who copied the whole folder still gets DLSS), or an
//!   `nvngx_*.dll`, NVIDIA's NGX runtimes, which NGX loads itself and which are not pinned by
//!   hash so that a newer DLSS can be used, but must pass Windows' Authenticode check with a
//!   signer whose certificate names NVIDIA Corporation (`security::check_ngx`, given by `win.rs`;
//!   [design: from the follow-up review of #2206]). Anything else, or a listed name with another
//!   hash, is refused. Each DLL the pass accepts is read through a handle that shares read
//!   access only, and the handles are kept ([`Pinned`]) until Streamline is shut down (`win.rs`),
//!   so a checked file cannot be changed, renamed or deleted before Streamline or NGX loads it,
//!   even when that happens later than `slInit` (`sl.common` delay-loads `NvLowLatencyVk.dll`
//!   during device creation [documented: Streamline's `premake.lua:485`, `src/wrapper.cpp:1004`
//!   -> `src/vulkan.cpp:1183`]); a file added to the folder after the second pass is not seen
//!   [design: the window is the few calls between that pass and `slInit`].
//! - **No links** [design: from the follow-up review of #2206]: the folder itself, and every
//!   entry the pass opens (each DLL; the updater's name is refused before it is opened), must
//!   not be a reparse point (`FILE_ATTRIBUTE_REPARSE_POINT`: symbolic links, junctions, mount
//!   points). Each is opened with `FILE_FLAG_OPEN_REPARSE_POINT`, so the handle is the entry
//!   itself rather than what a link points at, and the attribute is read through that handle
//!   [documented: Microsoft Learn, "CreateFileW function (fileapi.h)", dwFlagsAndAttributes, and
//!   "Symbolic Link Effects on File Systems Functions"]. The folder is held open too, without
//!   `FILE_SHARE_DELETE`, so it cannot be renamed or deleted while held (same page, dwShareMode).
//!   A link among the DLLs is `SignatureRejected`; the folder being one is `StreamlineRefused`
//!   (the reason "a file in the dlss folder is not from the tested release" would not fit). The
//!   folders above the DLSS folder are not checked: it is the player's own folder, beside the
//!   program they unpacked and outside the game folder.
//! - **The device** [design]: once the interposer makes the instance there is no way back when
//!   the device is made: Streamline's `vkCreateDevice` refuses a device that lacks an extension
//!   it adds [documented: `src/wrapper.cpp:816-829`], and wgpu-hal panics on that [documented:
//!   wgpu-hal 30.0.1 `vulkan/adapter.rs:3044-3050`]. So every GPU Vulkan exposes is checked
//!   first (Bevy can still fall back to any of them, [`candidates`]), from a [`Snapshot`] taken
//!   through the system's Vulkan loader (`probe`, a temporary instance destroyed after), against
//!   what DLSS asks for ([`Requirements`], from `slGetFeatureRequirements`) plus what
//!   Streamline's own `vkCreateDevice` adds whatever the plugins say ([`judge`]). The check is a
//!   pure function over the snapshot, so the tests need no GPU.
//!
//! What Streamline's `vkCreateDevice` does to the device the renderer asks for [documented:
//! `src/wrapper.cpp`]:
//! - always asks for `VK_KHR_timeline_semaphore` and `VK_KHR_push_descriptor` (`:291-295`) and the
//!   1.2 features `timelineSemaphore`, `descriptorIndexing`, `bufferDeviceAddress` (`:282-287`),
//!   plus each plugin's device extensions, 1.2 and 1.3 features and extra queues (`:309-412`);
//! - when the renderer chains individual 1.2 feature structures, as wgpu-hal does (its
//!   `PhysicalDeviceTimelineSemaphoreFeaturesKHR`, `PhysicalDeviceDescriptorIndexingFeaturesEXT`,
//!   ... [documented: wgpu-hal 30.0.1 `vulkan/adapter.rs:367-390`]), it also asks for the
//!   extension of each of `descriptorIndexing`, `timelineSemaphore` and `bufferDeviceAddress`
//!   the device supports (`:613-686`), and likewise `VK_KHR_synchronization2` for the 1.3
//!   `synchronization2` (`:698-735`): an extension promoted to core still has to be listed by
//!   name. Taken as always so here, the conservative reading;
//! - refuses the device when any extension it asks for is not listed (`:816-829`, before it drops
//!   `VK_EXT_buffer_device_address` in favour of the KHR one, `:836-848`), but enables only the
//!   features the device supports (`:767-789`; `sl_helpers_vk.h:124-216`) and skips extra queues
//!   a family cannot hold with a warning (`:902-930`).
//!
//! Features DLSS asks for are still required here (DLSS needs them even where the device
//! creation would not fail), and so is `privateData`: something in Streamline's or NGX's path
//! calls `vkCreatePrivateDataSlot` [observed: Khronos validation layer,
//! `VUID-vkCreatePrivateDataSlot-privateData-04564`, 2026-11-12; `docs/design/dlss-upscaling.md`,
//! "What the runs showed"], which `win.rs`
//! enables when the device offers it.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use ash::vk;

use crate::upscaler::Unavailable;

#[cfg(windows)]
use super::ffi::FEATURE_REQUIREMENT_VULKAN_SUPPORTED;
/// `sl::FeatureRequirementFlags::eVulkanSupported` [documented: `sl_core_types.h:630`], for the
/// tests off Windows (`ffi.rs` is Windows only).
#[cfg(not(windows))]
const FEATURE_REQUIREMENT_VULKAN_SUPPORTED: u32 = 1 << 2;

/// NVIDIA's updater's name, which Streamline looks for beside the interposer [documented:
/// `src/ota.cpp:206`].
pub(super) const UPDATER_NAME: &str = "nvngx_update.exe";

/// The DLLs of Streamline 2.14.16's release archive (`streamline-sdk-v2.14.16.zip`, `bin\x64`)
/// and their SHA-256 in hex [observed: the official release zip, 2026-11-12]. Its
/// `nvngx_dlss.dll` is left out on purpose: `nvngx_*.dll` are not pinned (the module comment).
pub(super) const RELEASE_DLLS: [(&str, &str); 12] = [
    ("NvLowLatencyVk.dll", "2a77dc3e1c724b7eea5755be0ae7423752e79a2459fae72181a9f00e3507e5d6"),
    ("sl.common.dll", "38eea3b4209afc455636ea372b5933d65fcc13becec6c84d101dcfc4d8648f75"),
    ("sl.deepdvc.dll", "2d493dccfefdc0580deb85afc65e70ae36adf77c6dd3c745c60623d9b8ac54f9"),
    ("sl.directsr.dll", "e3bbaf3e5eb906a63b3f00413c2d1c4c585ab3a2ee1946fcfa9ee2d804001c8e"),
    ("sl.dlss.dll", "18871eea87c90345da97d8109faa81ea7d5840e8d99b51b55b0e46cfa177f84e"),
    ("sl.dlss_d.dll", "8dff36fd063094eb1f659e8d4dcd52c7423859a9c4a3df3796084c48412d60ac"),
    ("sl.dlss_g.dll", "3ed9538df724993bea8a050195a6a78b9c8c8a566d5b05ade477aab3c390e90f"),
    ("sl.interposer.dll", "b1d7395aebd7b9722841d64dcf7433680020b6a8e421f851c16d5ba0c2182edf"),
    ("sl.nis.dll", "f9f3873c00e2737b22df1bf7605d4fdc7978dbf2955df4e152c5b52aada13b79"),
    ("sl.nvperf.dll", "dc5a820b97943c4d697a9ddec59b0dcde86ecadc093b401cf55980f7f89be43b"),
    ("sl.pcl.dll", "5698c38e722318681f215c9fe68e45b9bedd51378acc699d8eee26e961e57770"),
    ("sl.reflex.dll", "443577e869352436ff7ec8f4d9c90418d3e95368436ce219261e0f4f07a38122"),
];

/// The release that [`RELEASE_DLLS`] is.
pub(super) const RELEASE: &str = "Streamline 2.14.16";

/// 64 hex digits (either case) as 32 bytes; `None` for anything else. The one parser for the
/// listed hashes here and in `security.rs`.
pub(super) fn parse_sha256(hex: &str) -> Option<[u8; 32]> {
    let (pairs, rest) = hex.as_bytes().as_chunks::<2>();
    if pairs.len() != 32 || !rest.is_empty() {
        return None;
    }
    let mut out = [0u8; 32];
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        let high = char::from(*high).to_digit(16)?;
        let low = char::from(*low).to_digit(16)?;
        *byte = (high * 16 + low) as u8;
    }
    Some(out)
}

/// Why the folder refuses Streamline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FolderRefusal {
    /// An entry with the updater's name, as the folder spells it.
    Updater(String),
    /// The folder or one of its entries could not be listed.
    Unreadable(String),
    /// A DLL that is neither a file of the release nor an `nvngx_*.dll`.
    UnknownDll(String),
    /// A DLL with a release file's name and another SHA-256 (the name as the folder spells it,
    /// the hash in hex).
    WrongHash(String, String),
    /// A DLL that could not be opened (sharing read access only) or read to be hashed.
    DllUnreadable(String, String),
    /// A DLL that is a reparse point (a symbolic link, junction or mount point), as the folder
    /// spells it.
    Link(String),
    /// An `nvngx_*.dll` that did not pass the NVIDIA signature check (the name as the folder
    /// spells it, the check's words).
    NotNvidia(String, String),
    /// The folder itself is a reparse point (its path).
    FolderLink(String),
}

impl FolderRefusal {
    /// The reason the settings show for it.
    pub(super) fn reason(&self) -> Unavailable {
        match self {
            Self::Updater(_) | Self::Unreadable(_) => Unavailable::UpdaterInFolder,
            Self::UnknownDll(_) | Self::WrongHash(..) | Self::Link(_) | Self::NotNvidia(..) => Unavailable::SignatureRejected,
            Self::DllUnreadable(..) | Self::FolderLink(_) => Unavailable::StreamlineRefused,
        }
    }
}

/// Why one entry the pass opened did not pass, before its name is attached ([`check_entries`]).
#[derive(Debug)]
pub(super) enum Fault {
    /// It could not be opened (sharing read access only) or read.
    Unreadable(std::io::Error),
    /// It is a reparse point.
    Link,
    /// An `nvngx_*.dll` the NVIDIA signature check refused (its words).
    NotNvidia(String),
}

impl From<std::io::Error> for Fault {
    fn from(error: std::io::Error) -> Self {
        Self::Unreadable(error)
    }
}

impl Fault {
    /// The refusal for the entry `name` (as the folder spells it).
    fn refusal(self, name: String) -> FolderRefusal {
        match self {
            Self::Unreadable(error) => FolderRefusal::DllUnreadable(name, error.to_string()),
            Self::Link => FolderRefusal::Link(name),
            Self::NotNvidia(why) => FolderRefusal::NotNvidia(name, why),
        }
    }
}

impl fmt::Display for FolderRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Updater(name) => write!(f, "the folder holds {name}, which Streamline would start without checking it"),
            Self::Unreadable(why) => write!(f, "the folder could not be listed to check its files ({why})"),
            Self::UnknownDll(name) => write!(f, "the folder holds {name}, which is not a DLL of {RELEASE}'s release nor an nvngx_*.dll"),
            Self::WrongHash(name, sha256) => write!(f, "the folder's {name} has SHA-256 {sha256}, not that of {RELEASE}'s release"),
            Self::DllUnreadable(name, why) => write!(f, "the folder's {name} could not be opened and read for its check with writing kept out ({why})"),
            Self::Link(name) => write!(f, "the folder's {name} is a link (a reparse point: symbolic link, junction or mount point), not a file"),
            Self::NotNvidia(name, why) => write!(f, "the folder's {name} is not signed by NVIDIA Corporation ({why})"),
            Self::FolderLink(path) => write!(f, "the DLSS folder {path} is a link (a reparse point: symbolic link, junction or mount point), not a folder"),
        }
    }
}

/// What the folder pass accepted. It holds the folder and each DLL it accepted open, the DLLs
/// shared for reading only, until it is dropped: `win.rs` keeps it until Streamline is shut down.
#[derive(Debug, Default)]
pub(super) struct Pinned {
    /// The release DLLs whose hash matched, as the folder spells them.
    pub(super) pinned: Vec<String>,
    /// The `nvngx_*.dll` left to NGX, signed by NVIDIA, as the folder spells them.
    pub(super) ngx: Vec<String>,
    /// The handles the DLLs were checked through.
    _held: Vec<File>,
    /// The folder's own handle (no `FILE_SHARE_DELETE`: it cannot be renamed or deleted).
    _folder: Option<File>,
}

impl fmt::Display for Pinned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |names: &[String]| if names.is_empty() { "none".to_string() } else { names.join(", ") };
        write!(f, "{RELEASE}'s release files with their SHA-256: {}; NGX's own, signed by NVIDIA, not pinned by hash: {}", list(&self.pinned), list(&self.ngx))
    }
}

/// `name` in full Unicode upper case, the comparison the module's names use (Windows' file names
/// compare without case [documented: Microsoft, "Naming Files, Paths, and Namespaces"]).
fn upper(name: &OsStr) -> String {
    name.to_string_lossy().chars().flat_map(char::to_uppercase).collect()
}

/// What a folder entry is, by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// NVIDIA's updater (refused).
    Updater,
    /// A file of the release, with its SHA-256 in hex.
    Release(&'static str),
    /// An `nvngx_*.dll` (accepted, not pinned).
    Ngx,
    /// Any other DLL (refused).
    UnknownDll,
    /// Not a DLL (left alone).
    Other,
}

fn kind(name: &OsStr) -> Kind {
    let upper = upper(name);
    if upper == UPDATER_NAME.to_ascii_uppercase() {
        return Kind::Updater;
    }
    if let Some((_, sha256)) = RELEASE_DLLS.iter().find(|(file, _)| file.to_ascii_uppercase() == upper) {
        return Kind::Release(sha256);
    }
    if !upper.ends_with(".DLL") {
        Kind::Other
    } else if upper.starts_with("NVNGX_") {
        Kind::Ngx
    } else {
        Kind::UnknownDll
    }
}

/// The folder's entries judged by name, each release DLL by its SHA-256, which `hash` reads, and
/// each `nvngx_*.dll` by `ngx` (both may keep a handle, and refuse a link). Refused at the first
/// entry that is the updater, an unknown DLL, a release DLL with another hash, an `nvngx_*.dll`
/// `ngx` refuses, a link, or that cannot be read. Names compare in full Unicode upper case (`ſ`
/// and `ı` upper-case to ASCII letters; a name that only looks close to the updater's is refused
/// too, and one that looks close to a release file's must have its hash, which fails closed).
pub(super) fn check_entries<I, H, N>(entries: I, mut hash: H, mut ngx: N) -> Result<Pinned, FolderRefusal>
where
    I: IntoIterator<Item = std::io::Result<OsString>>,
    H: FnMut(&OsStr) -> Result<[u8; 32], Fault>,
    N: FnMut(&OsStr) -> Result<(), Fault>,
{
    let mut pinned = Pinned::default();
    for entry in entries {
        let name = entry.map_err(|e| FolderRefusal::Unreadable(e.to_string()))?;
        let shown = || name.to_string_lossy().into_owned();
        match kind(&name) {
            Kind::Updater => return Err(FolderRefusal::Updater(shown())),
            Kind::UnknownDll => return Err(FolderRefusal::UnknownDll(shown())),
            Kind::Ngx => {
                ngx(&name).map_err(|fault| fault.refusal(shown()))?;
                pinned.ngx.push(shown());
            }
            Kind::Other => {}
            Kind::Release(expected) => {
                let digest = hash(&name).map_err(|fault| fault.refusal(shown()))?;
                let sha256 = fnv_util::sha256::to_hex(&digest);
                if sha256 != expected {
                    return Err(FolderRefusal::WrongHash(shown(), sha256));
                }
                pinned.pinned.push(shown());
            }
        }
    }
    pinned.pinned.sort();
    pinned.ngx.sort();
    Ok(pinned)
}

/// The DLSS folder checked: not a link itself, no `nvngx_update.exe` in any case, as a file,
/// folder or link, and every DLL a release file with its hash or an `nvngx_*.dll` that
/// `verify_ngx` accepts (given its full path and the held handle; `security::check_ngx` on
/// Windows), none of them a link (the module comment). The folder and the DLLs stay open in the
/// [`Pinned`] returned.
pub(super) fn check_folder<V>(folder: &Path, mut verify_ngx: V) -> Result<Pinned, FolderRefusal>
where
    V: FnMut(&Path, &File) -> Result<(), String>,
{
    let dir = open_folder(folder).map_err(|e| FolderRefusal::Unreadable(e.to_string()))?;
    if is_link(folder, &dir).map_err(|e| FolderRefusal::Unreadable(e.to_string()))? {
        return Err(FolderRefusal::FolderLink(folder.display().to_string()));
    }
    let entries = std::fs::read_dir(folder).map_err(|e| FolderRefusal::Unreadable(e.to_string()))?;
    let held = RefCell::new(Vec::new());
    // The entry itself opened, not what a link points at, and refused when it is a link.
    let open = |name: &OsStr| -> Result<(PathBuf, File), Fault> {
        let path = folder.join(name);
        let file = open_shared_read(&path)?;
        if is_link(&path, &file)? {
            return Err(Fault::Link);
        }
        // Windows will not open a folder as a file; other systems will, so a folder with a
        // DLL's name is refused here as well [design: the same refusal everywhere].
        if !file.metadata()?.is_file() {
            return Err(Fault::Unreadable(std::io::Error::other("not a file")));
        }
        Ok((path, file))
    };
    let hash = |name: &OsStr| -> Result<[u8; 32], Fault> {
        let (_, mut file) = open(name)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        held.borrow_mut().push(file);
        Ok(fnv_util::sha256::digest(&bytes))
    };
    let ngx = |name: &OsStr| -> Result<(), Fault> {
        let (path, file) = open(name)?;
        verify_ngx(&path, &file).map_err(Fault::NotNvidia)?;
        held.borrow_mut().push(file);
        Ok(())
    };
    let mut pinned = check_entries(entries.map(|entry| entry.map(|e| e.file_name())), hash, ngx)?;
    pinned._held = held.into_inner();
    pinned._folder = Some(dir);
    Ok(pinned)
}

/// `path` opened for reading, sharing read access only on Windows: no one can write, delete or
/// rename the file while the handle is open (std's default shares all three; `share_mode`
/// replaces it) [documented: Microsoft Learn, "CreateFileW function (fileapi.h)", dwShareMode].
/// With `FILE_FLAG_OPEN_REPARSE_POINT`, a link is opened as itself, not followed ("normal reparse
/// point processing will not occur"; same page, dwFlagsAndAttributes), so [`is_link`] on the
/// handle sees it; a link then holds no file's bytes to hash or verify.
pub(super) fn open_shared_read(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
        options.share_mode(FILE_SHARE_READ);
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

/// The DLSS folder itself opened, as itself if it is a link (`FILE_FLAG_OPEN_REPARSE_POINT`;
/// `FILE_FLAG_BACKUP_SEMANTICS`, which a directory handle needs), sharing read and write but not
/// delete, so that while it is held the folder cannot be renamed or deleted, and files in it can
/// still be read [documented: Microsoft Learn, "CreateFileW function (fileapi.h)", dwShareMode
/// and dwFlagsAndAttributes].
fn open_folder(folder: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE};
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(folder)
}

/// Whether the opened entry is a reparse point (symbolic link, junction, mount point): its
/// attributes read through the handle, which [`open_shared_read`] and [`open_folder`] opened as
/// the entry itself (`MetadataExt::file_attributes`, the `dwFileAttributes` of the handle's
/// file). Off Windows (the tests only), whether `path` is a symbolic link.
#[cfg(windows)]
fn is_link(_path: &Path, file: &File) -> std::io::Result<bool> {
    use std::os::windows::fs::MetadataExt;
    Ok(file.metadata()?.file_attributes() & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

#[cfg(not(windows))]
fn is_link(path: &Path, _file: &File) -> std::io::Result<bool> {
    Ok(std::fs::symlink_metadata(path)?.file_type().is_symlink())
}

/// `VkPhysicalDeviceVulkan12Features`' members by the names Streamline reads them by
/// [documented: `sl_helpers_vk.h:33-87`]; a 1.2 feature DLSS names outside this list is
/// unknown here.
pub(super) const FEATURES_12: [&str; 47] = [
    "samplerMirrorClampToEdge",
    "drawIndirectCount",
    "storageBuffer8BitAccess",
    "uniformAndStorageBuffer8BitAccess",
    "storagePushConstant8",
    "shaderBufferInt64Atomics",
    "shaderSharedInt64Atomics",
    "shaderFloat16",
    "shaderInt8",
    "descriptorIndexing",
    "shaderInputAttachmentArrayDynamicIndexing",
    "shaderUniformTexelBufferArrayDynamicIndexing",
    "shaderStorageTexelBufferArrayDynamicIndexing",
    "shaderUniformBufferArrayNonUniformIndexing",
    "shaderSampledImageArrayNonUniformIndexing",
    "shaderStorageBufferArrayNonUniformIndexing",
    "shaderStorageImageArrayNonUniformIndexing",
    "shaderInputAttachmentArrayNonUniformIndexing",
    "shaderUniformTexelBufferArrayNonUniformIndexing",
    "shaderStorageTexelBufferArrayNonUniformIndexing",
    "descriptorBindingUniformBufferUpdateAfterBind",
    "descriptorBindingSampledImageUpdateAfterBind",
    "descriptorBindingStorageImageUpdateAfterBind",
    "descriptorBindingStorageBufferUpdateAfterBind",
    "descriptorBindingUniformTexelBufferUpdateAfterBind",
    "descriptorBindingStorageTexelBufferUpdateAfterBind",
    "descriptorBindingUpdateUnusedWhilePending",
    "descriptorBindingPartiallyBound",
    "descriptorBindingVariableDescriptorCount",
    "runtimeDescriptorArray",
    "samplerFilterMinmax",
    "scalarBlockLayout",
    "imagelessFramebuffer",
    "uniformBufferStandardLayout",
    "shaderSubgroupExtendedTypes",
    "separateDepthStencilLayouts",
    "hostQueryReset",
    "timelineSemaphore",
    "bufferDeviceAddress",
    "bufferDeviceAddressCaptureReplay",
    "bufferDeviceAddressMultiDevice",
    "vulkanMemoryModel",
    "vulkanMemoryModelDeviceScope",
    "vulkanMemoryModelAvailabilityVisibilityChains",
    "shaderOutputViewportIndex",
    "shaderOutputLayer",
    "subgroupBroadcastDynamicId",
];

/// `VkPhysicalDeviceVulkan13Features`' members by Streamline's names [documented:
/// `sl_helpers_vk.h:89-111`, which lists `robustImageAccess` twice].
pub(super) const FEATURES_13: [&str; 15] = [
    "robustImageAccess",
    "inlineUniformBlock",
    "descriptorBindingInlineUniformBlockUpdateAfterBind",
    "pipelineCreationCacheControl",
    "privateData",
    "shaderDemoteToHelperInvocation",
    "shaderTerminateInvocation",
    "subgroupSizeControl",
    "computeFullSubgroups",
    "synchronization2",
    "textureCompressionASTC_HDR",
    "shaderZeroInitializeWorkgroupMemory",
    "dynamicRendering",
    "shaderIntegerDotProduct",
    "maintenance4",
];

/// The 1.2 features Streamline's `vkCreateDevice` always asks for [documented:
/// `src/wrapper.cpp:282-287`].
const BASE_FEATURES_12: [&str; 3] = ["timelineSemaphore", "descriptorIndexing", "bufferDeviceAddress"];
/// The device extensions it always asks for [documented: `src/wrapper.cpp:291-295`].
const BASE_DEVICE_EXTENSIONS: [&str; 2] = ["VK_KHR_timeline_semaphore", "VK_KHR_push_descriptor"];
/// The extension it adds for a supported 1.2 or 1.3 feature on wgpu-hal's path [documented:
/// `src/wrapper.cpp:649-652, 654-669, 671-686, 718-734`].
const FEATURE_EXTENSIONS: [(&str, &str); 4] = [
    ("descriptorIndexing", "VK_EXT_descriptor_indexing"),
    ("timelineSemaphore", "VK_KHR_timeline_semaphore"),
    ("bufferDeviceAddress", "VK_KHR_buffer_device_address"),
    ("synchronization2", "VK_KHR_synchronization2"),
];
/// The 1.3 feature required here beyond Streamline's (the module comment).
const PRIVATE_DATA: &str = "privateData";

/// The names of the 1.2 features `v` reports, in [`FEATURES_12`]'s order.
pub(super) fn features12_names(v: &vk::PhysicalDeviceVulkan12Features) -> BTreeSet<String> {
    let bits = [
        v.sampler_mirror_clamp_to_edge,
        v.draw_indirect_count,
        v.storage_buffer8_bit_access,
        v.uniform_and_storage_buffer8_bit_access,
        v.storage_push_constant8,
        v.shader_buffer_int64_atomics,
        v.shader_shared_int64_atomics,
        v.shader_float16,
        v.shader_int8,
        v.descriptor_indexing,
        v.shader_input_attachment_array_dynamic_indexing,
        v.shader_uniform_texel_buffer_array_dynamic_indexing,
        v.shader_storage_texel_buffer_array_dynamic_indexing,
        v.shader_uniform_buffer_array_non_uniform_indexing,
        v.shader_sampled_image_array_non_uniform_indexing,
        v.shader_storage_buffer_array_non_uniform_indexing,
        v.shader_storage_image_array_non_uniform_indexing,
        v.shader_input_attachment_array_non_uniform_indexing,
        v.shader_uniform_texel_buffer_array_non_uniform_indexing,
        v.shader_storage_texel_buffer_array_non_uniform_indexing,
        v.descriptor_binding_uniform_buffer_update_after_bind,
        v.descriptor_binding_sampled_image_update_after_bind,
        v.descriptor_binding_storage_image_update_after_bind,
        v.descriptor_binding_storage_buffer_update_after_bind,
        v.descriptor_binding_uniform_texel_buffer_update_after_bind,
        v.descriptor_binding_storage_texel_buffer_update_after_bind,
        v.descriptor_binding_update_unused_while_pending,
        v.descriptor_binding_partially_bound,
        v.descriptor_binding_variable_descriptor_count,
        v.runtime_descriptor_array,
        v.sampler_filter_minmax,
        v.scalar_block_layout,
        v.imageless_framebuffer,
        v.uniform_buffer_standard_layout,
        v.shader_subgroup_extended_types,
        v.separate_depth_stencil_layouts,
        v.host_query_reset,
        v.timeline_semaphore,
        v.buffer_device_address,
        v.buffer_device_address_capture_replay,
        v.buffer_device_address_multi_device,
        v.vulkan_memory_model,
        v.vulkan_memory_model_device_scope,
        v.vulkan_memory_model_availability_visibility_chains,
        v.shader_output_viewport_index,
        v.shader_output_layer,
        v.subgroup_broadcast_dynamic_id,
    ];
    FEATURES_12.iter().zip(bits).filter(|(_, bit)| *bit == vk::TRUE).map(|(name, _)| (*name).to_string()).collect()
}

/// The names of the 1.3 features `v` reports, in [`FEATURES_13`]'s order.
pub(super) fn features13_names(v: &vk::PhysicalDeviceVulkan13Features) -> BTreeSet<String> {
    let bits = [
        v.robust_image_access,
        v.inline_uniform_block,
        v.descriptor_binding_inline_uniform_block_update_after_bind,
        v.pipeline_creation_cache_control,
        v.private_data,
        v.shader_demote_to_helper_invocation,
        v.shader_terminate_invocation,
        v.subgroup_size_control,
        v.compute_full_subgroups,
        v.synchronization2,
        v.texture_compression_astc_hdr,
        v.shader_zero_initialize_workgroup_memory,
        v.dynamic_rendering,
        v.shader_integer_dot_product,
        v.maintenance4,
    ];
    FEATURES_13.iter().zip(bits).filter(|(_, bit)| *bit == vk::TRUE).map(|(name, _)| (*name).to_string()).collect()
}

/// A GPU's kind, as wgpu names it from `VkPhysicalDeviceType` [documented: wgpu-hal 30.0.1
/// `vulkan/adapter.rs:2237-2244`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DeviceKind {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    #[default]
    Other,
}

impl DeviceKind {
    pub(super) fn from_vk(kind: vk::PhysicalDeviceType) -> Self {
        match kind {
            vk::PhysicalDeviceType::DISCRETE_GPU => Self::Discrete,
            vk::PhysicalDeviceType::INTEGRATED_GPU => Self::Integrated,
            vk::PhysicalDeviceType::VIRTUAL_GPU => Self::Virtual,
            vk::PhysicalDeviceType::CPU => Self::Cpu,
            _ => Self::Other,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Discrete => "discrete",
            Self::Integrated => "integrated",
            Self::Virtual => "virtual",
            Self::Cpu => "CPU",
            Self::Other => "other",
        }
    }
}

/// One queue family of a GPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct QueueFamily {
    pub graphics: bool,
    pub compute: bool,
    pub count: u32,
}

/// What one GPU offers, as the system's loader reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DeviceCaps {
    pub name: String,
    pub kind: DeviceKind,
    /// `VkPhysicalDeviceProperties::vendorID` (NVIDIA's is [`NVIDIA_VENDOR`]).
    pub vendor_id: u32,
    /// `VkPhysicalDeviceProperties::apiVersion`.
    pub api_version: u32,
    pub extensions: BTreeSet<String>,
    /// The 1.2 and 1.3 features it reports (names of [`FEATURES_12`], [`FEATURES_13`]); empty
    /// when its version is below.
    pub features12: BTreeSet<String>,
    pub features13: BTreeSet<String>,
    pub queue_families: Vec<QueueFamily>,
    /// `VkPhysicalDeviceIDProperties::deviceLUIDValid`: without a LUID Streamline cannot check
    /// DLSS's support on the device [documented: `src/sl.cpp:855-860`].
    pub luid_valid: bool,
    /// The LUID itself when valid, to tell afterwards whether Bevy made its device on this GPU.
    pub luid: Option<[u8; 8]>,
}

impl DeviceCaps {
    fn has(&self, extension: &str) -> bool {
        self.extensions.contains(extension)
    }

    fn label(&self) -> String {
        format!("{} ({})", self.name, self.kind.name())
    }
}

/// What the system's loader reports: its instance extensions and every GPU, in its order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub instance_extensions: BTreeSet<String>,
    pub devices: Vec<DeviceCaps>,
}

/// What DLSS asks for, copied out of `slGetFeatureRequirements`' answer (`win.rs`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Requirements {
    pub flags: u32,
    pub instance_extensions: Vec<String>,
    pub device_extensions: Vec<String>,
    pub features12: Vec<String>,
    pub features13: Vec<String>,
    pub graphics_queues: u32,
    pub compute_queues: u32,
    pub optical_flow_queues: u32,
}

impl fmt::Display for Requirements {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |items: &[String]| if items.is_empty() { "none".to_string() } else { items.join(", ") };
        write!(
            f,
            "flags {:#x}; instance extensions {}; device extensions {}; 1.2 features {}; 1.3 features {}; extra queues: {} graphics, {} compute, {} optical flow",
            self.flags,
            list(&self.instance_extensions),
            list(&self.device_extensions),
            list(&self.features12),
            list(&self.features13),
            self.graphics_queues,
            self.compute_queues,
            self.optical_flow_queues
        )
    }
}

/// The GPUs (indices into `devices`) the device check covers [design: the final review's P1]:
/// every one the system's loader enumerated, of any kind, CPU devices included. Bevy asks for a
/// GPU by name only as a preference: it passes over the named adapter when it does not support
/// the window's surface or no adapter's name matches, and then takes wgpu's own pick
/// [documented: vendor/bevy_render/src/renderer/mod.rs:166-194, 304], whose filtering
/// (conformance, version, surface support) this check cannot reproduce for certain. So neither
/// the power preference, `WGPU_ADAPTER_NAME` nor `WGPU_FORCE_FALLBACK_ADAPTER` narrows the list,
/// and a GPU wgpu would never offer is checked too (failing closed).
pub(super) fn candidates(devices: &[DeviceCaps]) -> Vec<usize> {
    (0..devices.len()).collect()
}

/// What a GPU lacks for Streamline's device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Shortfall {
    /// Its Vulkan version is below what is asked (`need`).
    Api { have: u32, need: u32 },
    Extension(String),
    Feature12(String),
    Feature13(String),
    /// No queue family holds the extra queues Streamline would add.
    Queues { kind: &'static str, extra: u32 },
    /// No valid LUID (Streamline cannot check DLSS's support on it).
    NoLuid,
}

fn version(v: u32) -> String {
    format!("{}.{}.{}", vk::api_version_major(v), vk::api_version_minor(v), vk::api_version_patch(v))
}

impl fmt::Display for Shortfall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { have, need } => write!(f, "Vulkan {} (has {})", version(*need), version(*have)),
            Self::Extension(name) => write!(f, "{name}"),
            Self::Feature12(name) => write!(f, "the Vulkan 1.2 feature {name}"),
            Self::Feature13(name) => write!(f, "the Vulkan 1.3 feature {name}"),
            Self::Queues { kind, extra } => write!(f, "room for {extra} extra {kind} queue(s)"),
            Self::NoLuid => write!(f, "a valid LUID (Streamline could not check DLSS on it)"),
        }
    }
}

/// The device extensions Streamline's `vkCreateDevice` would ask `device` for.
fn required_extensions(device: &DeviceCaps, req: &Requirements) -> BTreeSet<String> {
    let mut wanted: BTreeSet<String> = BASE_DEVICE_EXTENSIONS.iter().map(|e| (*e).to_string()).collect();
    wanted.extend(req.device_extensions.iter().cloned());
    let features12 = required_features12(req);
    let features13 = required_features13(req);
    for (feature, extension) in FEATURE_EXTENSIONS {
        let asked = features12.contains(feature) || features13.contains(feature);
        let supported = device.features12.contains(feature) || device.features13.contains(feature);
        if asked && supported {
            wanted.insert(extension.to_string());
        }
    }
    wanted
}

fn required_features12(req: &Requirements) -> BTreeSet<&str> {
    BASE_FEATURES_12.iter().copied().chain(req.features12.iter().map(String::as_str)).collect()
}

fn required_features13(req: &Requirements) -> BTreeSet<&str> {
    std::iter::once(PRIVATE_DATA).chain(req.features13.iter().map(String::as_str)).collect()
}

/// Whether family 0 (the one wgpu-hal makes its queue on) plus Streamline's extra queues fit
/// as Streamline adds them: graphics on the last family that draws, compute on the last
/// compute-only family or else family 0, each only beside a queue the renderer asked for there
/// except compute on a family of its own [documented: `src/wrapper.cpp:862-948`].
fn queue_shortfalls(device: &DeviceCaps, req: &Requirements) -> Vec<Shortfall> {
    let families = &device.queue_families;
    let mut out = Vec::new();
    let graphics_family = families.iter().rposition(|f| f.graphics).unwrap_or(0);
    let compute_family = families.iter().rposition(|f| f.compute && !f.graphics).unwrap_or(0);
    let count = |i: usize| families.get(i).map_or(0, |f| f.count);
    let mut on_family_0 = 1u32;
    if req.compute_queues > 0 {
        if compute_family == 0 {
            on_family_0 += req.compute_queues;
            if count(0) < on_family_0 {
                out.push(Shortfall::Queues { kind: "compute", extra: req.compute_queues });
            }
        } else if count(compute_family) < req.compute_queues {
            out.push(Shortfall::Queues { kind: "compute", extra: req.compute_queues });
        }
    }
    if req.graphics_queues > 0 && (graphics_family != 0 || count(0) < on_family_0 + req.graphics_queues) {
        out.push(Shortfall::Queues { kind: "graphics", extra: req.graphics_queues });
    }
    out
}

/// What `device` lacks for Streamline's device and DLSS, in a stable order; empty when it
/// suits.
pub(super) fn shortfalls(device: &DeviceCaps, req: &Requirements) -> Vec<Shortfall> {
    let mut out = Vec::new();
    // 1.2 features are always asked for; 1.3 for privateData (the module comment).
    let need = vk::API_VERSION_1_3;
    if device.api_version < need {
        out.push(Shortfall::Api { have: device.api_version, need });
    }
    for extension in required_extensions(device, req) {
        if !device.has(&extension) {
            out.push(Shortfall::Extension(extension));
        }
    }
    for feature in required_features12(req) {
        if !device.features12.contains(feature) {
            out.push(Shortfall::Feature12(feature.to_string()));
        }
    }
    for feature in required_features13(req) {
        if !device.features13.contains(feature) {
            out.push(Shortfall::Feature13(feature.to_string()));
        }
    }
    out.extend(queue_shortfalls(device, req));
    if !device.luid_valid {
        out.push(Shortfall::NoLuid);
    }
    out
}

/// Why Streamline may not make the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Decline {
    pub reason: Unavailable,
    /// The one log line's text.
    pub why: String,
}

/// The decision [design]: Streamline may make the device only when DLSS lists Vulkan, the loader
/// has every instance extension it asks for, every requirement is one this check knows, and
/// every GPU Vulkan exposes ([`candidates`]) has everything ([`shortfalls`]). Returns the GPUs
/// checked, for the log.
pub(super) fn judge(snapshot: &Snapshot, req: &Requirements) -> Result<Vec<String>, Decline> {
    let unsuitable = |why: String| Decline { reason: Unavailable::DeviceUnsuitable, why };
    if req.flags & FEATURE_REQUIREMENT_VULKAN_SUPPORTED == 0 {
        return Err(Decline { reason: Unavailable::NotVulkan, why: format!("DLSS's requirements do not list Vulkan (flags {:#x})", req.flags) });
    }
    let missing: Vec<&str> = req.instance_extensions.iter().filter(|e| !snapshot.instance_extensions.contains(*e)).map(String::as_str).collect();
    if !missing.is_empty() {
        return Err(unsuitable(format!("the system's Vulkan loader lacks the instance extension(s) {}", missing.join(", "))));
    }
    let mut unknown: Vec<String> = req.features12.iter().filter(|f| !FEATURES_12.contains(&f.as_str())).map(|f| format!("1.2 feature {f}")).collect();
    unknown.extend(req.features13.iter().filter(|f| !FEATURES_13.contains(&f.as_str())).map(|f| format!("1.3 feature {f}")));
    if req.optical_flow_queues > 0 {
        unknown.push(format!("{} optical flow queue(s)", req.optical_flow_queues));
    }
    if !unknown.is_empty() {
        return Err(unsuitable(format!("DLSS asks for what this check does not know: {}", unknown.join(", "))));
    }
    let picks = candidates(&snapshot.devices);
    if picks.is_empty() {
        return Err(unsuitable("the system's Vulkan loader shows no GPU, so the one the renderer will use cannot be checked".to_string()));
    }
    let mut failures = Vec::new();
    for &i in &picks {
        let device = &snapshot.devices[i];
        let lacks = shortfalls(device, req);
        if !lacks.is_empty() {
            let lacks: Vec<String> = lacks.iter().map(ToString::to_string).collect();
            failures.push(format!("{} lacks {}", device.label(), lacks.join(", ")));
        }
    }
    if failures.is_empty() {
        Ok(picks.iter().map(|&i| snapshot.devices[i].label()).collect())
    } else {
        Err(unsuitable(format!("a GPU Vulkan exposes is unsuitable (the renderer could still fall back to it): {}", failures.join("; "))))
    }
}

/// NVIDIA's Vulkan vendor id (its PCI vendor id) [documented: wgpu-hal 30.0.1
/// `auxil/mod.rs:45-48`, `nvidia::VENDOR`].
pub(super) const NVIDIA_VENDOR: u32 = 0x10DE;

/// A GPU the device check passed, as the renderer will report it afterwards: its name (wgpu
/// names an adapter by `deviceName` [documented: wgpu-hal 30.0.1 `vulkan/adapter.rs:2246-2252`])
/// and its LUID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Checked {
    pub name: String,
    pub luid: Option<[u8; 8]>,
}

impl Checked {
    pub(super) fn of(device: &DeviceCaps) -> Self {
        Self { name: device.name.clone(), luid: device.luid }
    }
}

/// What the loader asks the renderer for after the device check passed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Pin {
    /// The renderer is asked for this GPU by name (Bevy's `WgpuSettings::adapter_name`).
    Set(Checked),
    /// `WGPU_ADAPTER_NAME` is set: the player's choice stands (the check covered every GPU
    /// anyway).
    UserChose,
    /// No pin, and why; the check covered every GPU Vulkan exposes all the same.
    Declined(String),
}

/// The adapter pin [design: the midpoint review's P1]: a preference for the NVIDIA GPU among the
/// checked ones, so that on a machine where several pass the renderer still starts on the one
/// DLSS runs on. It does not keep Bevy off an unchecked GPU: Bevy takes the first adapter whose
/// lower-case name contains the asked name and that supports the window's surface, else its own
/// pick [documented: vendor/bevy_render/src/renderer/mod.rs:166-194, 280-315]; what keeps an
/// unchecked GPU out is that every GPU Vulkan exposes is checked ([`candidates`]). The pin is set
/// only when exactly one of the checked GPUs (`checked`, indices into `devices`) is NVIDIA's, its
/// name is contained in no other GPU's name and it has a LUID to check the pick against
/// afterwards. With `WGPU_ADAPTER_NAME` set (`user_name`) Bevy reads the variable over any
/// setting, so nothing is pinned.
pub(super) fn pin(devices: &[DeviceCaps], checked: &[usize], user_name: Option<&str>) -> Pin {
    if user_name.is_some() {
        return Pin::UserChose;
    }
    let nvidia: Vec<usize> = checked.iter().copied().filter(|&i| devices.get(i).is_some_and(|d| d.vendor_id == NVIDIA_VENDOR)).collect();
    let [only] = nvidia.as_slice() else {
        return Pin::Declined(if nvidia.is_empty() { "no checked GPU is NVIDIA's".to_string() } else { format!("{} checked GPUs are NVIDIA's", nvidia.len()) });
    };
    let device = &devices[*only];
    let wanted = device.name.to_lowercase();
    let also: Vec<String> = devices.iter().enumerate().filter(|(i, d)| i != only && d.name.to_lowercase().contains(&wanted)).map(|(_, d)| d.label()).collect();
    if !also.is_empty() {
        return Pin::Declined(format!("the name {} would also pick {}", device.name, also.join(", ")));
    }
    if device.luid.is_none() {
        return Pin::Declined(format!("{} has no valid LUID to check the pick against", device.label()));
    }
    Pin::Set(Checked::of(device))
}

/// A LUID for the log, its eight bytes in hex.
pub(super) fn luid_text(luid: Option<[u8; 8]>) -> String {
    luid.map_or_else(|| "none".to_string(), |l| l.iter().map(|b| format!("{b:02x}")).collect())
}

/// Whether the GPU Bevy made its device on (`chosen_name`, `chosen_luid`) is the one the loader
/// expected: the pinned GPU when there is a pin, else any checked GPU; name and LUID both. The
/// text is the one log line's.
pub(super) fn verify_choice(chosen_name: &str, chosen_luid: Option<[u8; 8]>, pin: Option<&Checked>, checked: &[Checked]) -> Result<String, String> {
    let chosen = format!("{chosen_name} (LUID {})", luid_text(chosen_luid));
    let same = |c: &Checked| c.name == chosen_name && c.luid.is_some() && c.luid == chosen_luid;
    match pin {
        Some(pin) if same(pin) => Ok(format!("the renderer made its device on {chosen}, the pinned GPU")),
        Some(pin) => Err(format!("the renderer made its device on {chosen}, not on the pinned {} (LUID {})", pin.name, luid_text(pin.luid))),
        None if checked.iter().any(same) => Ok(format!("the renderer made its device on {chosen}, a GPU the device check passed")),
        None => Err(format!(
            "the renderer made its device on {chosen}, which the device check did not pass (it passed {})",
            if checked.is_empty() { "none".to_string() } else { checked.iter().map(|c| format!("{} (LUID {})", c.name, luid_text(c.luid))).collect::<Vec<_>>().join(", ") }
        )),
    }
}

/// The snapshot through the system's Vulkan loader: `vulkan-1.dll` from System32 (not the
/// interposer, which is not loaded yet), a temporary Vulkan 1.3 instance (the version
/// Streamline asks for [documented: `src/wrapper.cpp:130-133`]), every GPU's properties,
/// extensions, features and queue families, and the instance destroyed. The loader stays loaded (the renderer loads it again anyway).
#[cfg(windows)]
pub(super) fn probe() -> Result<Snapshot, String> {
    use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

    let name = super::ffi::wide(OsStr::new("vulkan-1.dll"));
    // SAFETY: a NUL-terminated name, searched in System32 only.
    let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        return Err(format!("the system's Vulkan loader (vulkan-1.dll) did not load: {}", std::io::Error::last_os_error()));
    }
    // SAFETY: a loaded module; `vkGetInstanceProcAddr` has this signature.
    let get_instance_proc_addr: Option<vk::PFN_vkGetInstanceProcAddr> = unsafe { super::ffi::get_proc(module, c"vkGetInstanceProcAddr") };
    let Some(get_instance_proc_addr) = get_instance_proc_addr else {
        return Err("the system's Vulkan loader exports no vkGetInstanceProcAddr".to_string());
    };
    // SAFETY: the loader's own entry point.
    let entry = unsafe { ash::Entry::from_static_fn(ash::StaticFn { get_instance_proc_addr }) };
    // SAFETY: a global query.
    let available = unsafe { entry.enumerate_instance_extension_properties(None) }.map_err(|e| format!("vkEnumerateInstanceExtensionProperties: {e}"))?;
    let instance_extensions: BTreeSet<String> = available.iter().filter_map(|p| p.extension_name_as_c_str().ok()).map(|n| n.to_string_lossy().into_owned()).collect();
    let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
    let create = vk::InstanceCreateInfo::default().application_info(&app);
    // SAFETY: a plain instance, no extensions or layers.
    let instance = unsafe { entry.create_instance(&create, None) }.map_err(|e| format!("vkCreateInstance (Vulkan 1.3): {e}"))?;
    let devices = probe_devices(&instance);
    // SAFETY: nothing made from the instance outlives it.
    unsafe { instance.destroy_instance(None) };
    Ok(Snapshot { instance_extensions, devices: devices? })
}

#[cfg(windows)]
fn probe_devices(instance: &ash::Instance) -> Result<Vec<DeviceCaps>, String> {
    // SAFETY: queries on the instance's own physical devices, each structure chained only where
    // the device's version has it.
    unsafe {
        let physicals = instance.enumerate_physical_devices().map_err(|e| format!("vkEnumeratePhysicalDevices: {e}"))?;
        let mut devices = Vec::with_capacity(physicals.len());
        for physical in physicals {
            let properties = instance.get_physical_device_properties(physical);
            let api_version = properties.api_version;
            let extensions: BTreeSet<String> = instance
                .enumerate_device_extension_properties(physical)
                .map_err(|e| format!("vkEnumerateDeviceExtensionProperties: {e}"))?
                .iter()
                .filter_map(|p| p.extension_name_as_c_str().ok())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            let name = properties.device_name_as_c_str().map_or_else(|_| "unnamed GPU".to_string(), |n| n.to_string_lossy().into_owned());
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut properties2 = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            instance.get_physical_device_properties2(physical, &mut properties2);
            let luid_valid = id.device_luid_valid == vk::TRUE;
            let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
            let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
            let (features12, features13) = if api_version >= vk::API_VERSION_1_2 {
                let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut v12);
                if api_version >= vk::API_VERSION_1_3 {
                    features2 = features2.push_next(&mut v13);
                }
                instance.get_physical_device_features2(physical, &mut features2);
                (features12_names(&v12), if api_version >= vk::API_VERSION_1_3 { features13_names(&v13) } else { BTreeSet::new() })
            } else {
                (BTreeSet::new(), BTreeSet::new())
            };
            let queue_families: Vec<QueueFamily> = instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .map(|f| QueueFamily { graphics: f.queue_flags.contains(vk::QueueFlags::GRAPHICS), compute: f.queue_flags.contains(vk::QueueFlags::COMPUTE), count: f.queue_count })
                .collect();
            devices.push(DeviceCaps {
                name,
                kind: DeviceKind::from_vk(properties.device_type),
                vendor_id: properties.vendor_id,
                api_version,
                extensions,
                features12,
                features13,
                queue_families,
                luid_valid,
                luid: luid_valid.then_some(id.device_luid),
            });
        }
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    /// What DLSS asks for on an NVIDIA GPU [documented: `src/dlssEntry.cpp:229-249`; NGX may
    /// add more, `:243-244`], with Vulkan listed.
    fn dlss_requirements() -> Requirements {
        Requirements {
            flags: 0b111,
            instance_extensions: vec!["VK_KHR_get_physical_device_properties2".into(), "VK_KHR_external_memory_capabilities".into(), "VK_KHR_external_semaphore_capabilities".into()],
            device_extensions: vec!["VK_NVX_binary_import".into(), "VK_NVX_image_view_handle".into(), "VK_EXT_buffer_device_address".into(), "VK_KHR_push_descriptor".into()],
            features12: vec!["timelineSemaphore".into(), "descriptorIndexing".into(), "bufferDeviceAddress".into()],
            ..Requirements::default()
        }
    }

    /// An RTX-like GPU with everything.
    fn rtx() -> DeviceCaps {
        DeviceCaps {
            name: "NVIDIA GeForce RTX 5070 Ti".into(),
            kind: DeviceKind::Discrete,
            vendor_id: NVIDIA_VENDOR,
            api_version: vk::make_api_version(0, 1, 4, 312),
            extensions: names(&[
                "VK_KHR_swapchain",
                "VK_KHR_timeline_semaphore",
                "VK_KHR_push_descriptor",
                "VK_EXT_descriptor_indexing",
                "VK_KHR_buffer_device_address",
                "VK_EXT_buffer_device_address",
                "VK_NVX_binary_import",
                "VK_NVX_image_view_handle",
                "VK_KHR_synchronization2",
            ]),
            features12: FEATURES_12.iter().map(|s| (*s).to_string()).collect(),
            features13: FEATURES_13.iter().map(|s| (*s).to_string()).collect(),
            queue_families: vec![QueueFamily { graphics: true, compute: true, count: 16 }, QueueFamily { graphics: false, compute: true, count: 8 }],
            luid_valid: true,
            luid: Some([0x11, 0x22, 0, 0, 0, 0, 0, 0]),
        }
    }

    /// An integrated GPU without NVIDIA's extensions.
    fn igpu() -> DeviceCaps {
        let mut d = rtx();
        d.name = "AMD Radeon(TM) 610M".into();
        d.kind = DeviceKind::Integrated;
        // Any vendor but NVIDIA's (a test value).
        d.vendor_id = 0x1002;
        d.luid = Some([0x33, 0x44, 0, 0, 0, 0, 0, 0]);
        d.extensions.remove("VK_NVX_binary_import");
        d.extensions.remove("VK_NVX_image_view_handle");
        d
    }

    fn snapshot(devices: Vec<DeviceCaps>) -> Snapshot {
        Snapshot { instance_extensions: names(&["VK_KHR_surface", "VK_KHR_win32_surface", "VK_KHR_get_physical_device_properties2", "VK_KHR_external_memory_capabilities", "VK_KHR_external_semaphore_capabilities"]), devices }
    }

    #[test]
    fn a_suitable_gpu_passes() {
        assert_eq!(shortfalls(&rtx(), &dlss_requirements()), vec![]);
        assert_eq!(judge(&snapshot(vec![rtx()]), &dlss_requirements()), Ok(vec!["NVIDIA GeForce RTX 5070 Ti (discrete)".to_string()]));
        // With no NVIDIA GPU, DLSS lists no extensions or features of its own
        // (`src/dlssEntry.cpp:214-249`): Streamline's own baseline still applies.
        let bare = Requirements { flags: 0b111, ..Requirements::default() };
        assert_eq!(shortfalls(&igpu(), &bare), vec![]);
    }

    #[test]
    fn a_missing_extension_is_named() {
        let mut d = rtx();
        d.extensions.remove("VK_NVX_binary_import");
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Extension("VK_NVX_binary_import".into())]);
        // Streamline's own baseline, whatever DLSS asks for.
        let mut d = rtx();
        d.extensions.remove("VK_KHR_push_descriptor");
        assert_eq!(shortfalls(&d, &Requirements { flags: 0b111, ..Requirements::default() }), vec![Shortfall::Extension("VK_KHR_push_descriptor".into())]);
        // A feature promoted to core still needs its extension's name when supported.
        let mut d = rtx();
        d.extensions.remove("VK_EXT_descriptor_indexing");
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Extension("VK_EXT_descriptor_indexing".into())]);
        let err = judge(&snapshot(vec![d]), &dlss_requirements()).unwrap_err();
        assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
        assert!(err.why.contains("NVIDIA GeForce RTX 5070 Ti (discrete) lacks VK_EXT_descriptor_indexing"), "{}", err.why);
    }

    #[test]
    fn a_missing_feature_is_named() {
        let mut d = rtx();
        d.features12.remove("bufferDeviceAddress");
        // Not supported, so Streamline would not ask for its extension either.
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Feature12("bufferDeviceAddress".into())]);
        let mut d = rtx();
        d.features13.remove("privateData");
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::Feature13("privateData".into())]);
        let mut req = dlss_requirements();
        req.features13.push("dynamicRendering".into());
        let mut d = rtx();
        d.features13.remove("dynamicRendering");
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Feature13("dynamicRendering".into())]);
    }

    #[test]
    fn a_too_old_api_is_named() {
        let mut d = rtx();
        d.api_version = vk::make_api_version(0, 1, 2, 200);
        d.features13.clear();
        let lacks = shortfalls(&d, &dlss_requirements());
        assert_eq!(lacks.first(), Some(&Shortfall::Api { have: d.api_version, need: vk::API_VERSION_1_3 }));
        assert_eq!(lacks.first().unwrap().to_string(), "Vulkan 1.3.0 (has 1.2.200)");
        assert!(lacks.contains(&Shortfall::Feature13("privateData".into())));
    }

    #[test]
    fn unknown_requirements_decline() {
        for req in [
            Requirements { features12: vec!["shaderTimeTravel".into()], ..dlss_requirements() },
            Requirements { features13: vec!["robustBufferAccess2".into()], ..dlss_requirements() },
            Requirements { optical_flow_queues: 1, ..dlss_requirements() },
        ] {
            let err = judge(&snapshot(vec![rtx()]), &req).unwrap_err();
            assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
            assert!(err.why.contains("does not know"), "{}", err.why);
        }
        let no_vulkan = Requirements { flags: 0b011, ..dlss_requirements() };
        assert_eq!(judge(&snapshot(vec![rtx()]), &no_vulkan).unwrap_err().reason, Unavailable::NotVulkan);
        let mut loader = snapshot(vec![rtx()]);
        loader.instance_extensions.remove("VK_KHR_external_memory_capabilities");
        let err = judge(&loader, &dlss_requirements()).unwrap_err();
        assert!(err.why.contains("lacks the instance extension(s) VK_KHR_external_memory_capabilities"), "{}", err.why);
    }

    /// A CPU device (a software renderer) with Vulkan 1.3 but none of NVIDIA's extensions.
    fn cpu() -> DeviceCaps {
        let mut d = igpu();
        d.name = "llvmpipe (LLVM 19.1.0, 256 bits)".into();
        d.kind = DeviceKind::Cpu;
        // Any vendor but NVIDIA's (a test value).
        d.vendor_id = 0x10005;
        d.luid = Some([0x55, 0x66, 0, 0, 0, 0, 0, 0]);
        d
    }

    #[test]
    fn every_enumerated_gpu_is_a_candidate() {
        // Of any kind, in the loader's order, and one whose first family cannot draw (which wgpu
        // hides) too: Bevy's fallback is wgpu's own pick, which this check does not predict.
        let mut compute_only = rtx();
        compute_only.queue_families[0].graphics = false;
        let mut other = igpu();
        other.kind = DeviceKind::Other;
        assert_eq!(candidates(&[igpu(), rtx(), cpu(), compute_only, other]), vec![0, 1, 2, 3, 4]);
        assert_eq!(candidates(&[]), Vec::<usize>::new());
    }

    #[test]
    fn one_unsuitable_gpu_of_two_declines() {
        // `judge` takes no power preference, adapter name or forced fallback: whatever Bevy is
        // asked, it may fall back to wgpu's own pick (renderer/mod.rs:166-194, 304), so a laptop's
        // integrated GPU without NVIDIA's extensions declines in either order.
        for devices in [vec![igpu(), rtx()], vec![rtx(), igpu()]] {
            let err = judge(&snapshot(devices), &dlss_requirements()).unwrap_err();
            assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
            assert!(err.why.contains("AMD Radeon(TM) 610M (integrated) lacks VK_NVX_binary_import, VK_NVX_image_view_handle"), "{}", err.why);
            assert!(!err.why.contains("RTX"), "only the failing GPU is named: {}", err.why);
        }
        // A CPU device counts as well.
        let err = judge(&snapshot(vec![rtx(), cpu()]), &dlss_requirements()).unwrap_err();
        assert!(err.why.contains("llvmpipe (LLVM 19.1.0, 256 bits) (CPU) lacks VK_NVX_binary_import"), "{}", err.why);
        // Each failing GPU is named, in one line.
        let mut old = rtx();
        old.name = "NVIDIA GeForce GTX 750".into();
        old.extensions.remove("VK_NVX_binary_import");
        let err = judge(&snapshot(vec![old, igpu()]), &dlss_requirements()).unwrap_err();
        assert!(err.why.contains("NVIDIA GeForce GTX 750 (discrete) lacks VK_NVX_binary_import; AMD Radeon(TM) 610M (integrated) lacks"), "{}", err.why);
        assert!(!err.why.contains('\n'));
    }

    #[test]
    fn a_single_suitable_gpu_passes() {
        assert_eq!(judge(&snapshot(vec![rtx()]), &dlss_requirements()), Ok(vec!["NVIDIA GeForce RTX 5070 Ti (discrete)".to_string()]));
        // Two suitable GPUs: both checked, both listed.
        let mut second = rtx();
        second.name = "NVIDIA GeForce RTX 4060".into();
        second.luid = Some([9; 8]);
        assert_eq!(judge(&snapshot(vec![rtx(), second]), &dlss_requirements()).map(|l| l.len()), Ok(2));
    }

    #[test]
    fn a_named_rtx_does_not_spare_an_unsuitable_igpu() {
        // `WGPU_ADAPTER_NAME=nvidia` (or the adapter pin) asks Bevy for the RTX, but Bevy passes
        // over a named adapter that does not support the window's surface and takes wgpu's own
        // pick: the RTX can drop out of wgpu's eligible adapters and the integrated GPU be used.
        // So the integrated GPU is checked and the route declined.
        let both = snapshot(vec![igpu(), rtx()]);
        let all = candidates(&both.devices);
        assert_eq!(all, vec![0, 1]);
        assert_eq!(pin(&both.devices, &all, Some("nvidia")), Pin::UserChose);
        assert!(matches!(pin(&both.devices, &all, None), Pin::Set(c) if c.name.contains("RTX")), "the pin alone would name the RTX");
        let err = judge(&both, &dlss_requirements()).unwrap_err();
        assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
        assert!(err.why.contains("could still fall back to it"), "{}", err.why);
        assert!(err.why.contains("AMD Radeon(TM) 610M (integrated) lacks"), "{}", err.why);
    }

    #[test]
    fn no_gpu_declines() {
        let err = judge(&snapshot(vec![]), &dlss_requirements()).unwrap_err();
        assert_eq!(err.reason, Unavailable::DeviceUnsuitable);
        assert!(err.why.contains("shows no GPU"), "{}", err.why);
    }

    #[test]
    fn a_missing_luid_declines() {
        let mut d = rtx();
        d.luid_valid = false;
        assert_eq!(shortfalls(&d, &dlss_requirements()), vec![Shortfall::NoLuid]);
        assert!(judge(&snapshot(vec![d]), &dlss_requirements()).unwrap_err().why.contains("valid LUID"));
    }

    #[test]
    fn extra_queues_are_placed_as_streamline_places_them() {
        let mut req = dlss_requirements();
        req.compute_queues = 2;
        assert_eq!(shortfalls(&rtx(), &req), vec![], "a compute-only family of 8");
        let mut d = rtx();
        d.queue_families[1].count = 1;
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Queues { kind: "compute", extra: 2 }]);
        // No compute-only family: on family 0 beside the renderer's queue.
        let mut d = rtx();
        d.queue_families.truncate(1);
        d.queue_families[0].count = 2;
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Queues { kind: "compute", extra: 2 }]);
        d.queue_families[0].count = 3;
        assert_eq!(shortfalls(&d, &req), vec![]);
        let req = Requirements { graphics_queues: 1, ..dlss_requirements() };
        assert_eq!(shortfalls(&rtx(), &req), vec![]);
        // A later family that draws gets Streamline's graphics queues, not the renderer's family 0.
        let mut d = rtx();
        d.queue_families.push(QueueFamily { graphics: true, compute: true, count: 4 });
        assert_eq!(shortfalls(&d, &req), vec![Shortfall::Queues { kind: "graphics", extra: 1 }]);
    }

    #[test]
    fn feature_names_follow_the_vulkan_structures() {
        let none = vk::PhysicalDeviceVulkan12Features::default();
        assert!(features12_names(&none).is_empty());
        let some = vk::PhysicalDeviceVulkan12Features::default().timeline_semaphore(true).buffer_device_address(true).subgroup_broadcast_dynamic_id(true).sampler_mirror_clamp_to_edge(true);
        assert_eq!(features12_names(&some), names(&["timelineSemaphore", "bufferDeviceAddress", "subgroupBroadcastDynamicId", "samplerMirrorClampToEdge"]));
        let some = vk::PhysicalDeviceVulkan13Features::default().private_data(true).robust_image_access(true).maintenance4(true).texture_compression_astc_hdr(true);
        assert_eq!(features13_names(&some), names(&["privateData", "robustImageAccess", "maintenance4", "textureCompressionASTC_HDR"]));
        assert_eq!(FEATURES_12.iter().collect::<BTreeSet<_>>().len(), 47, "no name twice");
        assert_eq!(FEATURES_13.iter().collect::<BTreeSet<_>>().len(), 15);
        assert_eq!(DeviceKind::from_vk(vk::PhysicalDeviceType::DISCRETE_GPU), DeviceKind::Discrete);
        assert_eq!(DeviceKind::from_vk(vk::PhysicalDeviceType::from_raw(99)), DeviceKind::Other);
    }

    #[test]
    fn the_pin_names_the_one_checked_nvidia_gpu() {
        let both = snapshot(vec![igpu(), rtx()]);
        // Every GPU is checked; the pin prefers the NVIDIA one among them.
        let checked = candidates(&both.devices);
        assert_eq!(checked, vec![0, 1]);
        assert_eq!(pin(&both.devices, &checked, None), Pin::Set(Checked { name: "NVIDIA GeForce RTX 5070 Ti".into(), luid: Some([0x11, 0x22, 0, 0, 0, 0, 0, 0]) }));
        // One GPU only (a machine whose Vulkan shows only the RTX).
        assert!(matches!(pin(&[rtx()], &[0], None), Pin::Set(_)));
        // The player's WGPU_ADAPTER_NAME stands.
        assert_eq!(pin(&both.devices, &checked, Some("rtx")), Pin::UserChose);
    }

    #[test]
    fn the_pin_declines_when_the_name_cannot_pick_one_gpu() {
        // Two NVIDIA GPUs checked.
        let mut second = rtx();
        second.name = "NVIDIA GeForce RTX 4060".into();
        second.luid = Some([9; 8]);
        assert!(matches!(pin(&[rtx(), second], &[0, 1], None), Pin::Declined(why) if why.contains("2 checked GPUs")));
        // No NVIDIA GPU checked.
        assert!(matches!(pin(&[igpu(), rtx()], &[0], None), Pin::Declined(why) if why.contains("no checked GPU")));
        // Two GPUs of the same name: the name cannot tell them apart.
        let mut twin = rtx();
        twin.luid = Some([7; 8]);
        twin.vendor_id = 0x1002;
        assert!(matches!(pin(&[rtx(), twin], &[0], None), Pin::Declined(why) if why.contains("would also pick")));
        // A name another GPU's name contains (Bevy matches a part of the name).
        let mut longer = rtx();
        longer.name = "NVIDIA GeForce RTX 5070 Ti Laptop GPU".into();
        longer.vendor_id = 0x1002;
        assert!(matches!(pin(&[longer, rtx()], &[1], None), Pin::Declined(why) if why.contains("would also pick")));
        // Without a LUID the pick cannot be checked afterwards.
        let mut no_luid = rtx();
        no_luid.luid = None;
        assert!(matches!(pin(&[no_luid], &[0], None), Pin::Declined(why) if why.contains("LUID")));
    }

    #[test]
    fn the_chosen_gpu_is_compared_by_name_and_luid() {
        let rtx = Checked::of(&rtx());
        let radeon = Checked::of(&igpu());
        assert!(verify_choice("NVIDIA GeForce RTX 5070 Ti", rtx.luid, Some(&rtx), std::slice::from_ref(&rtx)).unwrap().contains("the pinned GPU"));
        assert!(verify_choice("AMD Radeon(TM) 610M", radeon.luid, Some(&rtx), std::slice::from_ref(&rtx)).unwrap_err().contains("not on the pinned NVIDIA GeForce RTX 5070 Ti (LUID 1122000000000000)"));
        // Same name, another LUID (a second card of the same model): not the pinned one.
        assert!(verify_choice("NVIDIA GeForce RTX 5070 Ti", Some([1; 8]), Some(&rtx), std::slice::from_ref(&rtx)).is_err());
        assert!(verify_choice("NVIDIA GeForce RTX 5070 Ti", None, Some(&rtx), std::slice::from_ref(&rtx)).is_err(), "no LUID: cannot be told");
        // No pin: any checked GPU.
        assert!(verify_choice("AMD Radeon(TM) 610M", radeon.luid, None, &[rtx.clone(), radeon.clone()]).unwrap().contains("a GPU the device check passed"));
        assert!(verify_choice("AMD Radeon(TM) 610M", radeon.luid, None, std::slice::from_ref(&rtx)).unwrap_err().contains("did not pass"));
        assert_eq!(luid_text(None), "none");
    }

    /// The release's hash for a name (any case), as the test's stand-in for reading the file.
    fn release_hash(name: &OsStr) -> Result<[u8; 32], Fault> {
        let upper = upper(name);
        let (_, hex) = RELEASE_DLLS.iter().find(|(file, _)| file.to_ascii_uppercase() == upper).expect("asked only for release files");
        Ok(parse_sha256(hex).expect("the listed hashes parse"))
    }

    #[test]
    fn every_listed_release_hash_parses() {
        for (file, hex) in RELEASE_DLLS {
            let digest = parse_sha256(hex).unwrap_or_else(|| panic!("{file}'s hash parses"));
            assert_eq!(fnv_util::sha256::to_hex(&digest), hex, "{file}: the log writes the listed form");
        }
    }

    /// The NVIDIA signature check's stand-in: every `nvngx_*.dll` signed by NVIDIA.
    fn signed(_: &OsStr) -> Result<(), Fault> {
        Ok(())
    }

    /// `names` judged, every release file holding the release's bytes, every `nvngx_*.dll`
    /// signed by NVIDIA.
    fn judged(names: &[&str]) -> Result<Pinned, FolderRefusal> {
        check_entries(names.iter().map(|n| Ok(OsString::from(n))), release_hash, signed)
    }

    #[test]
    fn the_updater_is_refused_in_any_case() {
        assert!(judged(&["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"]).is_ok());
        assert_eq!(judged(&["sl.interposer.dll", "nvngx_update.exe"]).unwrap_err(), FolderRefusal::Updater("nvngx_update.exe".into()));
        assert_eq!(judged(&["NVNGX_Update.EXE"]).unwrap_err(), FolderRefusal::Updater("NVNGX_Update.EXE".into()));
        assert!(judged(&["nvngx_update.exe.bak", "nvngx_update", "old_nvngx_update.exe"]).is_ok());
    }

    #[test]
    fn the_four_files_and_the_whole_release_folder_pass() {
        let four = judged(&["sl.interposer.dll", "sl.common.dll", "sl.dlss.dll", "nvngx_dlss.dll"]).unwrap();
        assert_eq!(four.pinned, ["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]);
        assert_eq!(four.ngx, ["nvngx_dlss.dll"]);
        assert!(four.to_string().contains("signed by NVIDIA, not pinned by hash: nvngx_dlss.dll"));
        // The archive's whole bin\x64 copied, with its other files (not DLLs) beside them.
        let mut all: Vec<&str> = RELEASE_DLLS.iter().map(|(file, _)| *file).collect();
        all.extend(["nvngx_dlss.dll", "sl.interposer.pdb", "readme.txt"]);
        let whole = judged(&all).unwrap();
        assert_eq!(whole.pinned.len(), RELEASE_DLLS.len());
        assert_eq!(whole.ngx, ["nvngx_dlss.dll"]);
    }

    #[test]
    fn any_other_dll_is_refused() {
        assert_eq!(judged(&["sl.interposer.dll", "version.dll"]).unwrap_err(), FolderRefusal::UnknownDll("version.dll".into()));
        assert_eq!(judged(&["sl.extra.dll"]).unwrap_err(), FolderRefusal::UnknownDll("sl.extra.dll".into()), "an sl. name not in the release");
        assert_eq!(judged(&["vulkan-1.DLL"]).unwrap_err(), FolderRefusal::UnknownDll("vulkan-1.DLL".into()), "the extension in any case");
        assert!(FolderRefusal::UnknownDll("x.dll".into()).to_string().contains("not a DLL of Streamline 2.14.16's release"));
        assert_eq!(FolderRefusal::UnknownDll("x.dll".into()).reason(), Unavailable::SignatureRejected);
    }

    #[test]
    fn a_release_name_with_another_hash_is_refused() {
        let zero = |_: &OsStr| Ok([0u8; 32]);
        let refused = check_entries([Ok(OsString::from("sl.dlss.dll"))], zero, signed).unwrap_err();
        assert_eq!(refused, FolderRefusal::WrongHash("sl.dlss.dll".into(), "0".repeat(64)));
        assert_eq!(refused.reason(), Unavailable::SignatureRejected);
        assert!(refused.to_string().contains(&"0".repeat(64)), "the log names the hash");
        // Each file has its own hash: sl.common.dll's bytes under sl.dlss.dll's name are refused.
        let swapped = |_: &OsStr| release_hash(OsStr::new("sl.common.dll"));
        assert!(matches!(check_entries([Ok(OsString::from("sl.dlss.dll"))], swapped, signed), Err(FolderRefusal::WrongHash(..))));
        let unreadable = |_: &OsStr| Err(Fault::from(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "in use")));
        let refused = check_entries([Ok(OsString::from("sl.common.dll"))], unreadable, signed).unwrap_err();
        assert!(matches!(&refused, FolderRefusal::DllUnreadable(name, why) if name == "sl.common.dll" && why.contains("in use")));
        assert_eq!(refused.reason(), Unavailable::StreamlineRefused);
    }

    #[test]
    fn names_compare_without_case() {
        let pinned = judged(&["SL.Interposer.DLL", "Sl.Common.Dll", "SL.DLSS.DLL", "NVNGX_DLSS.DLL", "nvlowlatencyvk.dll"]).unwrap();
        assert_eq!(pinned.pinned.len(), 4);
        assert_eq!(pinned.ngx, ["NVNGX_DLSS.DLL"]);
        assert_eq!(kind(OsStr::new("ſl.dlss.dll")), Kind::Release(RELEASE_DLLS[4].1), "ſ upper-cases to S: hashed as sl.dlss.dll");
        assert_eq!(kind(OsStr::new("nvngx_dlssg.dll")), Kind::Ngx);
        assert_eq!(kind(OsStr::new("nvngx_dlss.dll.bak")), Kind::Other);
    }

    #[test]
    fn an_unreadable_folder_is_refused() {
        let entries = vec![Ok(OsString::from("sl.interposer.dll")), Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"))];
        assert!(matches!(check_entries(entries, release_hash, signed), Err(FolderRefusal::Unreadable(why)) if why.contains("denied")));
        let gone = std::env::temp_dir().join(format!("nvf-dlss-preflight-gone-{}", std::process::id()));
        assert!(matches!(check_folder(&gone, |_, _| Ok(())), Err(FolderRefusal::Unreadable(_))));
        assert!(FolderRefusal::Unreadable("x".into()).to_string().contains("could not be listed"));
        assert_eq!(FolderRefusal::Unreadable("x".into()).reason(), Unavailable::UpdaterInFolder);
    }

    #[test]
    fn the_updater_is_found_in_a_real_folder() {
        let dir = std::env::temp_dir().join(format!("nvf-dlss-preflight-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nvngx_dlss.dll"), b"").unwrap();
        std::fs::write(dir.join("notes.txt"), b"").unwrap();
        // The NGX file is given to the signature check with its full path and its open handle.
        let mut asked = Vec::new();
        let pinned = check_folder(&dir, |path, file| {
            asked.push((path.to_path_buf(), file.metadata().map(|m| m.len()).ok()));
            Ok(())
        })
        .unwrap();
        assert_eq!(pinned.ngx, ["nvngx_dlss.dll"]);
        assert_eq!(asked, [(dir.join("nvngx_dlss.dll"), Some(0))]);
        drop(pinned);
        // One the signature check refuses keeps Streamline out, the check's words kept.
        let refused = check_folder(&dir, |_, _| Err("signer \"Someone Else\"".to_string())).unwrap_err();
        assert_eq!(refused, FolderRefusal::NotNvidia("nvngx_dlss.dll".into(), "signer \"Someone Else\"".into()));
        // A release file with other bytes is refused, its hash named.
        std::fs::write(dir.join("sl.common.dll"), b"").unwrap();
        let empty = fnv_util::sha256::hex(b"");
        assert_eq!(check_folder(&dir, |_, _| Ok(())).unwrap_err(), FolderRefusal::WrongHash("sl.common.dll".into(), empty));
        std::fs::remove_file(dir.join("sl.common.dll")).unwrap();
        // A folder with an NGX name is not a file: it cannot be opened as one, and is refused.
        std::fs::create_dir_all(dir.join("nvngx_other.dll")).unwrap();
        assert!(matches!(check_folder(&dir, |_, _| Ok(())), Err(FolderRefusal::DllUnreadable(name, _)) if name == "nvngx_other.dll"));
        std::fs::remove_dir(dir.join("nvngx_other.dll")).unwrap();
        // A folder with the updater's name counts too.
        std::fs::create_dir_all(dir.join("Nvngx_Update.exe")).unwrap();
        assert_eq!(check_folder(&dir, |_, _| Ok(())).unwrap_err(), FolderRefusal::Updater("Nvngx_Update.exe".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_ngx_file_not_signed_by_nvidia_or_a_link_is_refused() {
        let unsigned = |_: &OsStr| Err(Fault::NotNvidia("Windows did not verify its signature".into()));
        let refused = check_entries([Ok(OsString::from("NVNGX_DLSS.DLL"))], release_hash, unsigned).unwrap_err();
        assert_eq!(refused, FolderRefusal::NotNvidia("NVNGX_DLSS.DLL".into(), "Windows did not verify its signature".into()));
        assert_eq!(refused.reason(), Unavailable::SignatureRejected);
        assert!(refused.to_string().contains("NVNGX_DLSS.DLL is not signed by NVIDIA Corporation"), "the log names the file");
        // Files that are not DLLs are not given to the check.
        assert!(check_entries([Ok(OsString::from("nvngx_dlss.txt"))], release_hash, unsigned).is_ok());
        let link = |_: &OsStr| Err(Fault::Link);
        let refused = check_entries([Ok(OsString::from("sl.dlss.dll"))], |_: &OsStr| Err(Fault::Link), signed).unwrap_err();
        assert_eq!(refused, FolderRefusal::Link("sl.dlss.dll".into()));
        assert_eq!(refused.reason(), Unavailable::SignatureRejected);
        assert!(refused.to_string().contains("sl.dlss.dll is a link"));
        assert_eq!(check_entries([Ok(OsString::from("nvngx_dlss.dll"))], release_hash, link).unwrap_err(), FolderRefusal::Link("nvngx_dlss.dll".into()));
        let folder = FolderRefusal::FolderLink("C:\\nvf\\dlss".into());
        assert_eq!(folder.reason(), Unavailable::StreamlineRefused);
        assert!(folder.to_string().contains("C:\\nvf\\dlss is a link"));
    }

    /// A symbolic link made for a test; `None` where this account may not make one (Windows
    /// without the privilege or developer mode), when the test has nothing to check.
    fn symlink(target: &Path, link: &Path, dir: bool) -> Option<()> {
        #[cfg(windows)]
        let made = if dir { std::os::windows::fs::symlink_dir(target, link) } else { std::os::windows::fs::symlink_file(target, link) };
        #[cfg(not(windows))]
        let made = {
            let _ = dir;
            std::os::unix::fs::symlink(target, link)
        };
        match made {
            Ok(()) => Some(()),
            Err(e) => {
                eprintln!("symbolic links cannot be made here ({e}); the link checks are left to the run");
                None
            }
        }
    }

    #[test]
    fn links_in_or_as_the_folder_are_refused() {
        let base = std::env::temp_dir().join(format!("nvf-dlss-links-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
        let dir = base.join("dlss");
        std::fs::create_dir_all(&dir).unwrap();
        let elsewhere = base.join("elsewhere.dll");
        std::fs::write(&elsewhere, b"").unwrap();
        let checks = || -> Option<()> {
            // A DLL in the folder that is a link to a file elsewhere.
            symlink(&elsewhere, &dir.join("nvngx_dlss.dll"), false)?;
            let mut asked = 0;
            let refused = check_folder(&dir, |_, _| {
                asked += 1;
                Ok(())
            })
            .unwrap_err();
            assert_eq!(refused, FolderRefusal::Link("nvngx_dlss.dll".into()));
            assert_eq!(asked, 0, "a link never reaches the signature check");
            std::fs::remove_file(dir.join("nvngx_dlss.dll")).unwrap();
            assert!(check_folder(&dir, |_, _| Ok(())).is_ok(), "the folder alone passes");
            // The folder itself a link to the real one.
            let linked = base.join("linked");
            symlink(&dir, &linked, true)?;
            assert_eq!(check_folder(&linked, |_, _| Ok(())).unwrap_err(), FolderRefusal::FolderLink(linked.display().to_string()));
            // Removing a directory link removes the link, not the folder it points at.
            #[cfg(windows)]
            std::fs::remove_dir(&linked).unwrap();
            #[cfg(not(windows))]
            std::fs::remove_file(&linked).unwrap();
            Some(())
        };
        let _ = checks();
        std::fs::remove_dir_all(&base).unwrap();
    }
}
