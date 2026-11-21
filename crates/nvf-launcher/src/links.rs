//! Nexus and ModPub downloads.
//!
//! `nxm://newvegas/mods/{mod}/files/{file}?key=&expires=` is Nexus's
//! "Mod Manager Download" for New Vegas. The archive comes from
//! `GET /v1/games/newvegas/mods/{mod}/files/{file}/download_link.json`
//! with the `apikey` header. A free account also sends the link's `key`
//! and `expires`. Premium can omit those. The API key is sealed in
//! `nexus-api.txt` for this user account, not stored as a plain line.
//! `NEXUS_API_KEY` still overrides it. The raw key is not written into messages.
//! `modl://falloutnv/?url={percent-encoded https}` is a direct file.
//! The `url` is decoded once, the way Qt does: `%HH` is a byte, then UTF-8,
//! and `+` is a plus, not a space. No API key is sent. The host has to be a
//! public site name. `other` means this open launcher.
//!
//! The archive is saved in the launcher's `downloads` folder. It is not
//! installed, and it is not copied into the game folder.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};

use fnv_install::guard_write;

/// What a finished download should tell the window.
pub enum DownloadDone {
    /// File name in `downloads`. Not installed.
    Saved(String),
    /// This nxm link cannot start until a Nexus API key is saved.
    NeedKey(String),
    Failed(String),
}

/// Whether this process should open the window.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    /// Open the window.
    Run,
    /// A link was handed to the launcher that is already open.
    Stop,
}

#[derive(Debug)]
enum Link {
    Nxm { mod_id: u64, file_id: u64, key: Option<String>, expires: Option<String> },
    Modl { url: String },
}

const QUEUE: &str = "pending-links.txt";
const KEY_FILE: &str = "nexus-api.txt";

static HELD: OnceLock<std::fs::File> = OnceLock::new();

/// Claim this process as the one that receives links, or hand a link to the one
/// that already did. A normal second window still opens. A second process that
/// only carries a link does not.
pub fn startup() -> Startup {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| looks_like_link(a)).collect();
    let Some(home) = fnv_install::EngineHome::user() else {
        if !args.is_empty() {
            eprintln!("launcher: no home folder, so the link was not kept");
            return Startup::Stop;
        }
        return Startup::Run;
    };
    if !args.is_empty() && !claim(&home.root) {
        match enqueue(&home.root, &args) {
            Ok(()) => eprintln!("launcher: handed the link to the open launcher"),
            Err(e) => eprintln!("launcher: {e}"),
        }
        return Startup::Stop;
    }
    let _ = claim(&home.root);
    if !args.is_empty() {
        if let Err(e) = enqueue(&home.root, &args) {
            eprintln!("launcher: {e}");
        }
    }
    Startup::Run
}

fn claim(root: &Path) -> bool {
    if HELD.get().is_some() {
        return true;
    }
    let _ = std::fs::create_dir_all(root);
    let Ok(file) = std::fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(root.join("launcher.lock")) else {
        return true;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = HELD.set(file);
            true
        }
        Err(std::fs::TryLockError::WouldBlock) => false,
        Err(_) => true,
    }
}

fn looks_like_link(arg: &str) -> bool {
    let a = arg.trim().trim_matches('"').trim().to_ascii_lowercase();
    a.starts_with("nxm://") || a.starts_with("modl://")
}

fn clean_link(raw: &str) -> Result<String, String> {
    let line = raw.trim().trim_matches('"').trim();
    if line.is_empty() || line.len() > 4000 || line.chars().any(|c| c.is_control()) || !looks_like_link(line) {
        return Err("That link cannot be kept.".into());
    }
    Ok(line.to_string())
}

pub(crate) fn enqueue(root: &Path, raws: &[String]) -> Result<(), String> {
    let mut clean = Vec::with_capacity(raws.len());
    for raw in raws {
        clean.push(clean_link(raw)?);
    }
    with_queue(root, |lines| lines.extend(clean))
}

/// The next link a running window should download, if one is waiting.
pub fn pop(root: &Path) -> Option<String> {
    let path = root.join(QUEUE);
    if std::fs::metadata(&path).ok()?.len() == 0 {
        return None;
    }
    let mut taken = None;
    with_queue(root, |lines| {
        if !lines.is_empty() {
            taken = Some(lines.remove(0));
        }
    })
    .ok()?;
    taken
}

fn with_queue(root: &Path, change: impl FnOnce(&mut Vec<String>)) -> Result<(), String> {
    let path = root.join(QUEUE);
    guard_write(&path, None).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.lock().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
    change(&mut lines);
    file.seek(SeekFrom::Start(0)).map_err(|e| format!("{}: {e}", path.display()))?;
    file.set_len(0).map_err(|e| format!("{}: {e}", path.display()))?;
    if !lines.is_empty() {
        let body = lines.join("\n") + "\n";
        file.write_all(body.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Download one link on a background thread. The file lands in `downloads`.
pub fn spawn(root: PathBuf, raw: String) -> mpsc::Receiver<DownloadDone> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(fetch(&root, &raw));
    });
    rx
}

fn fetch(root: &Path, raw: &str) -> DownloadDone {
    match parse(raw) {
        Ok(Link::Nxm { mod_id, file_id, key, expires }) => fetch_nxm(root, raw, mod_id, file_id, key.as_deref(), expires.as_deref()),
        Ok(Link::Modl { url }) => match save_https(root, &url, "download") {
            Ok(name) => DownloadDone::Saved(name),
            Err(e) => DownloadDone::Failed(e),
        },
        Err(e) => DownloadDone::Failed(e),
    }
}

fn fetch_nxm(root: &Path, raw: &str, mod_id: u64, file_id: u64, key: Option<&str>, expires: Option<&str>) -> DownloadDone {
    let Some(api) = api_key(root) else {
        return DownloadDone::NeedKey(raw.to_string());
    };
    let list = nexus_request(mod_id, file_id, key, expires);
    let agent = crate::news::download_agent();
    let response = agent.get(&list).header("apikey", &api).header("Accept", "application/json").config().http_status_as_error(false).build().call();
    let mut resp = match response {
        Ok(resp) => resp,
        Err(e) => return DownloadDone::Failed(http_err("Nexus", e)),
    };
    let status = resp.status().as_u16();
    let daily = header_digits(resp.headers(), "x-rl-daily-remaining");
    let hourly = header_digits(resp.headers(), "x-rl-hourly-remaining");
    let pace = rate_note(daily.as_deref(), hourly.as_deref());
    let text = match resp.body_mut().read_to_string() {
        Ok(text) => text,
        Err(_) => return DownloadDone::Failed("Nexus did not return a download list.".into()),
    };
    if status != 200 {
        return match nexus_stop(status, &text) {
            NexusStop::NeedKey => DownloadDone::NeedKey(raw.to_string()),
            NexusStop::Failed(e) => DownloadDone::Failed(match pace {
                Some(note) if status == 429 => format!("{e} {note}"),
                _ => e,
            }),
        };
    }
    match first_https(&text) {
        Ok(uri) => match save_https(root, &uri, &nxm_fallback(mod_id, file_id, &uri)) {
            Ok(name) => DownloadDone::Saved(name),
            Err(e) => DownloadDone::Failed(e),
        },
        Err(e) => DownloadDone::Failed(e),
    }
}

/// True when `raw` is a New Vegas nxm link. A bad link is false.
pub fn is_nxm(raw: &str) -> bool {
    matches!(parse(raw), Ok(Link::Nxm { .. }))
}

pub fn has_api_key(root: &Path) -> bool {
    api_key(root).is_some()
}

/// The API key, sealed in `nexus-api.txt`. Not the game folder, and not a plain line.
pub fn save_api_key(root: &Path, key: &str) -> Result<(), String> {
    let key = key.trim();
    if !key_chars(key) {
        return Err("That does not look like a Nexus API key. Paste the key on its own, with no spaces.".into());
    }
    let path = root.join(KEY_FILE);
    guard_write(&path, None).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let (tag, blob) = seal(key.as_bytes())?;
    let body = format!("NVFKEY1 {tag}\n{}\n", hex_encode(&blob));
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))
}

/// Why a saved key cannot be used. A plain file is removed so the key does not stay readable.
pub fn saved_key_error(root: &Path) -> Option<String> {
    let path = root.join(KEY_FILE);
    if !path.is_file() {
        return None;
    }
    match open_key_file(&path) {
        Ok(_) => None,
        Err(KeyFile::Plain) => {
            if guard_write(&path, None).is_ok() {
                let _ = std::fs::remove_file(&path);
            }
            Some("The Nexus API key was stored in plain text, so that file was removed. Paste it again.".into())
        }
        Err(KeyFile::Bad) => Some("The saved Nexus API key could not be read on this account. Paste it again.".into()),
    }
}

const HANDLER_FILE: &str = "link-handler.txt";

/// Whether the player already answered the one-time link question.
pub fn handler_decided(root: &Path) -> bool {
    handler_choice(root).is_some() || handler_installed()
}

pub fn handler_choice(root: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(root.join(HANDLER_FILE)).ok()?;
    match text.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

pub fn remember_handler(root: &Path, yes: bool) -> Result<(), String> {
    let path = root.join(HANDLER_FILE);
    guard_write(&path, None).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let body = if yes { "yes\n" } else { "no\n" };
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn handler_installed() -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Classes\nxm", "/v", "URL Protocol"])
            .status()
            .ok()
            .is_some_and(|s| s.success())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        applications_dir().is_some_and(|dir| dir.join("nvforever-links.desktop").is_file())
    }
    #[cfg(target_os = "macos")]
    {
        false
    }
}

fn api_key(root: &Path) -> Option<String> {
    api_key_from(root, std::env::var("NEXUS_API_KEY").ok().as_deref())
}

fn api_key_from(root: &Path, env: Option<&str>) -> Option<String> {
    if let Some(key) = env.map(str::trim).filter(|k| key_chars(k)) {
        return Some(key.to_string());
    }
    let path = root.join(KEY_FILE);
    open_key_file(&path).ok()
}

enum KeyFile {
    Plain,
    Bad,
}

fn open_key_file(path: &Path) -> Result<String, KeyFile> {
    let text = std::fs::read_to_string(path).map_err(|_| KeyFile::Bad)?;
    let mut lines = text.lines();
    let head = lines.next().unwrap_or("").trim();
    let Some(tag) = head.strip_prefix("NVFKEY1 ") else {
        let line = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'));
        return Err(if line.is_some_and(key_chars) { KeyFile::Plain } else { KeyFile::Bad });
    };
    let raw = hex_decode(lines.next().unwrap_or("").trim()).map_err(|_| KeyFile::Bad)?;
    let plain = match tag {
        "nix" => open_portable(&raw).map_err(|_| KeyFile::Bad)?,
        #[cfg(windows)]
        "win" => open_dpapi(&raw).map_err(|_| KeyFile::Bad)?,
        #[cfg(not(windows))]
        "win" => return Err(KeyFile::Bad),
        _ => return Err(KeyFile::Bad),
    };
    let key = String::from_utf8(plain).map_err(|_| KeyFile::Bad)?;
    key_chars(key.trim()).then(|| key.trim().to_string()).ok_or(KeyFile::Bad)
}

fn seal(plain: &[u8]) -> Result<(&'static str, Vec<u8>), String> {
    #[cfg(windows)]
    {
        seal_dpapi(plain).map(|b| ("win", b))
    }
    #[cfg(not(windows))]
    {
        seal_portable(plain).map(|b| ("nix", b))
    }
}

#[cfg(not(windows))]
fn seal_portable(plain: &[u8]) -> Result<Vec<u8>, String> {
    let enc = account_key(b"enc");
    let mac_key = account_key(b"mac");
    let nonce = random_nonce()?;
    let mut ct = plain.to_vec();
    chacha20_xor(&enc, &nonce, 1, &mut ct);
    let mut msg = Vec::with_capacity(12 + ct.len());
    msg.extend_from_slice(&nonce);
    msg.extend_from_slice(&ct);
    let mac = hmac_sha256(&mac_key, &msg);
    msg.extend_from_slice(&mac);
    Ok(msg)
}

fn open_portable(blob: &[u8]) -> Result<Vec<u8>, ()> {
    if blob.len() < 12 + 32 {
        return Err(());
    }
    let (nonce, rest) = blob.split_at(12);
    let (ct, mac) = rest.split_at(rest.len() - 32);
    let mac_key = account_key(b"mac");
    let mut msg = Vec::with_capacity(12 + ct.len());
    msg.extend_from_slice(nonce);
    msg.extend_from_slice(ct);
    if !mac_eq(&hmac_sha256(&mac_key, &msg), mac) {
        return Err(());
    }
    let mut plain = ct.to_vec();
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| ())?;
    chacha20_xor(&account_key(b"enc"), &nonce, 1, &mut plain);
    Ok(plain)
}

/// A key for this account on this machine. Not a password, and not stored.
fn account_key(purpose: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"nvforever-nexus-api-v1\0");
    h.update(purpose);
    h.update(b"\0");
    if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
        h.update(id.trim().as_bytes());
    }
    h.update(account_uid().to_le_bytes());
    if let Ok(user) = std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        h.update(user.as_bytes());
    }
    if let Ok(host) = std::env::var("COMPUTERNAME") {
        h.update(host.as_bytes());
    }
    h.finalize().into()
}

fn account_uid() -> u32 {
    let Ok(text) = std::fs::read_to_string("/proc/self/status") else { return 0 };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            if let Some(n) = rest.split_whitespace().next().and_then(|n| n.parse().ok()) {
                return n;
            }
        }
    }
    0
}

#[cfg(not(windows))]
fn random_nonce() -> Result<[u8; 12], String> {
    let mut buf = [0u8; 12];
    let mut file = std::fs::File::open("/dev/urandom").map_err(|_| "Could not seal the API key.".to_string())?;
    std::io::Read::read_exact(&mut file, &mut buf).map_err(|_| "Could not seal the API key.".to_string())?;
    Ok(buf)
}

#[cfg(windows)]
fn seal_dpapi(plain: &[u8]) -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    use windows_sys::Win32::System::Memory::LocalFree;
    let mut owned = plain.to_vec();
    let input = CRYPT_INTEGER_BLOB { cbData: owned.len() as u32, pbData: owned.as_mut_ptr() };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe { CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) };
    if ok == 0 || output.pbData.is_null() {
        return Err("The API key could not be protected for this Windows account.".into());
    }
    let blob = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData as *mut c_void); }
    Ok(blob)
}

#[cfg(windows)]
fn open_dpapi(blob: &[u8]) -> Result<Vec<u8>, ()> {
    use std::ffi::c_void;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    use windows_sys::Win32::System::Memory::LocalFree;
    let mut owned = blob.to_vec();
    let input = CRYPT_INTEGER_BLOB { cbData: owned.len() as u32, pbData: owned.as_mut_ptr() };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe { CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output) };
    if ok == 0 || output.pbData.is_null() {
        return Err(());
    }
    let plain = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData as *mut c_void); }
    Ok(plain)
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    outer.finalize().into()
}

fn mac_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b) {
        d |= x ^ y;
    }
    d == 0
}

fn chacha20_xor(key: &[u8; 32], nonce: &[u8; 12], mut counter: u32, data: &mut [u8]) {
    let mut off = 0;
    while off < data.len() {
        let block = chacha20_block(key, nonce, counter);
        let n = (data.len() - off).min(64);
        for i in 0..n {
            data[off + i] ^= block[i];
        }
        off += n;
        counter = counter.wrapping_add(1);
    }
}

fn chacha20_block(key: &[u8; 32], nonce: &[u8; 12], counter: u32) -> [u8; 64] {
    fn u32le(b: &[u8]) -> u32 {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    fn qr(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
        s[a] = s[a].wrapping_add(s[b]);
        s[d] ^= s[a];
        s[d] = s[d].rotate_left(16);
        s[c] = s[c].wrapping_add(s[d]);
        s[b] ^= s[c];
        s[b] = s[b].rotate_left(12);
        s[a] = s[a].wrapping_add(s[b]);
        s[d] ^= s[a];
        s[d] = s[d].rotate_left(8);
        s[c] = s[c].wrapping_add(s[d]);
        s[b] ^= s[c];
        s[b] = s[b].rotate_left(7);
    }
    let mut s = [
        0x6170_7865,
        0x3320_646e,
        0x7962_2d32,
        0x6b20_6574,
        u32le(&key[0..4]),
        u32le(&key[4..8]),
        u32le(&key[8..12]),
        u32le(&key[12..16]),
        u32le(&key[16..20]),
        u32le(&key[20..24]),
        u32le(&key[24..28]),
        u32le(&key[28..32]),
        counter,
        u32le(&nonce[0..4]),
        u32le(&nonce[4..8]),
        u32le(&nonce[8..12]),
    ];
    let init = s;
    for _ in 0..10 {
        qr(&mut s, 0, 4, 8, 12);
        qr(&mut s, 1, 5, 9, 13);
        qr(&mut s, 2, 6, 10, 14);
        qr(&mut s, 3, 7, 11, 15);
        qr(&mut s, 0, 5, 10, 15);
        qr(&mut s, 1, 6, 11, 12);
        qr(&mut s, 2, 7, 8, 13);
        qr(&mut s, 3, 4, 9, 14);
    }
    for i in 0..16 {
        s[i] = s[i].wrapping_add(init[i]);
    }
    let mut out = [0u8; 64];
    for (i, word) in s.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, ()> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8, ()> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(()),
    }
}

fn key_chars(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '/' | '='))
}

/// How a non-200 answer from the download-link call should be shown.
enum NexusStop {
    NeedKey,
    Failed(String),
}

fn nexus_stop(status: u16, body: &str) -> NexusStop {
    let msg = json_message(body);
    let about_key = msg.to_ascii_lowercase().contains("api key");
    if status == 401 || (status == 403 && about_key) {
        return NexusStop::NeedKey;
    }
    let plain = match status {
        400 => "Nexus rejected that link's key or expiry. Open the mod page and use Mod Manager Download again.",
        403 => "Nexus refused this download. Open the mod page and use Mod Manager Download.",
        404 => "Nexus has no file at that link.",
        410 => "That Nexus link expired. Open the mod page and use Mod Manager Download again.",
        429 => "Nexus asked for a pause. Try the download again shortly.",
        _ => return NexusStop::Failed(if msg.is_empty() { format!("Nexus returned {status}.") } else { format!("Nexus returned {status}. {}", one_line(&msg)) }),
    };
    let text = if msg.is_empty() { plain.to_string() } else { format!("{plain} {}", one_line(&msg)) };
    NexusStop::Failed(text)
}

fn json_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return String::new();
    };
    for key in ["message", "error"] {
        if let Some(text) = value.get(key).and_then(|m| m.as_str()) {
            return text.to_string();
        }
    }
    String::new()
}

fn header_digits(headers: &ureq::http::HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    digits(value).then(|| value.to_string())
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.len() <= 8 && s.chars().all(|c| c.is_ascii_digit())
}

/// How many requests Nexus says are left. Daily first, then hourly.
fn rate_note(daily: Option<&str>, hourly: Option<&str>) -> Option<String> {
    if let Some(n) = daily.filter(|s| digits(s)) {
        return Some(format!("Daily requests left: {n}."));
    }
    hourly.filter(|s| digits(s)).map(|n| format!("Hourly requests left: {n}."))
}

fn nexus_request(mod_id: u64, file_id: u64, key: Option<&str>, expires: Option<&str>) -> String {
    let mut url = format!("https://api.nexusmods.com/v1/games/newvegas/mods/{mod_id}/files/{file_id}/download_link.json");
    let mut query = Vec::new();
    if let Some(key) = key.filter(|k| token_ok(k)) {
        query.push(format!("key={}", query_encode(key)));
    }
    if let Some(expires) = expires.filter(|k| token_ok(k)) {
        query.push(format!("expires={}", query_encode(expires)));
    }
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }
    url
}

fn token_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2000 && !s.chars().any(|c| c.is_control())
}

fn first_https(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| "Nexus did not return a download list.".to_string())?;
    if let Some(msg) = value.get("message").and_then(|m| m.as_str()) {
        return Err(format!("Nexus said: {}.", one_line(msg)));
    }
    let uris = value.as_array().ok_or_else(|| "Nexus did not return a download list.".to_string())?;
    for item in uris {
        if let Some(uri) = item.get("URI").and_then(|u| u.as_str()) {
            if https_download(uri).is_ok() {
                return Ok(uri.to_string());
            }
        }
    }
    Err("Nexus did not return an https download.".into())
}

fn one_line(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if out.len() >= 160 {
            break;
        }
        if !c.is_control() {
            out.push(c);
        }
    }
    out
}

fn http_err(what: &str, err: ureq::Error) -> String {
    match err {
        ureq::Error::StatusCode(code) => format!("{what} returned {code}."),
        ureq::Error::Timeout(_) => format!("{what} timed out."),
        ureq::Error::HostNotFound => format!("{what}: host not found."),
        ureq::Error::ConnectionFailed => format!("{what}: connection failed."),
        ureq::Error::TooManyRedirects => format!("{what}: too many redirects."),
        _ => format!("{what} failed."),
    }
}

fn save_https(root: &Path, url: &str, fallback: &str) -> Result<String, String> {
    https_download(url)?;
    let dir = downloads_dir(root)?;
    let agent = crate::news::download_agent();
    // No API key is attached. ModPub is the file itself. A Nexus file is the
    // https address Nexus already returned.
    let mut current = wire_url(url);
    let mut saved = None;
    for _ in 0..5 {
        addresses_are_public(&current)?;
        let response = agent.get(&current).config().http_status_as_error(false).max_redirects(0).build().call().map_err(|e| http_err("Download", e))?;
        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let loc = response.headers().get("location").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            current = redirect_target(&current, &loc)?;
            continue;
        }
        if status != 200 {
            return Err(format!("Download returned {status}."));
        }
        saved = Some(response);
        break;
    }
    let mut resp = saved.ok_or_else(|| "That download was sent on too many times.".to_string())?;
    let header = resp.headers().get("content-disposition").and_then(|v| v.to_str().ok()).map(str::to_string);
    let name = preferred_name(url, fallback, header.as_deref());
    let dest = unique_path(&dir, &name)?;
    guard_write(&dest, None).map_err(|e| e.to_string())?;
    let part = dir.join(format!(".{}.part", dest.file_name().and_then(|n| n.to_str()).unwrap_or("download")));
    guard_write(&part, None).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut reader = resp.body_mut().as_reader();
    std::io::copy(&mut reader, &mut file).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Download failed: {e}")
    })?;
    drop(file);
    std::fs::rename(&part, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("{}: {e}", dest.display())
    })?;
    Ok(dest.file_name().and_then(|n| n.to_str()).unwrap_or("download").to_string())
}

fn downloads_dir(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join("downloads");
    guard_write(&dir, None).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let canon = dir.canonicalize().unwrap_or(dir);
    guard_write(&canon, None).map_err(|e| e.to_string())?;
    Ok(canon)
}

fn nxm_fallback(mod_id: u64, file_id: u64, url: &str) -> String {
    let from_url = file_name_from_url(url);
    let ext = std::path::Path::new(&from_url).extension().and_then(|e| e.to_str()).unwrap_or("");
    if !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        format!("mod-{mod_id}-{file_id}.{ext}")
    } else {
        format!("mod-{mod_id}-{file_id}")
    }
}

fn preferred_name(url: &str, fallback: &str, header: Option<&str>) -> String {
    if let Some(name) = header.and_then(disposition_name) {
        return name;
    }
    let from_url = file_name_from_url(url);
    if !from_url.is_empty() && from_url != "download.bin" {
        return from_url;
    }
    sanitize_file_name(fallback).unwrap_or_else(|| "download.bin".into())
}

fn disposition_name(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if let Some(i) = lower.find("filename*=") {
        let rest = value[i + "filename*=".len()..].split(';').next().unwrap_or("").trim().trim_matches('"');
        let encoded = rest.split("''").nth(1).unwrap_or(rest);
        return sanitize_file_name(&percent_decode(encoded));
    }
    let i = lower.find("filename=")?;
    let rest = value[i + "filename=".len()..].split(';').next().unwrap_or("").trim();
    let name = rest.trim_matches('"');
    sanitize_file_name(&percent_decode(name))
}

fn file_name_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let seg = path.rsplit('/').next().unwrap_or("");
    sanitize_file_name(&percent_decode(seg)).unwrap_or_else(|| "download.bin".into())
}

fn sanitize_file_name(raw: &str) -> Option<String> {
    let name = raw.trim().trim_matches(|c| c == '"' || c == '\'');
    let bad = name.is_empty()
        || name.len() > 120
        || name == "."
        || name == ".."
        || name.starts_with('.')
        || name.ends_with('.')
        || name.ends_with(' ')
        || reserved_file_name(name)
        || name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
    if bad { None } else { Some(name.to_string()) }
}

fn unique_path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let name = sanitize_file_name(name).ok_or_else(|| "That file name cannot be used.".to_string())?;
    let dest = dir.join(&name);
    if !dest.exists() {
        return Ok(dest);
    }
    let path = Path::new(&name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("download");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for i in 2..1000 {
        let next = if ext.is_empty() { format!("{stem} ({i})") } else { format!("{stem} ({i}).{ext}") };
        let dest = dir.join(&next);
        if !dest.exists() {
            return Ok(dest);
        }
    }
    Err("Too many downloads with that name.".into())
}

fn reserved_file_name(name: &str) -> bool {
    let stem = name.split(['.', ' ']).next().unwrap_or(name);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

fn parse(raw: &str) -> Result<Link, String> {
    let raw = raw.trim().trim_matches('"').trim();
    let (scheme, rest) = raw.split_once("://").ok_or_else(|| "That is not a download link.".to_string())?;
    let (body, query) = rest.split_once('?').unwrap_or((rest, ""));
    let body = body.trim_end_matches('/');
    match scheme.to_ascii_lowercase().as_str() {
        "nxm" => parse_nxm(body, query),
        "modl" => parse_modl(body, query),
        _ => Err("That link is not nxm or modl.".into()),
    }
}

fn parse_nxm(body: &str, query: &str) -> Result<Link, String> {
    let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 5 || !parts[1].eq_ignore_ascii_case("mods") || !parts[3].eq_ignore_ascii_case("files") {
        return Err("That Nexus link is not a file download.".into());
    }
    if !is_fnv_nxm(parts[0]) {
        return Err(format!("That Nexus link is for {}, not New Vegas.", parts[0]));
    }
    let mod_id: u64 = parts[2].parse().map_err(|_| "That Nexus link has no mod id.".to_string())?;
    let file_id: u64 = parts[4].parse().map_err(|_| "That Nexus link has no file id.".to_string())?;
    if mod_id == 0 || file_id == 0 {
        return Err("That Nexus link has no file.".into());
    }
    let q = query_map(query);
    let key = q.iter().find(|(k, _)| k.eq_ignore_ascii_case("key")).map(|(_, v)| v.clone()).filter(|v| token_ok(v));
    let expires = q.iter().find(|(k, _)| k.eq_ignore_ascii_case("expires")).map(|(_, v)| v.clone()).filter(|v| token_ok(v));
    Ok(Link::Nxm { mod_id, file_id, key, expires })
}

fn parse_modl(body: &str, query: &str) -> Result<Link, String> {
    let game = body.split('/').find(|s| !s.is_empty()).unwrap_or("");
    if game.is_empty() {
        return Err("That ModPub link has no game.".into());
    }
    if !is_fnv_modl(game) {
        return Err(format!("That ModPub link is for {game}, not New Vegas."));
    }
    let Some(raw) = query_raw(query, "url").filter(|v| !v.is_empty()) else {
        return Err("That ModPub link has no file.".into());
    };
    let decoded = qt_percent_decode(raw)?;
    let url = wire_https(&decoded)?;
    Ok(Link::Modl { url })
}

fn is_fnv_nxm(game: &str) -> bool {
    // Nexus's own id in an nxm link is `newvegas`. The other two show up on older pages and on ModPub.
    game.eq_ignore_ascii_case("newvegas") || game.eq_ignore_ascii_case("falloutnv") || game.eq_ignore_ascii_case("falloutnewvegas")
}

fn is_fnv_modl(game: &str) -> bool {
    // ModPub's id is falloutnv. `other` means whichever instance is open.
    game.eq_ignore_ascii_case("falloutnv") || game.eq_ignore_ascii_case("newvegas") || game.eq_ignore_ascii_case("falloutnewvegas") || game.eq_ignore_ascii_case("other")
}

fn https_download(url: &str) -> Result<(), String> {
    https_host(url).map(|_| ())
}

fn https_host(url: &str) -> Result<String, String> {
    if url.len() < 8 || !url[..8].eq_ignore_ascii_case("https://") {
        return Err("That link has to be https. It was not downloaded.".into());
    }
    if url.chars().any(|c| c.is_control() || c == '"' || c == '\\' || c == '<' || c == '>' || (c.is_whitespace() && c != ' ')) {
        return Err("That download address cannot be used.".into());
    }
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains(' ') {
        return Err("That download address cannot be used.".into());
    }
    let host = split_host(authority)?;
    if !is_public_dns_name(&host) {
        return Err("That download has to be a public site. A local or numeric address is not used.".into());
    }
    Ok(host)
}

fn split_host(authority: &str) -> Result<String, String> {
    if authority.starts_with('[') {
        return Err("That download has to be a public site. A local or numeric address is not used.".into());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if let Some(port) = port {
        let n = port.parse::<u16>().ok().filter(|n| *n > 0);
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|c| c.is_ascii_digit()) || n.is_none() {
            return Err("That download address cannot be used.".into());
        }
    }
    if host.is_empty() {
        return Err("That download address cannot be used.".into());
    }
    Ok(host.to_string())
}

fn is_public_dns_name(host: &str) -> bool {
    if host.len() > 253 || !host.is_ascii() {
        return false;
    }
    let host = host.to_ascii_lowercase();
    const BLOCKED: &[&str] = &[".local", ".localhost", ".internal", ".localdomain", ".home.arpa", ".lan", ".home", ".onion", ".arpa"];
    if host == "localhost" || BLOCKED.iter().any(|suffix| host.ends_with(suffix)) {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let Some(tld) = labels.last() else { return false };
    if labels.len() < 2 || !tld.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    labels.iter().all(|label| {
        let bytes = label.as_bytes();
        !bytes.is_empty() && bytes.len() <= 63 && bytes.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'-') && bytes[0] != b'-' && bytes[bytes.len() - 1] != b'-'
    })
}

/// One percent-decode, the way Qt reads a ModPub `url`.
/// `+` stays a plus. A `%` that is not two hex digits is refused.
/// `rawurlencode` and `encodeURIComponent` both send a space as `%20`.
fn qt_percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("That download address is not encoded right.".into());
            }
            let hex = &s[i + 1..i + 3];
            let Ok(v) = u8::from_str_radix(hex, 16) else {
                return Err("That download address is not encoded right.".into());
            };
            out.push(v);
            i += 3;
            continue;
        }
        if !bytes[i].is_ascii() {
            return Err("That download address is not encoded right.".into());
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).map_err(|_| "That download address is not encoded right.".to_string())
}

fn wire_https(decoded: &str) -> Result<String, String> {
    https_download(decoded)?;
    Ok(encode_wire(decoded))
}

fn encode_wire(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    let cut = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..cut];
    let tail = rest[cut..].split('#').next().unwrap_or("");
    let mut out = format!("https://{authority}");
    if tail.is_empty() {
        return out;
    }
    let (path, query) = match tail.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (tail, None),
    };
    out.push_str(&encode_slashed(path));
    if let Some(query) = query {
        out.push('?');
        out.push_str(&encode_query(query));
    }
    out
}

fn encode_slashed(path: &str) -> String {
    let mut out = String::new();
    for (i, seg) in path.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&encode_component(seg));
    }
    out
}

fn encode_query(query: &str) -> String {
    let mut out = String::new();
    for (i, part) in query.split('&').enumerate() {
        if i > 0 {
            out.push('&');
        }
        match part.split_once('=') {
            Some((k, v)) => {
                out.push_str(&encode_component(k));
                out.push('=');
                out.push_str(&encode_component(v));
            }
            None => out.push_str(&encode_component(part)),
        }
    }
    out
}

/// Characters `encodeURIComponent` leaves as they are, except `+`.
/// A plus is `%2B`, so a server cannot read it as a space.
fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn query_raw<'a>(q: &'a str, name: &str) -> Option<&'a str> {
    q.split('&').find_map(|part| {
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        k.eq_ignore_ascii_case(name).then_some(v)
    })
}

fn redirect_target(current: &str, location: &str) -> Result<String, String> {
    let location = location.trim();
    if location.is_empty() || location.len() > 4000 || location.chars().any(|c| c.is_control() || c == '\\') {
        return Err("That download was sent somewhere that cannot be used.".into());
    }
    let next = if location.len() >= 8 && location[..8].eq_ignore_ascii_case("https://") {
        location.to_string()
    } else if location.len() >= 7 && location[..7].eq_ignore_ascii_case("http://") {
        return Err("That link has to be https. It was not downloaded.".into());
    } else if let Some(rest) = location.strip_prefix("//") {
        format!("https://{rest}")
    } else if let Some(path) = location.strip_prefix('/') {
        format!("{}/{path}", https_origin(current)?)
    } else {
        let base = current.split(['?', '#']).next().unwrap_or(current);
        let Some(i) = base.rfind('/') else {
            return Err("That download was sent somewhere that cannot be used.".into());
        };
        if i < "https://".len() {
            return Err("That download was sent somewhere that cannot be used.".into());
        }
        format!("{}{location}", &base[..=i])
    };
    https_download(&next)?;
    if next.chars().any(|c| !c.is_ascii() || c.is_whitespace()) {
        return Err("That download address cannot be used.".into());
    }
    Ok(next)
}

fn https_origin(url: &str) -> Result<String, String> {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return Err("That download was sent somewhere that cannot be used.".into());
    }
    Ok(format!("https://{authority}"))
}

fn addresses_are_public(url: &str) -> Result<(), String> {
    use std::net::ToSocketAddrs;
    let host = https_host(url)?;
    let mut found = false;
    let addrs = (host.as_str(), 443u16).to_socket_addrs().map_err(|_| format!("{host}: host not found."))?;
    for addr in addrs {
        found = true;
        if !ip_is_public(addr.ip()) {
            return Err("That download was sent to a private or local address. It was not downloaded.".into());
        }
    }
    if !found {
        return Err(format!("{host}: host not found."));
    }
    Ok(())
}

fn ip_is_public(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => ipv4_public(v4),
        std::net::IpAddr::V6(v6) => ipv6_public(v6),
    }
}

fn ipv4_public(v4: std::net::Ipv4Addr) -> bool {
    let o = v4.octets();
    if o[0] == 0 || o[0] == 255 {
        return false;
    }
    if o[0] == 100 && (o[1] & 0xc0) == 0x40 {
        return false;
    }
    if o[0] == 198 && (o[1] & 0xfe) == 18 {
        return false;
    }
    !v4.is_private() && !v4.is_loopback() && !v4.is_link_local() && !v4.is_broadcast() && !v4.is_documentation() && !v4.is_unspecified() && !v4.is_multicast()
}

fn ipv6_public(v6: std::net::Ipv6Addr) -> bool {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return ipv4_public(v4);
    }
    if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || v6.is_unique_local() {
        return false;
    }
    let s = v6.segments();
    if s[..6] == [0, 0, 0, 0, 0, 0] || (s[0] & 0xffc0) == 0xfe80 || (s[0] == 0x2001 && (s[1] == 0 || s[1] == 0x0db8)) || (s[0] == 0x0100 && s[1] == 0 && s[2] == 0 && s[3] == 0) {
        return false;
    }
    if s[0] == 0x2002 {
        return ipv4_public(std::net::Ipv4Addr::new((s[1] >> 8) as u8, s[1] as u8, (s[2] >> 8) as u8, s[2] as u8));
    }
    true
}

/// A decoded `%20` is a space. The request has to send it encoded.
fn wire_url(url: &str) -> String {
    url.replace(' ', "%20")
}

fn query_map(q: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in q.split('&') {
        if part.is_empty() {
            continue;
        }
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        out.push((percent_decode(k), percent_decode(v)));
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Tell the system that this program opens `nxm://` and `modl://` links.
pub fn register() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("This program's path was not found: {e}"))?;
    #[cfg(windows)]
    {
        register_windows(&exe)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = exe;
        Err("Link association on macOS is not set from here yet.".into())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        register_linux(&exe)
    }
}

fn registered_message() -> String {
    let key = fnv_install::EngineHome::user()
        .map(|h| format!("A Nexus link asks for the API key, then seals it in {}.", h.root.join(KEY_FILE).display()))
        .unwrap_or_else(|| "A Nexus link asks for the API key, then seals it in nexus-api.txt in the launcher home.".into());
    format!("This launcher now opens Nexus and ModPub links. The file shows up in Downloads. Install it from there. Nothing is copied into Data. {key} A ModPub link does not use that key.")
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn register_linux(exe: &Path) -> Result<String, String> {
    let text = linux_desktop(&exe.display().to_string())?;
    let dir = applications_dir().ok_or_else(|| "No home folder, so the link handler was not written.".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("nvforever-links.desktop");
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    let nxm = std::process::Command::new("xdg-mime").args(["default", "nvforever-links.desktop", "x-scheme-handler/nxm"]).status();
    let modl = std::process::Command::new("xdg-mime").args(["default", "nvforever-links.desktop", "x-scheme-handler/modl"]).status();
    let _ = std::process::Command::new("update-desktop-database").arg(&dir).status();
    let mime_ok = nxm.as_ref().ok().is_some_and(|s| s.success()) && modl.as_ref().ok().is_some_and(|s| s.success());
    if mime_ok {
        Ok(registered_message())
    } else {
        Ok(format!("Wrote {}. xdg-mime did not finish, so the system may not use it yet. {}", path.display(), registered_message()))
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn applications_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(xdg).join("applications"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/share/applications"))
}

#[cfg(windows)]
fn register_windows(exe: &Path) -> Result<String, String> {
    let command = windows_open_command(&exe.display().to_string())?;
    for scheme in ["nxm", "modl"] {
        let class = format!(r"HKCU\Software\Classes\{scheme}");
        reg(&class, &["/ve", "/d", &format!("URL:{scheme}"), "/f"])?;
        reg(&class, &["/v", "URL Protocol", "/t", "REG_SZ", "/d", "", "/f"])?;
        reg(&format!(r"{class}\shell\open\command"), &["/ve", "/d", &command, "/f"])?;
    }
    Ok(registered_message())
}

#[cfg(windows)]
fn reg(key: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new("reg");
    cmd.arg("add").arg(key);
    cmd.args(args);
    let status = cmd.status().map_err(|e| format!("reg: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("reg could not set {key}.")) }
}

pub(crate) fn linux_desktop(exe: &str) -> Result<String, String> {
    if exe.is_empty() || exe.contains(['"', '\n', '\r', '%']) {
        return Err("The launcher path cannot be registered.".into());
    }
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=NVForever Launcher\nNoDisplay=true\nStartupNotify=false\nExec=\"{exe}\" %u\nMimeType=x-scheme-handler/nxm;x-scheme-handler/modl;\n"
    ))
}

/// The Windows open command. The registrar uses it there; the test checks the quoting here.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_open_command(exe: &str) -> Result<String, String> {
    if exe.is_empty() || exe.contains(['"', '\n', '\r']) {
        return Err("The launcher path cannot be registered.".into());
    }
    Ok(format!("\"{exe}\" \"%1\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nxm_parses_a_new_vegas_file() {
        let link = parse("nxm://newvegas/mods/42507/files/100?key=abc_def&expires=170000&user_id=9").unwrap();
        match link {
            Link::Nxm { mod_id, file_id, key, expires } => {
                assert_eq!(mod_id, 42507);
                assert_eq!(file_id, 100);
                assert_eq!(key.as_deref(), Some("abc_def"));
                assert_eq!(expires.as_deref(), Some("170000"));
            }
            Link::Modl { .. } => panic!("modl"),
        }
        assert!(parse("nxm://falloutnewvegas/mods/1/files/2").is_ok());
        assert!(parse("nxm://falloutnv/mods/1/files/2").is_ok());
        let url = nexus_request(42507, 100, Some("abc def"), Some("170000"));
        assert!(url.starts_with("https://api.nexusmods.com/v1/games/newvegas/mods/42507/files/100/download_link.json?"));
        assert!(url.contains("key=abc%20def"));
        assert!(url.contains("expires=170000"));
        assert!(!url.contains("user_id"));
        assert_eq!(nexus_request(1, 2, None, None), "https://api.nexusmods.com/v1/games/newvegas/mods/1/files/2/download_link.json");
    }

    #[test]
    fn other_games_and_plain_http_are_refused() {
        let err = parse("nxm://skyrim/mods/1/files/2").unwrap_err();
        assert!(err.contains("skyrim") && err.contains("New Vegas"));
        let err = parse("modl://skyrimspecialedition/?url=https%3A%2F%2Fexample.com%2Fa.7z").unwrap_err();
        assert!(err.contains("skyrimspecialedition") && err.contains("New Vegas"));
        assert!(parse("modl://falloutnv/?url=http%3A%2F%2Fexample.com%2Fa.7z").unwrap_err().contains("https"));
        assert!(parse("modl://falloutnv/?url=file%3A%2F%2F%2Fetc%2Fpasswd").is_err());
        assert!(parse("modl://falloutnv/?url=https%3A%2F%2Fuser%3Apass%40example.com%2Fa.7z").is_err());
    }

    #[test]
    fn modl_decodes_the_download_address() {
        let link = parse("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2Fmods%2Fmy%20file.7z").unwrap();
        match link {
            Link::Modl { url } => assert_eq!(url, "https://example.com/mods/my%20file.7z"),
            Link::Nxm { .. } => panic!("nxm"),
        }
        assert!(parse("modl://newvegas/?url=https%3A%2F%2Fexample.com%2Fa.7z").is_ok());
        assert_eq!(wire_url("https://example.com/mods/my file.7z"), "https://example.com/mods/my%20file.7z");
        assert_eq!(file_name_from_url("https://example.com/mods/my%20file.7z"), "my file.7z");
        assert_eq!(file_name_from_url("https://example.com/mods/%2e%2e"), "download.bin");
        assert_eq!(file_name_from_url("https://example.com/CON.7z"), "download.bin");
        assert_eq!(file_name_from_url("https://example.com/file.7z."), "download.bin");
        assert_eq!(disposition_name("attachment; filename=\"Cool Mod.7z\"").as_deref(), Some("Cool Mod.7z"));
    }

    #[test]
    fn modl_matches_the_posted_link() {
        let posted = "modl://falloutnv/?url=https%3A%2F%2Feddoursul.win%2FCyberware%20TTW%20Patch.7z";
        match parse(posted).unwrap() {
            Link::Modl { url } => {
                assert_eq!(url, "https://eddoursul.win/Cyberware%20TTW%20Patch.7z");
                assert_eq!(file_name_from_url(&url), "Cyberware TTW Patch.7z");
            }
            Link::Nxm { .. } => panic!("nxm"),
        }
        // encodeURIComponent leaves parentheses. rawurlencode turns them into %28 %29.
        match parse("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2Ffile(1).7z").unwrap() {
            Link::Modl { url } => assert_eq!(url, "https://example.com/file(1).7z"),
            Link::Nxm { .. } => panic!("nxm"),
        }
        match parse("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2Ffile%281%29.7z").unwrap() {
            Link::Modl { url } => assert_eq!(url, "https://example.com/file(1).7z"),
            Link::Nxm { .. } => panic!("nxm"),
        }
        // Qt does not treat + as a space. Both forms ask for a plus in the name.
        for raw in ["modl://falloutnv/?url=https%3A%2F%2Fexample.com%2Fa+b.7z", "modl://falloutnv/?url=https%3A%2F%2Fexample.com%2Fa%2Bb.7z"] {
            match parse(raw).unwrap() {
                Link::Modl { url } => assert_eq!(url, "https://example.com/a%2Bb.7z"),
                Link::Nxm { .. } => panic!("nxm"),
            }
        }
        match parse("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2F100%2520done.7z").unwrap() {
            Link::Modl { url } => assert_eq!(url, "https://example.com/100%2520done.7z"),
            Link::Nxm { .. } => panic!("nxm"),
        }
        let posted_again = raw_url("https://eddoursul.win/Cyberware TTW Patch.7z");
        assert!(parse(&posted_again).is_ok());
        match parse(&raw_url("https://example.com/Мой.7z")).unwrap() {
            Link::Modl { url } => {
                assert!(url.starts_with("https://example.com/"));
                assert!(url.is_ascii());
                assert_eq!(file_name_from_url(&url), "Мой.7z");
            }
            Link::Nxm { .. } => panic!("nxm"),
        }
        assert!(parse("modl://other/?url=https%3A%2F%2Fexample.com%2Fa.7z").is_ok());
        assert!(parse("modl://OTHER/?url=https://example.com/a.7z").is_ok());
        assert!(parse("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2F%G5").is_err());
        assert!(parse("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2F100%").is_err());
        assert!(parse("modl://falloutnv/").unwrap_err().contains("no file"));
        for blocked in ["https://127.0.0.1/a.7z", "https://10.0.0.8/a.7z", "https://192.168.1.9/a.7z", "https://localhost/a.7z", "https://nas.local/a.7z", "https://[::1]/a.7z", "https://2130706433/a.7z"] {
            assert!(parse(&raw_url(blocked)).is_err(), "{blocked}");
        }
    }

    #[test]
    fn redirects_stay_on_public_https() {
        assert_eq!(redirect_target("https://example.com/mods/a.7z", "https://cdn.example.com/a.7z").unwrap(), "https://cdn.example.com/a.7z");
        assert_eq!(redirect_target("https://example.com/mods/a.7z", "/files/a.7z").unwrap(), "https://example.com/files/a.7z");
        assert_eq!(redirect_target("https://example.com/mods/a.7z", "b.7z").unwrap(), "https://example.com/mods/b.7z");
        assert!(redirect_target("https://example.com/a.7z", "http://example.com/a.7z").unwrap_err().contains("https"));
        assert!(redirect_target("https://example.com/a.7z", "https://127.0.0.1/a.7z").is_err());
        assert!(redirect_target("https://example.com/a.7z", "https://localhost/a.7z").is_err());
        assert!(redirect_target("https://example.com/a.7z", "//169.254.169.254/a").is_err());
    }

    #[test]
    fn private_addresses_are_not_public() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
        assert!(ip_is_public(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(ip_is_public(IpAddr::V6("2606:4700:4700::1111".parse().unwrap())));
        for blocked in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.0.1", "169.254.1.1", "100.64.0.1", "0.0.0.1", "255.255.255.255", "192.0.2.1", "198.18.0.1"] {
            let ip: Ipv4Addr = blocked.parse().unwrap();
            assert!(!ip_is_public(IpAddr::V4(ip)), "{blocked}");
        }
        for blocked in ["::1", "fe80::1", "fc00::1", "2001:db8::1", "::ffff:127.0.0.1"] {
            let ip: Ipv6Addr = blocked.parse().unwrap();
            assert!(!ip_is_public(IpAddr::V6(ip)), "{blocked}");
        }
    }

    fn raw_url(url: &str) -> String {
        let mut out = String::from("modl://falloutnv/?url=");
        for b in url.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }

    #[test]
    fn chacha20_matches_the_rfc_block() {
        let mut key = [0u8; 32];
        for (i, b) in key.iter_mut().enumerate() {
            *b = i as u8;
        }
        let nonce = [0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00];
        let block = chacha20_block(&key, &nonce, 1);
        assert_eq!(&block[..16], &[0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20, 0x71, 0xc4]);
    }

    #[test]
    fn nexus_list_uses_the_first_https_uri() {
        let text = r#"[{"name":"mirror","URI":"https://files.example/mod.7z"}]"#;
        assert_eq!(first_https(text).unwrap(), "https://files.example/mod.7z");
        assert!(first_https(r#"{"message":"no key"}"#).unwrap_err().contains("no key"));
        assert!(first_https(r#"[{"URI":"http://files.example/mod.7z"}]"#).is_err());
    }

    #[test]
    fn api_key_is_one_plain_line() {
        let dir = std::env::temp_dir().join(format!("nvf-links-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(KEY_FILE), "# note\n\nabc-DEF_1\n").unwrap();
        assert!(api_key_from(&dir, None).is_none());
        let gone = saved_key_error(&dir).unwrap();
        assert!(gone.contains("plain text"));
        assert!(!dir.join(KEY_FILE).exists());
        assert_eq!(api_key_from(&dir, Some("from-env")).as_deref(), Some("from-env"));
        std::fs::write(dir.join(KEY_FILE), "NVFKEY1 nope\n00\n").unwrap();
        assert!(saved_key_error(&dir).unwrap().contains("could not be read"));
        assert!(dir.join(KEY_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nexus_statuses_match_the_download_api() {
        assert!(matches!(nexus_stop(401, ""), NexusStop::NeedKey));
        assert!(matches!(nexus_stop(403, r#"{"message":"Provided API key is incorrect."}"#), NexusStop::NeedKey));
        assert!(matches!(nexus_stop(403, r#"{"error":"Missing API Key"}"#), NexusStop::NeedKey));
        match nexus_stop(403, r#"{"message":"You need a download key"}"#) {
            NexusStop::Failed(e) => assert!(e.contains("Mod Manager Download")),
            NexusStop::NeedKey => panic!("the website key is not the personal API key"),
        }
        match nexus_stop(400, r#"{"message":"expired key"}"#) {
            NexusStop::Failed(e) => assert!(e.contains("expiry") && e.contains("expired key")),
            NexusStop::NeedKey => panic!("400"),
        }
        match nexus_stop(410, "") {
            NexusStop::Failed(e) => assert!(e.contains("expired")),
            NexusStop::NeedKey => panic!("410"),
        }
        match nexus_stop(404, "") {
            NexusStop::Failed(e) => assert!(e.contains("no file")),
            NexusStop::NeedKey => panic!("404"),
        }
        match nexus_stop(429, "") {
            NexusStop::Failed(e) => assert!(e.contains("pause")),
            NexusStop::NeedKey => panic!("429"),
        }
        match nexus_stop(500, r#"{"message":"nope"}"#) {
            NexusStop::Failed(e) => assert!(e.contains("500") && e.contains("nope")),
            NexusStop::NeedKey => panic!("500"),
        }
        assert_eq!(rate_note(Some("12"), Some("3")).as_deref(), Some("Daily requests left: 12."));
        assert_eq!(rate_note(None, Some("3")).as_deref(), Some("Hourly requests left: 3."));
        assert!(rate_note(Some("nope"), None).is_none());
    }

    #[test]
    fn save_api_key_writes_one_line() {
        let dir = std::env::temp_dir().join(format!("nvf-links-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save_api_key(&dir, "  abc+DEF/12=  ").unwrap();
        let stored = std::fs::read_to_string(dir.join(KEY_FILE)).unwrap();
        assert!(stored.starts_with("NVFKEY1 "));
        assert!(!stored.contains("abc+DEF/12="));
        assert_eq!(api_key_from(&dir, None).as_deref(), Some("abc+DEF/12="));
        assert_eq!(api_key_from(&dir, Some("from-env")).as_deref(), Some("from-env"));
        assert!(save_api_key(&dir, "not a key").is_err());
        assert!(is_nxm("nxm://newvegas/mods/1/files/2"));
        assert!(is_nxm("nxm://falloutnv/mods/1/files/2"));
        assert!(!is_nxm("nxm://skyrim/mods/1/files/2"));
        assert!(!is_nxm("modl://falloutnv/?url=https%3A%2F%2Fexample.com%2Fa.7z"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn queued_links_come_out_in_order() {
        let dir = std::env::temp_dir().join(format!("nvf-links-queue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        enqueue(&dir, &["nxm://falloutnewvegas/mods/1/files/2".into(), "modl://falloutnv/?url=https://example.com/a.7z".into()]).unwrap();
        assert!(pop(&dir).unwrap().starts_with("nxm://"));
        assert!(pop(&dir).unwrap().starts_with("modl://"));
        assert!(pop(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn association_text_names_both_schemes() {
        let desktop = linux_desktop("/opt/NVForever/nvforever-launcher").unwrap();
        assert!(desktop.contains("x-scheme-handler/nxm;"));
        assert!(desktop.contains("x-scheme-handler/modl;"));
        assert!(desktop.contains("Exec=\"/opt/NVForever/nvforever-launcher\" %u"));
        assert!(linux_desktop("/opt/bad%path").is_err());
        assert_eq!(windows_open_command(r"C:\Games\nvforever-launcher.exe").unwrap(), "\"C:\\Games\\nvforever-launcher.exe\" \"%1\"");
        assert!(windows_open_command("C:\\bad\"path").is_err());
    }

    #[test]
    fn handler_choice_is_one_word() {
        let dir = std::env::temp_dir().join(format!("nvf-links-handler-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(handler_choice(&dir).is_none());
        remember_handler(&dir, false).unwrap();
        assert_eq!(handler_choice(&dir), Some(false));
        assert!(handler_decided(&dir));
        remember_handler(&dir, true).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(HANDLER_FILE)).unwrap(), "yes\n");
        assert_eq!(handler_choice(&dir), Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
