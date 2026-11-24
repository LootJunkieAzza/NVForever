//! The CO-OP page's "Allow co-op through Windows Firewall" (owner via parity 2026-11-23: "launcher
//! button that removes the pop up would be good"). Online play goes through Steam's relays
//! (`nvf-coop` steam.rs), outbound, with no rule needed; a direct LAN game without Steam needs
//! other players' packets let in;
//! Windows Firewall asks once per program path, and on a network marked Public that prompt is easy
//! to miss. The button asks Windows to run one elevated PowerShell command, so Windows shows its
//! own UAC prompt and the player agrees there: the old rules named [`RULE`] are removed (a moved
//! install would otherwise leave them behind), then inbound allow rules for UDP and TCP are added
//! for the installed co-op exe's exact path, on the Private and Public profiles. The launcher never
//! changes the firewall itself; the status line only reads the rules.

#![cfg_attr(not(windows), allow(dead_code))]

use std::path::Path;

/// The rules' display name.
pub const RULE: &str = "NVForever Co-Op";

/// A PowerShell single-quoted string literal of `s` (a quote inside is doubled).
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The command the elevated PowerShell runs: the old rules of that name removed, then the UDP and
/// TCP inbound allow rules for `exe` on the Private and Public profiles.
pub fn allow_script(exe: &Path) -> String {
    let name = quoted(RULE);
    let program = quoted(&exe.display().to_string());
    let mut s = format!("Remove-NetFirewallRule -DisplayName {name} -ErrorAction SilentlyContinue");
    for protocol in ["UDP", "TCP"] {
        s += &format!("; New-NetFirewallRule -DisplayName {name} -Direction Inbound -Action Allow -Program {program} -Protocol {protocol} -Profile Private,Public | Out-Null");
    }
    s
}

/// PowerShell's `-EncodedCommand` form of a script: UTF-16LE, then base64.
fn encoded(script: &str) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The arguments for the (unelevated) PowerShell the launcher starts: it asks Windows to run the
/// rule script elevated (`Start-Process -Verb RunAs`, so the UAC prompt is Windows' own) and waits
/// for it, so the status can be read again after.
pub fn elevate_args(exe: &Path) -> Vec<String> {
    let inner = format!("-NoProfile -NonInteractive -EncodedCommand {}", encoded(&allow_script(exe)));
    let outer = format!("Start-Process -FilePath powershell.exe -Verb RunAs -WindowStyle Hidden -Wait -ArgumentList {}", quoted(&inner));
    vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), outer]
}

/// The read-only query for the status line: the program paths of the rules named [`RULE`], one a line.
pub fn query_args() -> Vec<String> {
    let q = format!("Get-NetFirewallRule -DisplayName {} -ErrorAction SilentlyContinue | Where-Object {{ $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' -and $_.Action -eq 'Allow' }} | Get-NetFirewallApplicationFilter | ForEach-Object {{ $_.Program }}", quoted(RULE));
    vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), q]
}

/// Whether the query's output names `exe` (Windows paths: case does not matter, nor `/` for `\`).
pub fn covers(output: &str, exe: &Path) -> bool {
    let norm = |s: &str| s.trim().replace('/', "\\").to_lowercase();
    let want = norm(&exe.display().to_string());
    output.lines().any(|l| norm(l) == want)
}

/// The status line's words.
pub fn status_text(allowed: Option<bool>) -> &'static str {
    match allowed {
        Some(true) => "Firewall: allowed",
        // Online play goes through Steam's relays (nvf-coop steam.rs), which need no rule.
        Some(false) => "Firewall: not set (LAN players without Steam may not be able to join you)",
        None => "Firewall: checking…",
    }
}

#[cfg(windows)]
fn powershell(args: &[String]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("powershell.exe").args(args).creation_flags(CREATE_NO_WINDOW).output()
}

/// Whether the rules let `exe` in (`None`: could not tell). Reads only.
#[cfg(windows)]
pub fn check(exe: &Path) -> Option<bool> {
    let out = powershell(&query_args()).ok()?;
    Some(covers(&String::from_utf8_lossy(&out.stdout), exe))
}

/// Asks Windows to add the rules (its UAC prompt), waits, then reads the rules again.
#[cfg(windows)]
pub fn allow(exe: &Path) -> Option<bool> {
    let _ = powershell(&elevate_args(exe));
    check(exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn decode(b64: &str) -> String {
        const ABC: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = Vec::new();
        for c in b64.chars().filter(|c| *c != '=') {
            let v = ABC.find(c).unwrap() as u32;
            bits.extend((0..6).rev().map(|i| (v >> i) & 1));
        }
        let bytes: Vec<u8> = bits.chunks(8).filter(|c| c.len() == 8).map(|c| c.iter().fold(0u8, |a, b| (a << 1) | *b as u8)).collect();
        let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn the_script_removes_the_old_rules_then_allows_both_protocols_on_both_profiles() {
        let exe = PathBuf::from(r"D:\Games\NVForever\nvforever-coop.exe");
        let s = allow_script(&exe);
        assert!(s.starts_with("Remove-NetFirewallRule -DisplayName 'NVForever Co-Op' -ErrorAction SilentlyContinue; "));
        for p in ["UDP", "TCP"] {
            assert!(s.contains(&format!("New-NetFirewallRule -DisplayName 'NVForever Co-Op' -Direction Inbound -Action Allow -Program 'D:\\Games\\NVForever\\nvforever-coop.exe' -Protocol {p} -Profile Private,Public")), "{s}");
        }
        assert!(s.find("Remove-").unwrap() < s.find("New-").unwrap());
    }

    #[test]
    fn a_quote_in_the_install_path_is_escaped() {
        let s = allow_script(Path::new(r"C:\Users\O'Neil\NVForever\nvforever-coop.exe"));
        assert!(s.contains(r"-Program 'C:\Users\O''Neil\NVForever\nvforever-coop.exe'"), "{s}");
    }

    #[test]
    fn the_elevated_command_is_windows_own_runas_and_decodes_to_the_script() {
        let exe = PathBuf::from(r"C:\Program Files\NVForever 0.8.1\nvforever-coop.exe");
        let a = elevate_args(&exe);
        assert_eq!(&a[..3], ["-NoProfile", "-NonInteractive", "-Command"]);
        assert!(a[3].starts_with("Start-Process -FilePath powershell.exe -Verb RunAs -WindowStyle Hidden -Wait -ArgumentList '-NoProfile -NonInteractive -EncodedCommand "));
        let b64 = a[3].rsplit(' ').next().unwrap().trim_end_matches('\'');
        assert_eq!(decode(b64), allow_script(&exe));
    }

    #[test]
    fn base64_pads_as_powershell_expects() {
        // "A" is 41 00 in UTF-16LE: QQA=; "AB" is 41 00 42 00: QQBCAA==.
        assert_eq!(encoded("A"), "QQA=");
        assert_eq!(encoded("AB"), "QQBCAA==");
    }

    #[test]
    fn the_query_only_reads_the_named_inbound_allow_rules() {
        let q = query_args().join(" ");
        assert!(q.contains("Get-NetFirewallRule -DisplayName 'NVForever Co-Op'"));
        assert!(!q.contains("New-NetFirewallRule") && !q.contains("Remove-NetFirewallRule") && !q.contains("Set-NetFirewall"));
    }

    #[test]
    fn the_status_is_allowed_only_for_this_exes_path() {
        let exe = PathBuf::from(r"D:\Games\NVForever\nvforever-coop.exe");
        assert!(covers("d:\\games\\nvforever\\NVFOREVER-COOP.EXE\r\nD:\\Games\\NVForever\\nvforever-coop.exe\r\n", &exe));
        assert!(covers("D:/Games/NVForever/nvforever-coop.exe\n", &exe));
        assert!(!covers("C:\\Old\\NVForever\\nvforever-coop.exe\n", &exe), "a moved install's old rule does not count");
        assert!(!covers("", &exe));
        assert_eq!(status_text(Some(true)), "Firewall: allowed");
        assert_eq!(status_text(Some(false)), "Firewall: not set (LAN players without Steam may not be able to join you)");
    }
}
