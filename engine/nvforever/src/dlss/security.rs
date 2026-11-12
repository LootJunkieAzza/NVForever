//! The interposer's signature check: Streamline's `sl::security::verifyEmbeddedSignature` and
//! `isSignedByNVIDIA` (NVIDIA-RTX/Streamline v2.14.16 `include/sl_security.h:134-459`), written
//! in Rust. Streamline's guide asks the host to check `sl.interposer.dll` this way, by full
//! path, before loading it (`docs/ProgrammingGuide.md:96-104`) [documented].
//!
//! As in the header, `wintrust.dll` and `crypt32.dll` are loaded from System32 and their
//! functions looked up at run time (`sl_security.h:146-164, 346-354`), so the program's import
//! table is unchanged and nothing is loaded unless the spike runs. Differences from the header
//! [design]: handles are closed on every path (guards), where the header's early returns leak
//! some (`:178-192, 238-242`); the signer-info buffers are `Vec`s, not `LocalAlloc`; a nested
//! signature attribute without a value is treated as no signature instead of read.
//!
//! # The tested build: a SHA-256 allowlist
//!
//! Beside the signature, the interposer must be a build NVIDIA published and NVForever was tested
//! with: its SHA-256 must be one of [`TESTED_INTERPOSERS`] (today one, Streamline 2.14.16's)
//! [design: the PR owner's decision after the review of #2206, 2026-11-12]. Both checks must
//! pass ([`check_interposer`]); a file that fails either is refused with
//! `Unavailable::SignatureRejected` and its hash in the log. This closes the open point below for
//! the tested build: a file carrying a copied NVIDIA nested signature is not byte-for-byte that
//! build, so its hash differs. A new Streamline release needs its hash added here after it was
//! tested.
//!
//! # Open point: the secondary signature is not bound to the file
//!
//! Like the header, this check verifies the primary signature (index 0) against the file and then
//! only compares the public key of the nested (secondary) signature's signer with Streamline's
//! NVIDIA key. Nothing shows that the nested signature signs this file: a file validly signed by
//! someone else, carrying a copy of NVIDIA's nested-signature attribute from a genuine file, could
//! pass the signature check alone [hypothesis, from the stage-1 review]; the allowlist above
//! refuses it. The cryptographic binding would let other builds than the listed ones be
//! trusted; it stays open. Tried and taken out again:
//! - `WinVerifyTrust` on index 1 (`WSS_VERIFY_SPECIFIC`, `dwIndex` 1) rejects the genuine
//!   `sl.interposer.dll` of Streamline 2.14.16 with `0x800b0109` (`CERT_E_UNTRUSTEDROOT`)
//!   [observed, 2026-11-12]: NVIDIA's secondary signature chains to a root Windows does not trust,
//!   which is presumably why the header pins the key instead [hypothesis].
//! - Accepting `CERT_E_UNTRUSTEDROOT` for index 1 is not supported by the documentation:
//!   `WinVerifyTrust` "No other value besides zero should be considered a successful return", and
//!   no page found says that a chain error means the file's digest was compared and matched
//!   [documented: Microsoft Learn, "WinVerifyTrust function (wintrust.h)", Return value; "Example
//!   C Program: Verifying the Signature of a PE File", read 2026-11-12].
//!
//! The route left [hypothesis, not built]: verify the nested PKCS#7 message itself
//! against the pinned certificate (`CryptMsgControl`, `CMSG_CTRL_VERIFY_SIGNATURE_EX`), decode its
//! content as `SpcIndirectDataContent` and check that digest against this file through the
//! subject interface package (`CryptSIPRetrieveSubjectGuid`, `CryptSIPLoad`, the dispatch's
//! verify function, documented as "validates the indirect hashed data against the supplied
//! subject", Microsoft Learn, "CryptSIPVerifyIndirectData function (mssip.h)"). Not done here
//! because the documentation does not state that `CMSG_CTRL_VERIFY_SIGNATURE` checks the
//! signed message-digest attribute against the content, nor the decode type for
//! `SpcIndirectDataContent`, and the code could not be run against the genuine file in this step.
//!
//! Translated from NVIDIA Streamline (https://github.com/NVIDIA-RTX/Streamline), whose header
//! `sl_security.h` carries this notice (licence file `license.txt` in the Streamline repository):
//!
//! ```text
//! Copyright (c) 2022-2023 NVIDIA CORPORATION. All rights reserved
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```

use std::ffi::{c_void, CStr};
use std::path::Path;

use windows_sys::core::{BOOL, GUID, PCSTR};
use windows_sys::Win32::Foundation::{LocalFree, HMODULE, HWND};
use windows_sys::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_INFO, CERT_STRONG_SIGN_PARA, CERT_STRONG_SIGN_PARA_0, CMSG_SIGNER_INFO, CRYPT_INTEGER_BLOB, HCERTSTORE, CERT_CLOSE_STORE_FORCE_FLAG,
    CERT_FIND_SUBJECT_CERT, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED, CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_OBJECT_FILE, CERT_STORE_PROV_PKCS7,
    CERT_STRONG_SIGN_OID_INFO_CHOICE, CMSG_SIGNER_INFO_PARAM, CNG_RSA_PUBLIC_KEY_BLOB, CRYPT_DECODE_ALLOC_FLAG, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
};
use windows_sys::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WINTRUST_SIGNATURE_SETTINGS, WSS_GET_SECONDARY_SIG_COUNT,
    WSS_VERIFY_SPECIFIC, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

use super::ffi::{get_proc, wide};
use crate::upscaler::Unavailable;

/// `szOID_CERT_STRONG_SIGN_OS_CURRENT` as an ANSI string: the header casts the ANSI macro to
/// `LPSTR` (`sl_security.h:419`), and `CERT_STRONG_SIGN_PARA`'s `pszOID` is a `PSTR`; windows-sys
/// has the constant only as a wide string (`Win32/Security/Cryptography/mod.rs:9764`, value
/// "1.3.6.1.4.1.311.72.1.1") [documented].
const STRONG_SIGN_OS_CURRENT: &CStr = c"1.3.6.1.4.1.311.72.1.1";

/// The nested-signature attribute's OID (`sl_security.h:213`) [documented].
const NESTED_SIGNATURE: &CStr = c"1.3.6.1.4.1.311.2.4.1";

/// Streamline's RSA public key as a CNG `RSA1` blob, which the secondary signature's certificate
/// must carry (`sl_security.h:274-301`) [documented].
const STREAMLINE_PUBLIC_KEY: [u8; 411] = [
    0x52, 0x53, 0x41, 0x31, 0x00, 0x0c, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01,
    0xc1, 0x8e, 0x40, 0xc3, 0xf5, 0xa7, 0x01, 0x9a, 0x37, 0x6b, 0x47, 0xa8, 0x58, 0xe8, 0xbe, 0xe3, 0x55, 0x0a, 0xee, 0x0f, 0x0d, 0x32, 0xaa, 0x12, 0xf9, 0x56, 0x7f,
    0x5d, 0xfd, 0x82, 0x09, 0x33, 0x21, 0x42, 0xf2, 0xe8, 0x74, 0x98, 0x51, 0xb3, 0x88, 0x74, 0xcd, 0x00, 0x6e, 0xb1, 0x08, 0x10, 0x4b, 0xf1, 0xda, 0xd6, 0x97, 0x87,
    0xd4, 0x9c, 0xb1, 0x13, 0xa8, 0xa2, 0x86, 0x15, 0x0e, 0xc1, 0xa5, 0x9c, 0xe5, 0x90, 0x9b, 0xbe, 0x69, 0xdc, 0x6a, 0x82, 0xbe, 0xb4, 0x4b, 0x4b, 0xfa, 0x95, 0x8e,
    0xc1, 0xfc, 0x2b, 0x61, 0x95, 0xd1, 0x91, 0xed, 0xeb, 0x87, 0xe7, 0x09, 0x84, 0x05, 0x41, 0x03, 0xb0, 0x2d, 0xd4, 0x39, 0x7f, 0x62, 0x06, 0x56, 0x33, 0x93, 0x7e,
    0x77, 0x54, 0x06, 0x77, 0x2b, 0x75, 0x05, 0xbc, 0xeb, 0x98, 0xea, 0xc0, 0xa2, 0xca, 0x98, 0x86, 0x0f, 0x10, 0x65, 0xde, 0x19, 0x2c, 0xa6, 0x1e, 0x93, 0xb0, 0x92,
    0x5d, 0x5f, 0x5b, 0x6f, 0x79, 0x6d, 0x2c, 0x76, 0xa6, 0x67, 0x50, 0xaa, 0x8f, 0xc2, 0x4c, 0xf1, 0x08, 0xf7, 0xc0, 0x27, 0x29, 0xf0, 0x68, 0xf4, 0x64, 0x00, 0x1c,
    0xb6, 0x28, 0x1e, 0x25, 0xb8, 0xf3, 0x8a, 0xd1, 0x6e, 0x65, 0xa3, 0x61, 0x9d, 0xf8, 0xca, 0x4a, 0x41, 0x60, 0x80, 0x62, 0xdf, 0x41, 0xa4, 0x8b, 0xdc, 0x97, 0xee,
    0xeb, 0x64, 0x6f, 0xe4, 0x8f, 0x4b, 0xdf, 0x24, 0x01, 0x80, 0xd9, 0xb4, 0x0a, 0xec, 0x0d, 0x3e, 0xb7, 0x76, 0xba, 0xe9, 0xe7, 0xde, 0x07, 0xdd, 0x30, 0xc8, 0x4a,
    0x14, 0x79, 0xec, 0x15, 0xed, 0x5c, 0xc6, 0xcc, 0xd4, 0xe6, 0x06, 0x3c, 0x42, 0x92, 0x10, 0xf7, 0x7c, 0x80, 0x1e, 0x78, 0xd3, 0xb4, 0x9f, 0xc2, 0x3b, 0xa8, 0x7b,
    0xa0, 0xe3, 0x0c, 0xd9, 0xad, 0x2e, 0x09, 0x72, 0xe2, 0x8f, 0x54, 0x28, 0x87, 0x3c, 0xba, 0x7c, 0x97, 0x80, 0xdc, 0x09, 0xb5, 0x12, 0x34, 0x78, 0x9a, 0x26, 0xd0,
    0xa3, 0xa7, 0xa7, 0x1b, 0x25, 0x19, 0xe5, 0x6e, 0xbe, 0xd7, 0x5a, 0x91, 0x32, 0xc4, 0xa9, 0x2f, 0xcc, 0xd5, 0x82, 0x4b, 0x5b, 0x9f, 0xad, 0xf3, 0x2f, 0xed, 0x4f,
    0x33, 0xe1, 0x50, 0x33, 0xd6, 0x90, 0x79, 0x22, 0xe5, 0x1c, 0xc7, 0x35, 0xe7, 0x58, 0xe6, 0xb4, 0x8b, 0xc4, 0x28, 0x20, 0xec, 0xca, 0x70, 0xbb, 0x02, 0x1b, 0x48,
    0xd8, 0x84, 0x51, 0x24, 0x33, 0x2a, 0x08, 0xb1, 0x15, 0x4e, 0xbc, 0x88, 0xa5, 0xe1, 0x37, 0x76, 0x70, 0xe6, 0xdf, 0x3f, 0x73, 0xfd, 0x0d, 0x8a, 0xd9, 0x0d, 0xa5,
    0x35, 0xb2, 0xb4, 0x01, 0x42, 0x96, 0xc4, 0xaa, 0x1c, 0xeb, 0x68, 0x62, 0x36, 0xbf, 0xef, 0x5e, 0x2a, 0x3d, 0x18, 0x91, 0x8b, 0x92, 0x0a, 0x1e, 0xce, 0x98, 0x5b,
    0x7b, 0x64, 0x42, 0x09, 0xb0, 0x1d,
];

/// The `sl.interposer.dll` builds NVForever was tested with: what each is, and its SHA-256 in
/// hex. Only these are loaded (the module's "The tested build").
pub(super) const TESTED_INTERPOSERS: [(&str, &str); 1] = [(
    "Streamline 2.14.16",
    // `bin\x64\sl.interposer.dll` of NVIDIA's release archive `streamline-sdk-v2.14.16.zip`
    // [observed: the release zip's file and the tested bundle, 2026-11-12].
    "b1d7395aebd7b9722841d64dcf7433680020b6a8e421f851c16d5ba0c2182edf",
)];

/// 64 hex digits (either case) as 32 bytes; `None` for anything else.
fn parse_sha256(hex: &str) -> Option<[u8; 32]> {
    let digits = hex.as_bytes();
    if digits.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (byte, pair) in out.iter_mut().zip(digits.chunks_exact(2)) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *byte = (high * 16 + low) as u8;
    }
    Some(out)
}

/// The tested build whose SHA-256 is `digest`, if any.
fn tested_build(digest: &[u8; 32]) -> Option<&'static str> {
    TESTED_INTERPOSERS.iter().find(|(_, hex)| parse_sha256(hex).as_ref() == Some(digest)).map(|(build, _)| *build)
}

/// The tested builds as the log names them: "Streamline 2.14.16 b1d7…".
fn tested_list() -> String {
    TESTED_INTERPOSERS.iter().map(|(build, hex)| format!("{build} {hex}")).collect::<Vec<_>>().join(", ")
}

/// The interposer, accepted by [`check_interposer`].
pub(super) struct Accepted {
    /// The file's SHA-256 in hex.
    pub(super) sha256: String,
    /// The tested build it is.
    pub(super) build: &'static str,
}

/// Why [`check_interposer`] refused the interposer: the reason the settings show, and the log's
/// words.
pub(super) struct Refused {
    pub(super) reason: Unavailable,
    pub(super) why: String,
}

/// The interposer's two checks, both of which must pass: its signature
/// ([`verify_embedded_signature`]) and its SHA-256 being a tested build's ([`TESTED_INTERPOSERS`]).
/// `path` is a full path.
pub(super) fn check_interposer(path: &Path) -> Result<Accepted, Refused> {
    let bytes = std::fs::read(path).map_err(|e| Refused { reason: Unavailable::StreamlineRefused, why: format!("it could not be read for its checks: {e}") })?;
    let digest = fnv_util::sha256::digest(&bytes);
    let sha256 = fnv_util::sha256::to_hex(&digest);
    verify_embedded_signature(path).map_err(|why| Refused { reason: Unavailable::SignatureRejected, why: format!("{why} (SHA-256 {sha256})") })?;
    let Some(build) = tested_build(&digest) else {
        return Err(Refused {
            reason: Unavailable::SignatureRejected,
            why: format!("its SHA-256 {sha256} is not a tested build's (tested: {}); its signature passed", tested_list()),
        });
    };
    Ok(Accepted { sha256, build })
}

// The signatures of the functions looked up, as windows-sys declares them
// (`Win32/Security/WinTrust/mod.rs:15`, `Win32/Security/Cryptography/mod.rs:84-285`), with
// pointers to structures this file does not build written as `*const c_void`.
type WinVerifyTrust = unsafe extern "system" fn(HWND, *mut GUID, *mut c_void) -> i32;
type CryptQueryObject =
    unsafe extern "system" fn(u32, *const c_void, u32, u32, u32, *mut u32, *mut u32, *mut u32, *mut HCERTSTORE, *mut *mut c_void, *mut *mut c_void) -> BOOL;
type CryptMsgGetParam = unsafe extern "system" fn(*const c_void, u32, u32, *mut c_void, *mut u32) -> BOOL;
type CryptMsgOpenToDecode = unsafe extern "system" fn(u32, u32, u32, usize, *const CERT_INFO, *const c_void) -> *mut c_void;
type CryptMsgUpdate = unsafe extern "system" fn(*const c_void, *const u8, u32, BOOL) -> BOOL;
type CryptMsgClose = unsafe extern "system" fn(*const c_void) -> BOOL;
type CertOpenStore = unsafe extern "system" fn(PCSTR, u32, usize, u32, *const c_void) -> HCERTSTORE;
type CertCloseStore = unsafe extern "system" fn(HCERTSTORE, u32) -> BOOL;
type CertFindCertificateInStore = unsafe extern "system" fn(HCERTSTORE, u32, u32, u32, *const c_void, *const CERT_CONTEXT) -> *mut CERT_CONTEXT;
type CertFreeCertificateContext = unsafe extern "system" fn(*const CERT_CONTEXT) -> BOOL;
type CryptDecodeObjectEx = unsafe extern "system" fn(u32, PCSTR, *const u8, u32, u32, *const c_void, *mut c_void, *mut u32) -> BOOL;

/// A System32 library, loaded by name from System32 only (`sl_security.h:150, 349`).
fn system_library(name: &str) -> Option<HMODULE> {
    let name = wide(name.as_ref());
    // SAFETY: a NUL-terminated name, searched in System32 only.
    let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    (!module.is_null()).then_some(module)
}

/// `verifyEmbeddedSignature` (`sl_security.h:331-459`): the file's primary Authenticode signature
/// (index 0) verified by Windows (`WINTRUST_ACTION_GENERIC_VERIFY_V2`, no revocation check, the
/// current OS strong-signature policy), exactly one secondary signature, and that secondary
/// signature's signer carrying Streamline's NVIDIA key. A file passes only when all three hold.
/// `path` must be a full path (`:328-330`). The secondary signature is not verified against the
/// file; see the module's open point.
fn verify_embedded_signature(path: &Path) -> Result<(), String> {
    let path_w = wide(path.as_os_str());
    let wintrust = system_library("wintrust.dll").ok_or("wintrust.dll did not load")?;
    // SAFETY: windows-sys's signature of `WinVerifyTrust`.
    let verify: WinVerifyTrust = unsafe { get_proc(wintrust, c"WinVerifyTrust") }.ok_or("wintrust.dll has no WinVerifyTrust")?;

    // 1. The primary signature, also counting the secondary ones (`sl_security.h:407-426`).
    let primary = verify_primary(verify, &path_w);
    if primary.status != 0 {
        return Err(format!("Windows did not verify its primary signature, index 0 (WinVerifyTrust 0x{:08x})", primary.status as u32));
    }
    // 2. Exactly one secondary signature (`:433-438`).
    if primary.secondary_sigs != 1 {
        return Err(format!("{} secondary signatures where Streamline wants exactly 1 (NVIDIA's)", primary.secondary_sigs));
    }
    // 3. The secondary signature's signer is Streamline's NVIDIA key (`:441-442`).
    if !is_signed_by_nvidia(&path_w) {
        return Err("its secondary signature is not by Streamline's NVIDIA key".to_string());
    }
    Ok(())
}

/// What the [`verify_primary`] pass returned.
struct PrimaryCheck {
    /// `WinVerifyTrust`'s result for the VERIFY call; 0 (`ERROR_SUCCESS`) when trusted.
    status: i32,
    /// `cSecondarySigs`.
    secondary_sigs: u32,
}

/// One `WinVerifyTrust` pass over the file's primary signature (index 0, `WSS_VERIFY_SPECIFIC`,
/// counting the secondary signatures with `WSS_GET_SECONDARY_SIG_COUNT`): a VERIFY call, then
/// always a CLOSE call on the same structure, so the pass releases its own state.
///
/// Field meanings [documented: Microsoft Learn, "WINTRUST_SIGNATURE_SETTINGS structure
/// (wintrust.h)" and "WINTRUST_DATA structure (wintrust.h)", read 2026-11-12; declarations as
/// windows-sys 0.61.2 has them, `Win32/Security/WinTrust/mod.rs:775-789, 856-863, 960, 969,
/// 1085, 1087`]:
/// - `dwIndex`: "the index of the signature to be validated if the dwFlags member is set to
///   WSS_VERIFY_SPECIFIC"; `WSS_VERIFY_SPECIFIC` (1): "set this value if you set the dwIndex
///   parameter".
/// - `WSS_GET_SECONDARY_SIG_COUNT` (2): return the number of secondary signatures found in
///   `cSecondarySigs`.
/// - `pCryptoPolicy`: the `CERT_STRONG_SIGN_PARA` policy "a signature must pass to be considered
///   valid".
/// - `WTD_STATEACTION_VERIFY` (1): verify the object; `hWVTStateData` receives a state handle that
///   "must be freed by specifying the WTD_STATEACTION_CLOSE action in a subsequent call";
///   `WTD_STATEACTION_CLOSE` (2) "must be specified for every use of the WTD_STATEACTION_VERIFY
///   action".
/// - `pSignatureSettings` is honoured from "Windows 7 with KB3033929, Windows 8, and Windows
///   Server 2012".
///
/// [hypothesis] Not confirmed by the documentation: that the nested signature `is_signed_by_nvidia`
/// reads (the first value of the file signer's nested-signature attribute) is the one Windows
/// counts in `cSecondarySigs`; that the CLOSE call is needed even when VERIFY failed (made here,
/// as the header does, `sl_security.h:454-456`).
fn verify_primary(verify: WinVerifyTrust, path_w: &[u16]) -> PrimaryCheck {
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path_w.as_ptr(),
        hFile: std::ptr::null_mut(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut policy = CERT_STRONG_SIGN_PARA {
        cbSize: size_of::<CERT_STRONG_SIGN_PARA>() as u32,
        dwInfoChoice: CERT_STRONG_SIGN_OID_INFO_CHOICE,
        Anonymous: CERT_STRONG_SIGN_PARA_0 { pszOID: STRONG_SIGN_OS_CURRENT.as_ptr().cast_mut().cast() },
    };
    let mut settings = WINTRUST_SIGNATURE_SETTINGS {
        cbStruct: size_of::<WINTRUST_SIGNATURE_SETTINGS>() as u32,
        dwIndex: 0,
        dwFlags: WSS_GET_SECONDARY_SIG_COUNT | WSS_VERIFY_SPECIFIC,
        pCryptoPolicy: &mut policy,
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        pSignatureSettings: &mut settings,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: every pointer in `data` points at a local that outlives both calls.
    let status = unsafe { verify(std::ptr::null_mut(), &mut action, std::ptr::from_mut(&mut data).cast()) };
    // Read through the pointer Windows wrote through (`sl_security.h:434`).
    // SAFETY: `pSignatureSettings` still points at `settings`.
    let secondary_sigs = unsafe { (*data.pSignatureSettings).cSecondarySigs };
    // Any state must be released by a call with CLOSE (`sl_security.h:454-456`).
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: as above.
    unsafe { verify(std::ptr::null_mut(), &mut action, std::ptr::from_mut(&mut data).cast()) };
    PrimaryCheck { status, secondary_sigs }
}

/// crypt32's functions used by [`is_signed_by_nvidia`] (`sl_security.h:150-162`).
struct Crypt32 {
    query_object: CryptQueryObject,
    msg_get_param: CryptMsgGetParam,
    msg_open_to_decode: CryptMsgOpenToDecode,
    msg_update: CryptMsgUpdate,
    msg_close: CryptMsgClose,
    open_store: CertOpenStore,
    close_store: CertCloseStore,
    find_certificate: CertFindCertificateInStore,
    free_certificate: CertFreeCertificateContext,
    decode_object: CryptDecodeObjectEx,
}

impl Crypt32 {
    fn load() -> Option<Self> {
        let m = system_library("crypt32.dll")?;
        // SAFETY: each name with windows-sys's signature for it.
        unsafe {
            Some(Self {
                query_object: get_proc(m, c"CryptQueryObject")?,
                msg_get_param: get_proc(m, c"CryptMsgGetParam")?,
                msg_open_to_decode: get_proc(m, c"CryptMsgOpenToDecode")?,
                msg_update: get_proc(m, c"CryptMsgUpdate")?,
                msg_close: get_proc(m, c"CryptMsgClose")?,
                open_store: get_proc(m, c"CertOpenStore")?,
                close_store: get_proc(m, c"CertCloseStore")?,
                find_certificate: get_proc(m, c"CertFindCertificateInStore")?,
                free_certificate: get_proc(m, c"CertFreeCertificateContext")?,
                decode_object: get_proc(m, c"CryptDecodeObjectEx")?,
            })
        }
    }

    /// A message's signer information (`CMSG_SIGNER_INFO_PARAM`, size first, then the data;
    /// `sl_security.h:183-211, 224-231`), in a buffer aligned for `CMSG_SIGNER_INFO`.
    fn signer_info(&self, msg: *const c_void) -> Option<Vec<u64>> {
        let mut size = 0u32;
        // SAFETY: a size query on an open message.
        if unsafe { (self.msg_get_param)(msg, CMSG_SIGNER_INFO_PARAM, 0, std::ptr::null_mut(), &mut size) } == 0 || size == 0 {
            return None;
        }
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: the buffer holds `size` bytes, 8-aligned.
        let ok = unsafe { (self.msg_get_param)(msg, CMSG_SIGNER_INFO_PARAM, 0, buffer.as_mut_ptr().cast(), &mut size) };
        (ok != 0 && size as usize >= size_of::<CMSG_SIGNER_INFO>()).then_some(buffer)
    }
}

/// Closes a message (`CryptMsgClose`) when dropped.
struct Msg<'a>(&'a Crypt32, *mut c_void);
impl Drop for Msg<'_> {
    fn drop(&mut self) {
        // SAFETY: an open message handle, closed once.
        unsafe { (self.0.msg_close)(self.1) };
    }
}

/// Closes a certificate store (`CERT_CLOSE_STORE_FORCE_FLAG`, `sl_security.h:316`) when dropped.
struct Store<'a>(&'a Crypt32, HCERTSTORE);
impl Drop for Store<'_> {
    fn drop(&mut self) {
        // SAFETY: an open store handle, closed once.
        unsafe { (self.0.close_store)(self.1, CERT_CLOSE_STORE_FORCE_FLAG) };
    }
}

/// Frees a certificate context when dropped.
struct Cert<'a>(&'a Crypt32, *mut CERT_CONTEXT);
impl Drop for Cert<'_> {
    fn drop(&mut self) {
        // SAFETY: a context from `CertFindCertificateInStore`, freed once.
        unsafe { (self.0.free_certificate)(self.1) };
    }
}

/// `isSignedByNVIDIA` (`sl_security.h:134-326`): the file's signer information, its nested
/// (secondary) signature, that signature's signer certificate, and whether the certificate's
/// public key is Streamline's.
fn is_signed_by_nvidia(path_w: &[u16]) -> bool {
    let Some(crypt) = Crypt32::load() else { return false };
    let encoding = X509_ASN_ENCODING | PKCS_7_ASN_ENCODING;
    let (mut enc, mut content, mut format) = (0u32, 0u32, 0u32);
    let mut store: HCERTSTORE = std::ptr::null_mut();
    let mut msg: *mut c_void = std::ptr::null_mut();
    // SAFETY: a NUL-terminated path; out pointers to locals (`sl_security.h:167-177`).
    let found = unsafe {
        (crypt.query_object)(
            CERT_QUERY_OBJECT_FILE,
            path_w.as_ptr().cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            &mut enc,
            &mut content,
            &mut format,
            &mut store,
            &mut msg,
            std::ptr::null_mut(),
        )
    };
    if found == 0 {
        return false;
    }
    let _store = (!store.is_null()).then(|| Store(&crypt, store));
    let _msg = (!msg.is_null()).then(|| Msg(&crypt, msg));
    if msg.is_null() {
        return false;
    }
    let Some(signer) = crypt.signer_info(msg) else { return false };
    // SAFETY: `signer_info` checked the buffer holds at least a `CMSG_SIGNER_INFO`, 8-aligned.
    let signer = unsafe { &*signer.as_ptr().cast::<CMSG_SIGNER_INFO>() };
    let attrs = signer.UnauthAttrs;
    for i in 0..attrs.cAttr as usize {
        // SAFETY: `cAttr` attributes at `rgAttr`, inside the signer buffer.
        let attr = unsafe { &*attrs.rgAttr.add(i) };
        // SAFETY: an attribute's OID is a NUL-terminated string.
        if attr.pszObjId.is_null() || unsafe { CStr::from_ptr(attr.pszObjId.cast()) } != NESTED_SIGNATURE {
            continue;
        }
        if attr.cValue == 0 || attr.rgValue.is_null() {
            return false;
        }
        // SAFETY: at least one value.
        let blob: &CRYPT_INTEGER_BLOB = unsafe { &*attr.rgValue };
        return nested_signer_is_streamline(&crypt, encoding, blob);
    }
    false
}

/// The nested signature in `blob` (`sl_security.h:219-308`): decoded as a PKCS#7 message, its
/// signer's certificate found in the signature's own store, the certificate's public key decoded
/// as a CNG RSA blob and compared with Streamline's.
fn nested_signer_is_streamline(crypt: &Crypt32, encoding: u32, blob: &CRYPT_INTEGER_BLOB) -> bool {
    // SAFETY: no stream info or recipient (`sl_security.h:219`).
    let msg2 = unsafe { (crypt.msg_open_to_decode)(encoding, 0, 0, 0, std::ptr::null(), std::ptr::null()) };
    if msg2.is_null() {
        return false;
    }
    let _msg2 = Msg(crypt, msg2);
    // SAFETY: the blob's bytes, the final (only) update.
    if unsafe { (crypt.msg_update)(msg2, blob.pbData, blob.cbData, 1) } == 0 {
        return false;
    }
    let Some(signer2) = crypt.signer_info(msg2) else { return false };
    // SAFETY: as in `is_signed_by_nvidia`.
    let signer2 = unsafe { &*signer2.as_ptr().cast::<CMSG_SIGNER_INFO>() };
    // SAFETY: the PKCS#7 store provider reads the blob (`sl_security.h:233-237`).
    let store2 = unsafe { (crypt.open_store)(CERT_STORE_PROV_PKCS7, encoding, 0, 0, std::ptr::from_ref(blob).cast()) };
    if store2.is_null() {
        return false;
    }
    let _store2 = Store(crypt, store2);
    // SAFETY: an all-zero CERT_INFO is valid (the header value-initialises it, `:244`).
    let mut info: CERT_INFO = unsafe { std::mem::zeroed() };
    info.Issuer = signer2.Issuer;
    info.SerialNumber = signer2.SerialNumber;
    // SAFETY: the subject is found by issuer and serial number (`sl_security.h:250-255`).
    let cert = unsafe { (crypt.find_certificate)(store2, encoding, 0, CERT_FIND_SUBJECT_CERT, std::ptr::from_ref(&info).cast(), std::ptr::null()) };
    if cert.is_null() {
        return false;
    }
    let _cert = Cert(crypt, cert);
    // SAFETY: a context Windows returned, with its certificate information.
    let key = unsafe { &(*(*cert).pCertInfo).SubjectPublicKeyInfo.PublicKey };
    let mut decoded: *mut c_void = std::ptr::null_mut();
    let mut length = 0u32;
    // SAFETY: the header passes CRYPT_ENCODE_ALLOC_FLAG (`:268`), whose value 0x8000 is
    // CRYPT_DECODE_ALLOC_FLAG's; Windows allocates the result, freed with LocalFree (`:303`).
    let ok = unsafe {
        (crypt.decode_object)(encoding, CNG_RSA_PUBLIC_KEY_BLOB, key.pbData, key.cbData, CRYPT_DECODE_ALLOC_FLAG, std::ptr::null(), std::ptr::from_mut(&mut decoded).cast(), &mut length)
    };
    if ok == 0 || decoded.is_null() {
        return false;
    }
    // SAFETY: Windows allocated `length` bytes at `decoded`.
    let same = length as usize == STREAMLINE_PUBLIC_KEY.len() && unsafe { std::slice::from_raw_parts(decoded.cast::<u8>(), length as usize) } == STREAMLINE_PUBLIC_KEY;
    // SAFETY: allocated by CryptDecodeObjectEx with the alloc flag.
    unsafe { LocalFree(decoded) };
    same
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_is_parsed_strictly() {
        let lower = parse_sha256(TESTED_INTERPOSERS[0].1).expect("the listed hash parses");
        assert_eq!(&lower[..4], &[0xb1, 0xd7, 0x39, 0x5a]);
        assert_eq!(lower[31], 0xdf);
        assert_eq!(parse_sha256(&TESTED_INTERPOSERS[0].1.to_ascii_uppercase()), Some(lower), "either case");
        let zeros = "0".repeat(64);
        assert_eq!(parse_sha256(&zeros), Some([0; 32]));
        assert_eq!(parse_sha256(&"0".repeat(63)), None, "too short");
        assert_eq!(parse_sha256(&"0".repeat(65)), None, "too long");
        assert_eq!(parse_sha256(&format!("{}g", "0".repeat(63))), None, "not hex");
        assert_eq!(parse_sha256(&format!("+{}", "0".repeat(63))), None, "a sign is not a digit");
        assert_eq!(parse_sha256(""), None);
    }

    #[test]
    fn only_a_tested_build_is_matched() {
        let tested = parse_sha256(TESTED_INTERPOSERS[0].1).unwrap();
        assert_eq!(tested_build(&tested), Some("Streamline 2.14.16"));
        let mut other = tested;
        other[31] ^= 1;
        assert_eq!(tested_build(&other), None, "one bit off");
        assert_eq!(tested_build(&[0; 32]), None);
        // The empty file's SHA-256 is not a tested build's.
        assert_eq!(tested_build(&fnv_util::sha256::digest(b"")), None);
        assert_eq!(fnv_util::sha256::to_hex(&tested), TESTED_INTERPOSERS[0].1, "the log writes the listed form");
        assert!(tested_list().contains("Streamline 2.14.16 b1d7395aebd7"));
    }
}
