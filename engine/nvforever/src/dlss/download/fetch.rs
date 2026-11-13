//! The archive's bytes by HTTP range requests (Windows: ureq through the system's TLS, as the
//! player reports send, `reports/upload.rs`).
//!
//! [design] GitHub answers a release asset's address with a redirect to a signed address on
//! its asset host, which serves byte ranges (`Accept-Ranges: bytes`, a `206 Partial Content`
//! to a `Range` request [observed: #2250, 2026-11-12]). Redirects are not followed blindly: the
//! first request goes to [`ASSET_URL`] with redirects off, and only a redirect to `https` on
//! [`ALLOWED_HOSTS`] is taken; the ranges are then asked of that address, which is resolved
//! again once when it answers `403` (signed addresses expire). Every answer must be a `206`
//! whose `Content-Range` is the range asked for out of [`ASSET_SIZE`] bytes; a `200` means
//! the server ignored the range ([`Failure::NoRanges`]). No content coding is accepted
//! (`Accept-Encoding: identity`). A connect timeout, a per-read timeout, and for each answer's
//! body a time bound scaled by its size ([`body_time`]: a minute plus 8 KiB a second), so a
//! server trickling bytes cannot hold the download forever while a slow line still finishes.
//!
//! Test aid: `NVF_DLSS_DOWNLOAD_URL=<url>` replaces [`ASSET_URL`] only when it is `http` on
//! this machine (`127.0.0.1`, `localhost`, `[::1]`); anything else is ignored with a warning.
//! The pins still apply: the size, the entries, every hash.

use super::{ASSET_SIZE, Failure};

/// The hosts a redirect from [`super::ASSET_URL`] may lead to: GitHub's release asset hosts
/// [observed: the asset's redirect, 2026-11-12, and GitHub's earlier asset host].
pub(super) const ALLOWED_HOSTS: [&str; 2] = ["release-assets.githubusercontent.com", "objects.githubusercontent.com"];
/// The test aid's variable.
pub(super) const TEST_URL_VAR: &str = "NVF_DLSS_DOWNLOAD_URL";
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// An absolute URL's scheme and host, both lower case (an IPv6 host with its brackets); `None`
/// for anything else, and for an address with user information (`user@host`).
fn scheme_host(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']')?;
        (&authority[..=end], &authority[end + 1..])
    } else {
        let host = authority.split(':').next().unwrap_or_default();
        (host, &authority[host.len()..])
    };
    let port_ok = port.is_empty() || port.strip_prefix(':').is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    (port_ok && !host.is_empty()).then(|| (scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
}

/// Whether `url` is `http` on this machine (the test aid's only allowed form).
pub(super) fn is_loopback_http(url: &str) -> bool {
    scheme_host(url).is_some_and(|(scheme, host)| scheme == "http" && LOOPBACK_HOSTS.contains(&host.as_str()))
}

/// Whether a redirect to `location` may be taken: `https` on [`ALLOWED_HOSTS`], or, while the
/// test aid is in use, `http` on this machine.
pub(super) fn redirect_allowed(location: &str, test_aid: bool) -> bool {
    scheme_host(location).is_some_and(|(scheme, host)| scheme == "https" && ALLOWED_HOSTS.contains(&host.as_str())) || (test_aid && is_loopback_http(location))
}

/// A `Content-Range` value `bytes <first>-<last>/<total>` read as the three numbers.
pub(super) fn content_range(value: &str) -> Option<(u64, u64, u64)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let (first, last) = range.split_once('-')?;
    Some((first.trim().parse().ok()?, last.trim().parse().ok()?, total.trim().parse().ok()?))
}

/// A `206` answer's `Content-Range` checked against the range asked for: the total must be
/// [`ASSET_SIZE`], the range the one asked for.
pub(super) fn check_range(value: Option<&str>, start: u64, len: u64) -> Result<(), Failure> {
    let (first, last, total) = value.and_then(content_range).ok_or(Failure::NoRanges)?;
    if total != ASSET_SIZE {
        return Err(Failure::WrongSize(total));
    }
    if len == 0 || first != start || last != start + len - 1 {
        return Err(Failure::NoRanges);
    }
    Ok(())
}

/// The longest an answer's body of `len` bytes may take [design]: a minute, plus a second for
/// each 8 KiB (a 30 MB range gets about an hour: only a line slower than 64 kbit/s fails).
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn body_time(len: u64) -> std::time::Duration {
    std::time::Duration::from_secs(60 + len / (8 * 1024))
}

#[cfg(windows)]
pub(super) use http::Http;

#[cfg(windows)]
mod http {
    use std::io::Read;
    use std::time::Duration;

    use super::super::{ASSET_SIZE, ASSET_URL, Failure, RangeSource};
    use super::{TEST_URL_VAR, check_range, is_loopback_http, redirect_allowed};

    /// How long connecting (and resolving the name) may take.
    const CONNECT: Duration = Duration::from_secs(20);
    /// How long the answer's headers, and each read of its body, may take.
    const READ: Duration = Duration::from_secs(30);

    /// The archive over HTTP. Visible to the download module only: nothing else reaches the
    /// network through it, and the download starts only with a `LicenceAccepted`.
    pub(in crate::dlss::download) struct Http {
        agent: ureq::Agent,
        /// [`ASSET_URL`], or the test aid's address.
        start_url: String,
        test_aid: bool,
        /// The address the ranges are asked of, once known.
        resolved: Option<String>,
    }

    impl Http {
        /// The pinned address, or the test aid's when it is set and allowed.
        pub(in crate::dlss::download) fn new() -> Self {
            match std::env::var(TEST_URL_VAR) {
                Ok(url) if !url.is_empty() && is_loopback_http(&url) => Self::with_url(url, true),
                Ok(url) if !url.is_empty() => {
                    bevy::log::warn!("DLSS download: {TEST_URL_VAR}={url} is not an http address on this machine (127.0.0.1, localhost, [::1]); ignored");
                    Self::with_url(ASSET_URL.to_string(), false)
                }
                _ => Self::with_url(ASSET_URL.to_string(), false),
            }
        }

        pub(in crate::dlss::download) fn with_url(start_url: String, test_aid: bool) -> Self {
            let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
            let agent = ureq::Agent::config_builder()
                .tls_config(tls)
                .http_status_as_error(false)
                .max_redirects(0)
                .accept_encoding("identity")
                .timeout_resolve(Some(CONNECT))
                .timeout_connect(Some(CONNECT))
                .timeout_recv_response(Some(READ))
                .timeout_per_read(Some(READ))
                .user_agent(concat!("NVForever/", env!("CARGO_PKG_VERSION")))
                .build()
                .into();
            Self { agent, start_url, test_aid, resolved: None }
        }

        /// A `GET` of `len` bytes from `start`.
        fn get(&self, url: &str, start: u64, len: u64) -> Result<ureq::http::Response<ureq::Body>, Failure> {
            let last = start + len.max(1) - 1;
            self.agent
                .get(url)
                .header("Range", format!("bytes={start}-{last}"))
                .config()
                .timeout_recv_body(Some(super::body_time(len)))
                .build()
                .call()
                .map_err(|e| Failure::Offline(e.to_string()))
        }

        /// The address the ranges are asked of: the start address's redirect, or the start
        /// address itself when it serves ranges.
        fn resolve(&mut self) -> Result<String, Failure> {
            let answer = self.get(&self.start_url, 0, 1)?;
            let status = answer.status().as_u16();
            let url = match status {
                206 => self.start_url.clone(),
                301 | 302 | 303 | 307 | 308 => {
                    let location = answer.headers().get("location").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
                    if !redirect_allowed(&location, self.test_aid) {
                        return Err(Failure::Redirect(if location.is_empty() { "nowhere".into() } else { location }));
                    }
                    location
                }
                200 => return Err(Failure::NoRanges),
                other => return Err(Failure::Http(other)),
            };
            self.resolved = Some(url.clone());
            Ok(url)
        }
    }

    impl RangeSource for Http {
        fn describe(&self) -> String {
            self.start_url.clone()
        }

        fn size(&mut self) -> Result<u64, Failure> {
            let mut first = self.range(0, 1)?;
            let mut byte = [0u8; 1];
            first.read_exact(&mut byte).map_err(|e| Failure::Offline(e.to_string()))?;
            Ok(ASSET_SIZE)
        }

        fn range(&mut self, start: u64, len: u64) -> Result<Box<dyn Read + '_>, Failure> {
            let mut url = match self.resolved.clone() {
                Some(url) => url,
                None => self.resolve()?,
            };
            let mut resolved_again = false;
            loop {
                let answer = self.get(&url, start, len)?;
                match answer.status().as_u16() {
                    206 => {
                        check_range(answer.headers().get("content-range").and_then(|v| v.to_str().ok()), start, len)?;
                        if answer.headers().get("content-encoding").and_then(|v| v.to_str().ok()).is_some_and(|v| !v.eq_ignore_ascii_case("identity")) {
                            return Err(Failure::NoRanges);
                        }
                        return Ok(Box::new(answer.into_body().into_reader()));
                    }
                    403 if !resolved_again && url != self.start_url => {
                        resolved_again = true;
                        self.resolved = None;
                        url = self.resolve()?;
                    }
                    200 => return Err(Failure::NoRanges),
                    other => return Err(Failure::Http(other)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_on_the_asset_hosts_is_followed() {
        let signed = "https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sp=r&sig=x";
        assert!(redirect_allowed(signed, false));
        assert!(redirect_allowed("HTTPS://Objects.GitHubUserContent.com/x", false));
        assert!(redirect_allowed("https://release-assets.githubusercontent.com:443/x", false));
        for refused in [
            "http://release-assets.githubusercontent.com/x",
            "https://release-assets.githubusercontent.com.example.com/x",
            "https://evil.example/release-assets.githubusercontent.com",
            "https://user@release-assets.githubusercontent.com/x",
            "https://release-assets.githubusercontent.com:x/y",
            "/relative/path",
            "",
            "http://127.0.0.1:8080/x.zip",
        ] {
            assert!(!redirect_allowed(refused, false), "{refused}");
        }
        assert!(redirect_allowed("http://127.0.0.1:8080/x.zip", true), "the test aid's own machine");
        assert!(!redirect_allowed("http://example.com/x.zip", true));
    }

    #[test]
    fn the_test_aid_takes_only_http_on_this_machine() {
        for allowed in ["http://127.0.0.1:9/x.zip", "http://localhost:8787/streamline-sdk-v2.14.16.zip", "http://[::1]:80/a", "http://LOCALHOST/a"] {
            assert!(is_loopback_http(allowed), "{allowed}");
        }
        for refused in [
            "https://127.0.0.1/x.zip",
            "http://127.0.0.2/x.zip",
            "http://localhost.example.com/x",
            "http://127.0.0.1@example.com/x",
            "http://example.com/?h=127.0.0.1",
            "ftp://localhost/x",
            "http://[::1/x",
            "127.0.0.1:9/x.zip",
        ] {
            assert!(!is_loopback_http(refused), "{refused}");
        }
    }

    #[test]
    fn every_answer_must_be_the_range_asked_of_the_pinned_size() {
        assert_eq!(content_range("bytes 0-0/276016170"), Some((0, 0, 276_016_170)));
        assert_eq!(check_range(Some("bytes 0-0/276016170"), 0, 1), Ok(()));
        assert_eq!(check_range(Some("bytes 100-199/276016170"), 100, 100), Ok(()));
        assert_eq!(check_range(Some("bytes 100-199/276016171"), 100, 100), Err(Failure::WrongSize(276_016_171)));
        assert_eq!(check_range(Some("bytes 0-199/276016170"), 100, 100), Err(Failure::NoRanges));
        assert_eq!(check_range(Some("bytes */276016170"), 100, 100), Err(Failure::NoRanges));
        assert_eq!(check_range(None, 0, 1), Err(Failure::NoRanges));
    }

    #[test]
    fn a_body_may_take_a_minute_plus_a_second_per_8_kib() {
        assert_eq!(body_time(0).as_secs(), 60);
        assert_eq!(body_time(65_577).as_secs(), 68);
        assert_eq!(body_time(64 << 20).as_secs(), 60 + 8192);
    }
}
