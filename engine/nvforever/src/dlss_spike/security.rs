//! The interposer's signature check: Streamline's `sl::security::verifyEmbeddedSignature` and
//! `isSignedByNVIDIA` (`include/sl_security.h:134-459`, v2.14.16, saved in
//! `fork-notes/dlss-streamline/sources/include/`), written in Rust. Streamline's guide asks the
//! host to check `sl.interposer.dll` this way, by full path, before loading it
//! (`ProgrammingGuide.md:96-104`) [documented].
//!
//! As in the header, `wintrust.dll` and `crypt32.dll` are loaded from System32 and their
//! functions looked up at run time (`sl_security.h:146-164, 346-354`), so the program's import
//! table is unchanged and nothing is loaded unless the spike runs. Differences from the header
//! [design]: the secondary signature (index 1) is verified against the file by `WinVerifyTrust`
//! too, where the header only compares its signer's public key (see
//! [`verify_embedded_signature`]); handles are closed on every path (guards), where the header's early returns leak
//! some (`:178-192, 238-242`); the signer-info buffers are `Vec`s, not `LocalAlloc`; a nested
//! signature attribute without a value is treated as no signature instead of read.
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

/// `verifyEmbeddedSignature` (`sl_security.h:331-459`), with one check the header lacks: the
/// file's primary Authenticode signature (index 0) verified by Windows
/// (`WINTRUST_ACTION_GENERIC_VERIFY_V2`, no revocation check, the current OS strong-signature
/// policy), exactly one secondary signature, that secondary signature (index 1) verified by
/// Windows against the file too, and its signer's key Streamline's NVIDIA key. A file passes only
/// when all four hold. `path` must be a full path (`:328-330`).
///
/// The header verifies only index 0 and then compares the nested signer's public key
/// (`:407-456`); the key comparison alone does not show that the nested signature signs this
/// file [design, from the stage-1 review], so index 1 goes through `WinVerifyTrust` as well.
pub fn verify_embedded_signature(path: &Path) -> Result<(), String> {
    let path_w = wide(path.as_os_str());
    let wintrust = system_library("wintrust.dll").ok_or("wintrust.dll did not load")?;
    // SAFETY: windows-sys's signature of `WinVerifyTrust`.
    let verify: WinVerifyTrust = unsafe { get_proc(wintrust, c"WinVerifyTrust") }.ok_or("wintrust.dll has no WinVerifyTrust")?;

    // 1. The primary signature, also counting the secondary ones (`sl_security.h:407-426`).
    let primary = verify_signature(verify, &path_w, 0, WSS_GET_SECONDARY_SIG_COUNT | WSS_VERIFY_SPECIFIC);
    if primary.status != 0 {
        return Err(format!("Windows did not verify its primary signature, index 0 (WinVerifyTrust 0x{:08x})", primary.status as u32));
    }
    // 2. Exactly one secondary signature (`:433-438`).
    if primary.secondary_sigs != 1 {
        return Err(format!("{} secondary signatures where Streamline wants exactly 1 (NVIDIA's)", primary.secondary_sigs));
    }
    // 3. That secondary signature verified against the file. `dwVerifiedSigIndex` is "the index
    // used for verification ... set on return" [documented: WINTRUST_SIGNATURE_SETTINGS, see
    // `verify_signature`], so anything but 1 means Windows checked some other signature.
    let secondary = verify_signature(verify, &path_w, 1, WSS_VERIFY_SPECIFIC);
    if secondary.status != 0 {
        return Err(format!("Windows did not verify its secondary signature, index 1 (WinVerifyTrust 0x{:08x})", secondary.status as u32));
    }
    if secondary.verified_index != 1 {
        return Err(format!("Windows reported verifying signature index {} when index 1 was asked for", secondary.verified_index));
    }
    // 4. The secondary signature's signer is Streamline's NVIDIA key (`:441-442`).
    if !is_signed_by_nvidia(&path_w) {
        return Err("its secondary signature is not by Streamline's NVIDIA key".to_string());
    }
    Ok(())
}

/// What one [`verify_signature`] pass returned.
struct SignatureCheck {
    /// `WinVerifyTrust`'s result for the VERIFY call; 0 (`ERROR_SUCCESS`) when trusted.
    status: i32,
    /// `cSecondarySigs`, meaningful only when `WSS_GET_SECONDARY_SIG_COUNT` was set.
    secondary_sigs: u32,
    /// `dwVerifiedSigIndex`.
    verified_index: u32,
}

/// One `WinVerifyTrust` pass over the file's embedded signature number `index`: a VERIFY call,
/// then always a CLOSE call on the same structure, so every pass releases its own state.
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
/// - `dwVerifiedSigIndex`: "the index used for verification ... set on return from Wintrust".
/// - `pCryptoPolicy`: the `CERT_STRONG_SIGN_PARA` policy "a signature must pass to be considered
///   valid".
/// - `WTD_STATEACTION_VERIFY` (1): verify the object; `hWVTStateData` receives a state handle that
///   "must be freed by specifying the WTD_STATEACTION_CLOSE action in a subsequent call";
///   `WTD_STATEACTION_CLOSE` (2) "must be specified for every use of the WTD_STATEACTION_VERIFY
///   action".
/// - `pSignatureSettings` is honoured from "Windows 7 with KB3033929, Windows 8, and Windows
///   Server 2012".
///
/// [hypothesis] Not confirmed by the documentation: that index 1 names the same nested signature
/// `is_signed_by_nvidia` reads (the first value of the file signer's nested-signature attribute)
/// when Windows counts exactly one secondary signature; that the CLOSE call is needed even when
/// VERIFY failed (made here, as the header does, `sl_security.h:454-456`); and that NVIDIA's
/// secondary signature chains to a root Windows trusts and meets the same strong-signature policy
/// as the primary. If the last does not hold, the genuine bundle fails check 3 and the log says
/// so with the `WinVerifyTrust` value.
fn verify_signature(verify: WinVerifyTrust, path_w: &[u16], index: u32, flags: u32) -> SignatureCheck {
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
        dwIndex: index,
        dwFlags: flags,
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
    let (secondary_sigs, verified_index) = unsafe { ((*data.pSignatureSettings).cSecondarySigs, (*data.pSignatureSettings).dwVerifiedSigIndex) };
    // Any state must be released by a call with CLOSE (`sl_security.h:454-456`).
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: as above.
    unsafe { verify(std::ptr::null_mut(), &mut action, std::ptr::from_mut(&mut data).cast()) };
    SignatureCheck { status, secondary_sigs, verified_index }
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
