//! Mirror resolvers: from a filehost's link to a file the download manager
//! can fetch.
//!
//! Ported from Union.Manifold's resolvers (MIT, github.com/fyiel/Union.Manifold),
//! which do this for the same hosts kryo.to mirrors to. Two kinds:
//!
//!  * API hosts answer a plain HTTP client with the file's real address:
//!    Pixeldrain, Gofile, Buzzheavier, MediaFire, FuckingFast. Catbox and
//!    FileDitch-style direct links need nothing at all.
//!  * Page hosts (VikingFile, Mocha, 1fichier, ...) only hand the file to a
//!    real browser, often after a check. Their page is opened in a hidden
//!    window of our own, the host's own download button is pressed, and the
//!    download it starts is taken over. If the check wants a person, the
//!    window is shown and waits for them. See `browser.rs`.
//!
//! Whatever comes back carries the headers the file wants (cookies, user
//! agent, referer) and how many connections the host tolerates.

mod browser;
mod buzzheavier;
mod gofile;
mod mediafire;
mod mocha;
mod pixeldrain;
mod simple;

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tauri::{AppHandle, Runtime};

pub use browser::cancel as cancel_browser;

/// The user agent resolvers present, a current desktop Chrome: hosts that
/// sniff for a browser get one, and it is replayed on the download itself.
pub const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// A file a mirror resolved to.
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    pub url: String,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    /// Sent with every request for the file.
    pub headers: HashMap<String, String>,
    /// How many connections the host is happy with.
    pub connections: u32,
}

/// What a mirror link is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A plain HTTP client can resolve it.
    Api,
    /// Only a browser can: a hidden window does it.
    Page,
    /// Already the file.
    Direct,
}

pub(crate) fn host_matches(url: &str, re: &Regex) -> bool {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).is_some_and(|h| re.is_match(&h))
}

fn hostname(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase())).unwrap_or_default()
}

/// Last non-empty path segment, percent-decoded.
pub(crate) fn last_segment(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    u.path_segments()?
        .rfind(|s| !s.is_empty())
        .map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string())
}

/// A JSON number, or a number in a string.
pub(crate) fn num(v: Option<&serde_json::Value>) -> Option<u64> {
    let v = v?;
    let n = v
        .as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()).map(|f| f as u64))?;
    (n != 0).then_some(n)
}

/// Direct file hosts: the link is the file.
static DIRECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(^|\.)(files\.catbox\.moe|litter\.catbox\.moe)$").unwrap());

/// A 0807.st file link: `0807.st/<id>.<ext>`, the file itself. Never its
/// `/d/<token>` link - opening that DELETES the file - nor the `/p/` viewer.
pub(crate) fn st0807_file(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else { return false };
    if !u.host_str().is_some_and(|h| h.eq_ignore_ascii_case("0807.st") || h.eq_ignore_ascii_case("www.0807.st")) {
        return false;
    }
    let segs: Vec<&str> = u.path_segments().map(|s| s.filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    matches!(segs.as_slice(), [one] if one.contains('.') && !one.starts_with('.'))
}

/// Page hosts, by domain, and what stands in the way.
const PAGE_HOSTS: &[(&str, &str)] = &[
    ("vikingfile.com", "VikingFile"),
    ("vik1ngfile.site", "VikingFile"),
    ("mocha.my", "Mocha"),
    ("dropdrive.org", "DropDrive"),
    // DropDrive's API and pages while dropdrive.org is suspended.
    ("dropdrive.qsnetwork.dev", "DropDrive"),
    ("fileditch.com", "FileDitch"),
    ("fileditchfiles.me", "FileDitch"),
    ("fileditchfiles.st", "FileDitch"),
    ("1fichier.com", "1fichier"),
    ("akirabox.com", "AkiraBox"),
    ("qiwi.gg", "Qiwi"),
    ("datanodes.to", "DataNodes"),
    ("krakenfiles.com", "KrakenFiles"),
    ("send.cm", "Send.cm"),
];

fn domain_match(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|rest| rest.ends_with('.'))
}

fn page_host(url: &str) -> Option<&'static str> {
    let host = hostname(url);
    PAGE_HOSTS.iter().find(|(d, _)| domain_match(&host, d)).map(|(_, name)| *name)
}

/// What kind of mirror `url` is, and the host's name. `None` for a link no
/// resolver knows (MEGA's encrypted transfers, torrents, unknown sites).
pub fn classify(url: &str) -> Option<(Kind, &'static str)> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return None;
    }
    if pixeldrain::matches(url) {
        return Some((Kind::Api, "Pixeldrain"));
    }
    if gofile::matches(url) {
        return Some((Kind::Api, "Gofile"));
    }
    if buzzheavier::matches(url) {
        return Some((Kind::Api, "Buzzheavier"));
    }
    if mediafire::matches(url) {
        return Some((Kind::Api, "MediaFire"));
    }
    if simple::fuckingfast_matches(url) {
        return Some((Kind::Api, "FuckingFast"));
    }
    // Our own Mocha shares: dl.kryo.to signs a direct link (mocha.rs). Any
    // other Mocha link is a page host below.
    if mocha::share_token(url).is_some() {
        return Some((Kind::Api, "Mocha"));
    }
    if host_matches(url, &DIRECT_RE) {
        return Some((Kind::Direct, "catbox"));
    }
    if st0807_file(url) {
        return Some((Kind::Direct, "0807.st"));
    }
    page_host(url).map(|name| (Kind::Page, name))
}

/// Resolve a mirror link to its file.
///
/// An API host that fails falls through to the browser, which is how a
/// person would get past whatever the API would not do (a Cloudflare check
/// in front of Buzzheavier, say).
pub async fn resolve<R: Runtime>(app: &AppHandle<R>, url: &str) -> Result<Resolved, String> {
    let (kind, name) = classify(url).ok_or_else(|| format!("Kryoto can't download from {} yet.", hostname(url)))?;
    let started = std::time::Instant::now();
    let result = match kind {
        Kind::Direct => Ok(Resolved {
            url: url.to_string(),
            file_name: last_segment(url),
            headers: HashMap::from([("User-Agent".into(), UA.into())]),
            connections: 8,
            ..Default::default()
        }),
        Kind::Api => {
            let api = if pixeldrain::matches(url) {
                pixeldrain::resolve(url).await
            } else if gofile::matches(url) {
                gofile::resolve(url).await
            } else if buzzheavier::matches(url) {
                buzzheavier::resolve(url).await
            } else if mediafire::matches(url) {
                mediafire::resolve(url).await
            } else if mocha::share_token(url).is_some() {
                mocha::hotlink(url).await
            } else {
                simple::fuckingfast(url).await
            };
            match api {
                Ok(r) => Ok(r),
                Err(e) => {
                    crate::logging::info("resolver", &format!("{name}: {e}; trying its page"));
                    browser::solve(app, url, name).await.map_err(|b| format!("{e}; {b}"))
                }
            }
        }
        Kind::Page => browser::solve(app, url, name).await,
    };
    match &result {
        Ok(r) => crate::logging::info(
            "resolver",
            &format!("{name} resolved in {}ms ({} connections)", started.elapsed().as_millis(), r.connections),
        ),
        Err(e) => crate::logging::error("resolver", &format!("{name}: {e}")),
    }
    result.map_err(|e| format!("{name}: {e}"))
}

/// A client that looks like a browser, with a cookie jar of its own.
pub(crate) fn client(redirects: bool) -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .user_agent(UA)
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(30));
    if !redirects {
        b = b.redirect(reqwest::redirect::Policy::none());
    }
    b.build().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_hosts_kryo_mirrors_to() {
        assert_eq!(classify("https://pixeldrain.com/u/abcd1234").map(|c| c.1), Some("Pixeldrain"));
        assert_eq!(classify("https://gofile.io/d/dc1V9W").map(|c| c.0), Some(Kind::Api));
        assert_eq!(classify("https://buzzheavier.com/AbCd1234").map(|c| c.1), Some("Buzzheavier"));
        assert_eq!(classify("https://vikingfile.com/f/xyz").map(|c| c.0), Some(Kind::Page));
        assert_eq!(classify("https://mocha.my/share/pvpgaoYk-6So_SEvo").map(|c| c.0), Some(Kind::Api));
        assert_eq!(classify("https://mocha.my/share/tok").map(|c| c.0), Some(Kind::Page));
        assert_eq!(classify("https://files.catbox.moe/x.7z").map(|c| c.0), Some(Kind::Direct));
        assert_eq!(classify("https://dropdrive.org/d/YIJaPhMI"), Some((Kind::Page, "DropDrive")));
        assert_eq!(classify("https://dropdrive.qsnetwork.dev/d/YIJaPhMI"), Some((Kind::Page, "DropDrive")));
        assert_eq!(classify("https://0807.st/Ab3dEfG.7z"), Some((Kind::Direct, "0807.st")));
        // 0807's deletion link deletes the file when opened: never a download.
        assert_eq!(classify("https://0807.st/d/deletiontoken"), None);
        assert_eq!(classify("https://0807.st/p/Ab3dEfG"), None);
        assert_eq!(classify("https://mega.nz/file/abc"), None);
        assert_eq!(classify("magnet:?xt=urn:btih:abc"), None);
    }

    #[test]
    fn lookalike_domains_are_not_hosts() {
        assert_eq!(classify("https://vikingfile.com.evil.net/f/x"), None);
        assert_eq!(classify("https://notmocha.my/share/x"), None);
        assert_eq!(classify("https://www.mocha.my/share/x").map(|c| c.1), Some("Mocha"));
    }
}

/// Against the real hosts: `KRYOTO_LIVE_MIRRORS="url url" cargo test --lib live_api -- --ignored --nocapture`.
#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn live_api_hosts_resolve() {
        tauri::async_runtime::block_on(run())
    }

    async fn run() {
        for url in std::env::var("KRYOTO_LIVE_MIRRORS").unwrap_or_default().split_whitespace() {
            let r = if super::pixeldrain::matches(url) {
                super::pixeldrain::resolve(url).await
            } else if super::gofile::matches(url) {
                super::gofile::resolve(url).await
            } else if super::buzzheavier::matches(url) {
                super::buzzheavier::resolve(url).await
            } else {
                super::mediafire::resolve(url).await
            };
            println!("{url} -> {r:?}");
            let r = r.expect("resolves");
            let mut req = reqwest::Client::new().get(&r.url).header("Range", "bytes=0-0");
            for (k, v) in &r.headers {
                req = req.header(k, v);
            }
            let res = req.send().await.expect("file answers");
            println!("  file: {} {:?}", res.status(), res.headers().get("content-range"));
            assert!(res.status().as_u16() == 206 || res.status().as_u16() == 200);
        }
    }
}
