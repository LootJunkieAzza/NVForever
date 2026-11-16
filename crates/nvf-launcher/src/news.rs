//! The Home page's news posts. Read from the site's `news.json` (a list of posts written
//! for the launcher) and, when the site has none, from this repository's GitHub releases
//! (each release's title, date and notes). The last list fetched is kept in the home's
//! cache folder, so the page shows it at once and offline. Nothing is sent but the
//! requests themselves.
//!
//! `news.json`, as the site serves it:
//!
//! ```json
//! { "posts": [ { "title": "NVForever 0.7.0", "date": "2026-11-11", "author": "LootJunkieAzza",
//!                "image": "https://nv-forever.com/assets/img/news/070.jpg",
//!                "body": "Markdown…", "url": "https://…" } ] }
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SITE_NEWS: &str = "https://nv-forever.com/news.json";
pub const RELEASES: &str = "https://api.github.com/repos/LootJunkieAzza/NVForever/releases?per_page=10";
pub const SITE: &str = "https://nv-forever.com";
const CACHE_FILE: &str = "launcher-news.json";
/// The longest a news image may be; a bigger one is left out.
const MAX_IMAGE: u64 = 8 << 20;
/// The most posts kept from a fetch (and so the most images fetched at once).
const MAX_POSTS: usize = 20;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub title: String,
    /// `YYYY-MM-DD` (or a full ISO time; only the date is shown).
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub url: Option<String>,
    /// The image's bytes, once fetched; never cached.
    #[serde(skip)]
    pub image_bytes: Option<Vec<u8>>,
}

impl Post {
    /// The date as the page shows it: `13 November 2026`, or the text as given when it is
    /// not a date.
    pub fn shown_date(&self) -> String {
        let d = self.date.get(..10).unwrap_or(&self.date);
        let mut it = d.split('-').map(|x| x.parse::<u32>().ok());
        match (it.next().flatten(), it.next().flatten(), it.next().flatten()) {
            (Some(y), Some(m @ 1..=12), Some(day)) => {
                const MONTHS: [&str; 12] =
                    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
                format!("{day} {} {y}", MONTHS[m as usize - 1])
            }
            _ => self.date.clone(),
        }
    }
}

#[derive(Deserialize)]
struct SiteNews {
    posts: Vec<Post>,
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    name: Option<String>,
    tag_name: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    author: Option<Author>,
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

/// The site's posts in `text`.
pub fn parse_site(text: &str) -> Result<Vec<Post>, String> {
    serde_json::from_str::<SiteNews>(text).map(|n| n.posts).map_err(|e| format!("news.json: {e}"))
}

/// GitHub's release list in `text` as posts, drafts left out.
pub fn parse_releases(text: &str) -> Result<Vec<Post>, String> {
    let v: Vec<Release> = serde_json::from_str(text).map_err(|e| format!("releases: {e}"))?;
    Ok(v.into_iter()
        .filter(|r| !r.draft)
        .map(|r| Post {
            title: r.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
            date: r.published_at.unwrap_or_default(),
            author: r.author.map(|a| a.login).unwrap_or_default(),
            image: None,
            body: r.body.unwrap_or_default().replace("\r\n", "\n"),
            url: r.html_url,
            image_bytes: None,
        })
        .collect())
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(concat!("NVForever-Launcher/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut r = agent.get(url).header("Accept", "application/json").call().map_err(|e| format!("{url}: {e}"))?;
    r.body_mut().read_to_string().map_err(|e| format!("{url}: {e}"))
}

/// The release list, fetched at most once a minute: the news (when the site has none) and
/// the update check both read it, and GitHub allows 60 requests an hour without a sign-in.
fn release_list(agent: &ureq::Agent) -> Result<String, String> {
    use std::sync::Mutex;
    use std::time::Instant;
    static LAST: Mutex<Option<(Instant, String)>> = Mutex::new(None);
    // Held across the request, so a second caller waits for the first one's answer.
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, text)) = last.as_ref() {
        if at.elapsed() < Duration::from_secs(60) {
            return Ok(text.clone());
        }
    }
    let text = get_text(agent, RELEASES)?;
    *last = Some((Instant::now(), text.clone()));
    Ok(text)
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> Option<Vec<u8>> {
    let mut r = agent.get(url).call().ok()?;
    r.body_mut().with_config().limit(MAX_IMAGE).read_to_vec().ok()
}

/// Where the news came from, for the page's small print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Site,
    Releases,
    Cache,
}

/// The site's posts, else the releases, each with its image when it has one.
pub fn fetch() -> Result<(Vec<Post>, Source), String> {
    let agent = agent();
    let (mut posts, source) = match get_text(&agent, SITE_NEWS).and_then(|t| parse_site(&t)) {
        Ok(p) if !p.is_empty() => (p, Source::Site),
        site => {
            let site_err = site.err().unwrap_or_else(|| "news.json: no posts".into());
            match release_list(&agent).and_then(|t| parse_releases(&t)) {
                Ok(p) => (p, Source::Releases),
                Err(e) => return Err(format!("{site_err}; {e}")),
            }
        }
    };
    posts.truncate(MAX_POSTS);
    std::thread::scope(|s| {
        for p in &mut posts {
            if let Some(url) = p.image.clone().filter(|u| u.starts_with("https://")) {
                let agent = &agent;
                s.spawn(move || p.image_bytes = get_bytes(agent, &url));
            }
        }
    });
    Ok((posts, source))
}

pub fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(CACHE_FILE)
}

pub fn load_cache(cache_dir: &Path) -> Vec<Post> {
    std::fs::read_to_string(cache_path(cache_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_cache(cache_dir: &Path, posts: &[Post]) {
    if std::fs::create_dir_all(cache_dir).is_ok() {
        if let Ok(t) = serde_json::to_string(posts) {
            let _ = std::fs::write(cache_path(cache_dir), t);
        }
    }
}

/// The site's settings script, whose `nextRelease` drives its countdown bar.
pub const SITE_CONFIG: &str = "https://nv-forever.com/assets/js/config.js";

/// The site's next release: its version and, when the date is set, the moment (seconds since
/// the Unix epoch).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextRelease {
    pub version: String,
    pub at: Option<i64>,
}

/// The quoted value of `name: "…"` (or `'…'`) in `block`.
fn js_field(block: &str, name: &str) -> Option<String> {
    let mut rest = block;
    loop {
        let i = rest.find(name)?;
        rest = &rest[i + name.len()..];
        if let Some(v) = rest.trim_start().strip_prefix(':') {
            let v = v.trim_start();
            let q = v.chars().next().filter(|c| matches!(c, '"' | '\''))?;
            let v = &v[1..];
            return Some(v[..v.find(q)?].to_string());
        }
    }
}

/// `nextRelease: { version: "0.8.0", date: "2026-11-18T20:00:00Z" }` of the site's
/// `config.js`; `None` when there is no such block or its version is empty (the site hides
/// its bar then too).
pub fn parse_next_release(js: &str) -> Option<NextRelease> {
    let block = &js[js.find("nextRelease")?..];
    let block = &block[..block.find('}')?];
    let version = js_field(block, "version").filter(|v| !v.trim().is_empty())?;
    let at = js_field(block, "date").and_then(|d| parse_iso(&d));
    Some(NextRelease { version, at })
}

/// `YYYY-MM-DDTHH:MM[:SS][Z|±HH:MM]` as seconds since the Unix epoch (no zone: UTC).
pub fn parse_iso(t: &str) -> Option<i64> {
    let t = t.trim();
    let num = |a: usize, b: usize| t.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi) = (num(11, 13).unwrap_or(0), num(14, 16).unwrap_or(0));
    let sec = if t.get(16..17) == Some(":") { num(17, 19).unwrap_or(0) } else { 0 };
    let zone = t.get(16..).unwrap_or("");
    let offset = match zone.rfind(['+', '-']) {
        Some(i) if zone.len() - i == 6 => {
            let z = &zone[i..];
            let m = z[1..3].parse::<i64>().ok()? * 60 + z[4..6].parse::<i64>().ok()?;
            if z.starts_with('-') {
                -m
            } else {
                m
            }
        }
        _ => 0,
    };
    // Days from the civil date (Howard Hinnant's days_from_civil).
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + sec - offset * 60)
}

/// The countdown's words for `left` seconds: `2d 14h 03m 12s`.
pub fn countdown_text(left: i64) -> String {
    let l = left.max(0);
    let (d, h, m, s) = (l / 86_400, l % 86_400 / 3600, l % 3600 / 60, l % 60);
    if d > 0 {
        format!("{d}d {h:02}h {m:02}m {s:02}s")
    } else {
        format!("{h:02}h {m:02}m {s:02}s")
    }
}


/// The newest published release on GitHub: its version (`0.8.0`, the tag without its `v`)
/// and its page, where the archives are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Latest {
    pub version: String,
    pub url: String,
}

pub fn parse_latest(text: &str) -> Result<Latest, String> {
    #[derive(Deserialize)]
    struct R {
        tag_name: String,
        html_url: String,
    }
    let r: R = serde_json::from_str(text).map_err(|e| format!("latest release: {e}"))?;
    Ok(Latest { version: r.tag_name.trim_start_matches('v').to_string(), url: r.html_url })
}

/// Whether version `a` (`0.8.0`, `0.10.1`) is later than `b`, part by part as numbers; a
/// part that is not a number (a pre-release suffix) counts as 0.
pub fn is_newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse().unwrap_or(0)).collect() };
    let (a, b) = (parts(a), parts(b));
    let n = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    (0..n).map(|i| at(&a, i).cmp(&at(&b, i))).find(|o| o.is_ne()) == Some(std::cmp::Ordering::Greater)
}

/// The newest release, when GitHub answers.
/// From the release list, not `releases/latest`: that one leaves pre-releases out, and every
/// NVForever release so far is one (it answers 404).
pub fn fetch_latest() -> Result<Option<Latest>, String> {
    let text = release_list(&agent())?;
    let v: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|e| format!("releases: {e}"))?;
    let newest = v.into_iter().find(|r| r["draft"] != serde_json::Value::Bool(true));
    newest.map(|r| parse_latest(&r.to_string())).transpose()
}

/// The site's next release, if it announces one.
pub fn fetch_next_release() -> Result<Option<NextRelease>, String> {
    let js = get_text(&agent(), SITE_CONFIG)?;
    Ok(parse_next_release(&js))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_release_from_the_sites_config() {
        let js = "window.NVF_CONFIG = {\n  version: \"0.7.0\",\n  nextRelease: {\n    version: \"0.8.0\",\n    date: \"2026-11-18T20:00:00Z\"\n  },\n  links: {}\n};";
        let n = parse_next_release(js).unwrap();
        assert_eq!(n.version, "0.8.0");
        assert_eq!(n.at, parse_iso("2026-11-18T20:00:00Z"));
        assert_eq!(parse_next_release("nextRelease: { version: \"\", date: \"\" }"), None);
        assert_eq!(parse_next_release("nextRelease: { version: '0.9.0', date: '' }"), Some(NextRelease { version: "0.9.0".into(), at: None }));
    }

    #[test]
    fn newer_versions() {
        assert!(is_newer("0.8.0", "0.7.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("0.7.1", "0.7"));
        assert!(!is_newer("0.7.0", "0.7.0"));
        assert!(!is_newer("0.6.9", "0.7.0"));
        let l = parse_latest(r#"{"tag_name":"v0.8.0","html_url":"https://x/v0.8.0","assets":[]}"#).unwrap();
        assert_eq!(l, Latest { version: "0.8.0".into(), url: "https://x/v0.8.0".into() });
    }

    #[test]
    fn iso_times() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2000-03-01T00:00Z"), Some(951_868_800));
        assert_eq!(parse_iso("2026-11-18T21:00:00+01:00"), parse_iso("2026-11-18T20:00:00Z"));
        assert_eq!(parse_iso("nope"), None);
        assert_eq!(countdown_text(2 * 86_400 + 14 * 3600 + 3 * 60 + 12), "2d 14h 03m 12s");
        assert_eq!(countdown_text(-5), "00h 00m 00s");
    }

    #[test]
    fn site_posts() {
        let p = parse_site(r#"{"posts":[{"title":"A","date":"2026-11-13","body":"*hi*","image":"https://x/y.jpg"},{"title":"B"}]}"#).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].shown_date(), "13 November 2026");
        assert_eq!(p[1].shown_date(), "");
        assert!(parse_site("[]").is_err());
    }

    #[test]
    fn releases_as_posts() {
        let p = parse_releases(
            r#"[{"name":"","tag_name":"v0.7.0","published_at":"2026-11-11T10:00:00Z","body":"a\r\nb","html_url":"u","author":{"login":"L"}},
                {"name":"x","tag_name":"v9","draft":true}]"#,
        )
        .unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].title.as_str(), p[0].body.as_str(), p[0].author.as_str()), ("v0.7.0", "a\nb", "L"));
        assert_eq!(p[0].shown_date(), "11 November 2026");
    }

    #[test]
    fn cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("nvf-launcher-news-{}", std::process::id()));
        let posts = vec![Post { title: "T".into(), body: "B".into(), image_bytes: Some(vec![1]), ..Post::default() }];
        save_cache(&dir, &posts);
        let back = load_cache(&dir);
        assert_eq!(back[0].title, "T");
        assert_eq!(back[0].image_bytes, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
