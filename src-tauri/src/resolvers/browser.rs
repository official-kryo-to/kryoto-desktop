//! The page solver: a host that only gives its file to a real browser gets
//! one. Ported from Union.Manifold's `resolver.rs` (MIT).
//!
//! The host's page opens in a hidden window of our own. A probe reports the
//! page's state through its title (the one channel a page with no bridge to
//! us has), the host's own download button is pressed every few seconds
//! until the page starts a download, and that download is cancelled in the
//! window and handed to the download manager with the page's cookies and
//! user agent. Nothing about the host's check is skipped: when the page shows
//! a check that wants a person, or nothing has happened after a while, the
//! window is shown and waits for them.

use super::Resolved;
use base64::Engine as _;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::webview::DownloadEvent;
use tauri::{AppHandle, Emitter, Manager, Runtime, Url, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "resolver";
const TICK: Duration = Duration::from_millis(500);
/// How long the page is left to itself before it is shown.
const HIDDEN_FOR: Duration = Duration::from_secs(15);
/// How long a check that wants a click stays hidden (it may pass by itself).
const INTERACTIVE_GRACE: Duration = Duration::from_secs(6);
/// All of it, including time a person spends on a captcha.
const BUDGET: Duration = Duration::from_secs(180);
const PRESS_EVERY: Duration = Duration::from_secs(4);
const PROBE_MARK: &str = "\u{200b}\u{e00d}KRY:";

/// One page at a time: a second resolve waits for the first.
static SLOT: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[derive(Default)]
struct Shared {
    captured: Mutex<Option<(String, Option<String>)>>,
    title: Mutex<Option<String>>,
    cancelled: std::sync::atomic::AtomicBool,
}

static ACTIVE: Mutex<Option<Arc<Shared>>> = Mutex::new(None);

/// Stop the page being solved, if there is one (the person closed it, or
/// cancelled the download).
pub fn cancel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(s) = ACTIVE.lock().ok().and_then(|a| a.clone()) {
        s.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct Probe {
    #[serde(default)]
    r: String,
    #[serde(default)]
    u: String,
    /// A captcha or Turnstile widget is on the page and not passed yet. A
    /// passed one fills in its response field; DropDrive leaves the widget on
    /// the page afterwards, and counting it as waiting held its Download back.
    #[serde(default)]
    t: bool,
    /// The page says the file is gone.
    #[serde(default)]
    n: bool,
}

fn probe_js() -> String {
    format!(
        r#"(function(){{try{{var b=document.body?document.body.innerText.slice(0,6000):'';var d={{r:document.readyState,u:navigator.userAgent,t:!!document.querySelector('iframe[src*="challenges.cloudflare.com"],iframe[src*="turnstile"],iframe[src*="hcaptcha"],iframe[src*="recaptcha"],.cf-turnstile,.h-captcha,.g-recaptcha,#challenge-form')&&![].some.call(document.querySelectorAll('[name="cf-turnstile-response"],[name="h-captcha-response"],[name="g-recaptcha-response"]'),function(e){{return !!e.value}}),n:/file not found|has been removed|no longer available|link (has )?expired|invalid file|was deleted|file does not exist/i.test(b)}};document.title="{PROBE_MARK}"+btoa(unescape(encodeURIComponent(JSON.stringify(d))));}}catch(e){{}}}})()"#
    )
}

fn decode_probe(title: &str) -> Option<Probe> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(title.strip_prefix(PROBE_MARK)?.trim()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// What counts as the host's download button. Kept in step with the page
/// script by building both from the same patterns.
const PRESS: &str = r"^(?:continue to download|start download|download|download now|free download|generate direct link|generate link|get link|create download link|download\b.*)$";
const DONT_PRESS: &str = r"^download (the|our) app\b";

#[cfg(test)]
fn presses(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    regex::Regex::new(PRESS).unwrap().is_match(&l) && !regex::Regex::new(DONT_PRESS).unwrap().is_match(&l)
}

fn press_js() -> String {
    let ok = serde_json::to_string(PRESS).unwrap_or_default();
    let no = serde_json::to_string(DONT_PRESS).unwrap_or_default();
    format!(
        r#"(function(){{try{{var ok=new RegExp({ok}),no=new RegExp({no});var c=document.querySelectorAll('button,a,[role=button],input[type=submit]');for(var i=0;i<c.length;i++){{var el=c[i];if(el.offsetParent===null||el.disabled)continue;var t=(el.textContent||el.value||'').trim().toLowerCase();if(ok.test(t)&&!no.test(t)){{el.click();return;}}}}}}catch(e){{}}}})()"#
    )
}

/// Mocha's own share flow, run in its share page: its Turnstile ("Bandwidth
/// Patrol", its own site key), the ticket its API trades the token for, then
/// the file, started the way its Download button starts it. Only the ad its
/// button opens on the first press is left out: kryo.to has Mocha's go-ahead
/// to hotlink, and the ad's pop-up is what stalls the button in our window.
const MOCHA_SITEKEY: &str = "0x4AAAAAADvCUptk-JVN3aZp";
const MOCHA_API: &str = "https://api.mocha.my";

fn mocha_share_token(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "mocha.my" && host != "www.mocha.my" {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|s| !s.is_empty());
    (parts.next()? == "share").then_some(())?;
    let token = parts.next()?;
    token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_').then(|| token.to_string())
}

fn mocha_js(token: &str) -> String {
    let token = serde_json::to_string(token).unwrap_or_default();
    format!(
        r#"(function(){{if(window.__kryMocha)return;window.__kryMocha=1;var API="{MOCHA_API}",KEY="{MOCHA_SITEKEY}",T={token};
var box=document.createElement('div');box.style.cssText='position:fixed;left:50%;top:50%;transform:translate(-50%,-50%);z-index:2147483647;padding:18px;border-radius:14px;background:#141414;border:1px solid #2a2a2a;box-shadow:0 20px 60px #000a;font:13px system-ui;color:#ddd;text-align:center';
box.innerHTML='<div style="margin-bottom:10px">Kryoto is getting your file from Mocha</div><div id="kry-ts"></div>';(document.body||document.documentElement).appendChild(box);
function say(m){{box.firstChild.textContent=m}}
function go(){{window.turnstile.render('#kry-ts',{{sitekey:KEY,theme:'dark',callback:function(t){{say('Checked. Asking Mocha for the file...');fetch(API+'/api/turnstile/verify',{{method:'POST',credentials:'include',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{token:t,shareToken:T}})}}).then(function(r){{return r.json()}}).then(function(j){{if(!j||!j.success||!j.ticket){{say('Mocha did not accept the check: '+JSON.stringify(j).slice(0,120));return}}say('Starting the download...');location.href=API+'/api/shares/'+encodeURIComponent(T)+'/download?ticket='+encodeURIComponent(j.ticket);}}).catch(function(e){{say('Mocha did not answer: '+e)}})}}}});}}
if(window.turnstile&&window.turnstile.render)go();else{{var s=document.createElement('script');s.src='https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';s.onload=function(){{var n=0,i=setInterval(function(){{if(window.turnstile&&window.turnstile.render){{clearInterval(i);go()}}else if(++n>80)clearInterval(i)}},125)}};document.head.appendChild(s);}}}})()"#
    )
}

static FILE_EXT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)\.(zip|rar|7z|00\d|iso|bin|tar|gz|xz|zst)$").unwrap());

/// A navigation straight to an archive: taken as the file, not followed.
fn looks_like_file(url: &Url) -> bool {
    FILE_EXT_RE.is_match(url.path())
}

/// The registrable part of a host, near enough: its last two labels
/// (`api.mocha.my` and `mocha.my` are one site).
fn site_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    labels[labels.len().saturating_sub(2)..].join(".")
}

/// Sites a mirror's page may load scripts and pictures from besides its own:
/// the checks hosts put in front of their files, and the public code CDNs pages
/// load libraries from. Everything else is somebody else's ads.
const THIRD_PARTY_OK: &[&str] = &[
    "challenges.cloudflare.com",
    "hcaptcha.com",
    "recaptcha.net",
    "google.com",
    "gstatic.com",
    "cdnjs.cloudflare.com",
    "cdn.jsdelivr.net",
    "unpkg.com",
    "code.jquery.com",
    "ajax.googleapis.com",
    "fonts.googleapis.com",
];

/// AD BLOCKING, the native kind.
///
/// A mirror's page is there to hand over one file, and what the file host
/// wraps it in - banners, pop-under scripts, video ads, "click anywhere"
/// layers - only slows that down or hijacks the press. So the page gets its
/// own site and the few outside sites in `THIRD_PARTY_OK`, and every other
/// script, picture and video it asks for is refused before it is fetched.
///
/// Native, not a script in the page: anything injected lands in every frame,
/// Cloudflare's Turnstile frame included, and breaks the check (Desktop has
/// been there once with a page-script plugin). Documents, fetches and XHRs
/// are never refused, so the file itself, the host's API and any frame always
/// load. VikingFile's page loads nothing from outside but Turnstile, so this
/// changes nothing it needs.
fn blocked(page_site: &str, request: &str) -> bool {
    let Ok(u) = request.parse::<Url>() else { return false };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    if site_of(&u) == page_site {
        return false;
    }
    !THIRD_PARTY_OK.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Hook `blocked` into the window's WebView2: scripts, pictures and media the
/// page asks for from anywhere else answer 403 without leaving the PC.
#[cfg(windows)]
fn block_ads<R: Runtime>(window: &tauri::WebviewWindow<R>, page_site: String) {
    let _ = window.with_webview(move |w| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        use webview2_com::WebResourceRequestedEventHandler;
        let Ok(core) = w.controller().CoreWebView2() else { return };
        let env = w.environment();
        for ctx in [
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT,
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE,
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA,
        ] {
            let _ = core.AddWebResourceRequestedFilter(windows::core::w!("*"), ctx);
        }
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = windows::core::PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            let uri = webview2_com::take_pwstr(uri);
            if blocked(&page_site, &uri) {
                let refused = env.CreateWebResourceResponse(None, 403, windows::core::w!("Blocked"), windows::core::w!(""))?;
                args.SetResponse(&refused)?;
            }
            Ok(())
        }));
        let mut token = Default::default();
        let _ = core.add_WebResourceRequested(&handler, &mut token);
    });
}

fn file_name_of(url: &Url) -> Option<String> {
    let seg = url.path_segments()?.rfind(|s| !s.is_empty())?;
    let name = percent_encoding::percent_decode_str(seg).decode_utf8_lossy().to_string();
    (!name.is_empty()).then_some(name)
}

pub async fn solve<R: Runtime>(app: &AppHandle<R>, page: &str, host: &str) -> Result<Resolved, String> {
    let url: Url = page.parse().map_err(|_| "not a valid link".to_string())?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err("not a web link".into());
    }
    let _slot = tokio::time::timeout(BUDGET, SLOT.lock()).await.map_err(|_| "another mirror is still being opened".to_string())?;
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let shared = Arc::new(Shared::default());
    if let Ok(mut a) = ACTIVE.lock() {
        *a = Some(shared.clone());
    }
    let _ = app.emit("resolver-status", serde_json::json!({ "state": "solving", "host": host }));

    let (on_dl, on_nav, on_title, on_popup) = (shared.clone(), shared.clone(), shared.clone(), shared.clone());
    let start_page = url.clone();
    let site = site_of(&url);
    #[cfg(windows)]
    let ad_site = site.clone();
    let popup_app = app.clone();
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url.clone()))
        .title(format!("Kryoto: {host} download"))
        .inner_size(960.0, 720.0)
        .min_inner_size(420.0, 320.0)
        .center()
        .visible(false)
        .focused(false)
        .on_document_title_changed(move |_, title| {
            if let Ok(mut t) = on_title.title.lock() {
                *t = Some(title);
            }
        })
        .on_download(move |_, event| {
            if let DownloadEvent::Requested { url, destination } = event {
                let name = destination.file_name().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty());
                if let Ok(mut c) = on_dl.captured.lock() {
                    *c = Some((url.to_string(), name));
                }
            }
            // Never let the page save it: the download manager takes it.
            false
        })
        // Hosts open the file in a new tab ("Opens in new tab"), and their
        // first press often opens an ad. The host's own pop-ups load here,
        // where the download is caught; anything off-site is refused.
        .on_new_window(move |target, _| {
            if looks_like_file(&target) {
                if let Ok(mut c) = on_popup.captured.lock() {
                    *c = Some((target.to_string(), file_name_of(&target)));
                }
            } else if site_of(&target) == site {
                if let Some(w) = popup_app.get_webview_window(LABEL) {
                    let _ = w.navigate(target);
                }
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .on_navigation(move |nav| {
            if nav.as_str().trim_end_matches('/') != start_page.as_str().trim_end_matches('/') && looks_like_file(nav) {
                if let Ok(mut c) = on_nav.captured.lock() {
                    *c = Some((nav.to_string(), file_name_of(nav)));
                }
                return false;
            }
            true
        });
    let window = builder.build().map_err(|e| format!("could not open its page ({e})"))?;
    #[cfg(windows)]
    block_ads(&window, ad_site);

    let started = Instant::now();
    let mut shown = false;
    let mut interactive_since: Option<Instant> = None;
    let mut ready = false;
    let mut user_agent: Option<String> = None;
    let mut last_press = Instant::now() - PRESS_EVERY;
    // A host with a recipe of its own runs that instead of button presses.
    let recipe = mocha_share_token(&url).map(|t| mocha_js(&t));
    let mut recipe_ran = false;

    let outcome = loop {
        tokio::time::sleep(TICK).await;
        if shared.cancelled.load(std::sync::atomic::Ordering::SeqCst) || app.get_webview_window(LABEL).is_none() {
            break Err("the page was closed before it gave the file".to_string());
        }
        let _ = window.eval(probe_js());
        tokio::time::sleep(TICK / 2).await;
        if let Some(p) = shared.title.lock().ok().and_then(|mut t| t.take()).and_then(|t| decode_probe(&t)) {
            if !p.u.is_empty() {
                user_agent = Some(p.u);
            }
            ready = p.r == "complete";
            interactive_since = if p.t { Some(interactive_since.unwrap_or_else(Instant::now)) } else { None };
            if p.n && started.elapsed() > Duration::from_secs(5) {
                break Err("the host says the file is gone".to_string());
            }
        }
        if let Some((file, name)) = shared.captured.lock().ok().and_then(|mut c| c.take()) {
            let cookies = window.cookies_for_url(url.clone()).unwrap_or_default();
            let cookie = cookies.iter().map(|c| format!("{}={}", c.name(), c.value())).collect::<Vec<_>>().join("; ");
            let mut headers = HashMap::from([("Referer".to_string(), url.to_string())]);
            headers.insert("User-Agent".into(), user_agent.clone().unwrap_or_else(|| super::UA.into()));
            if !cookie.is_empty() {
                headers.insert("Cookie".into(), cookie);
            }
            break Ok(Resolved {
                file_name: name.or_else(|| file.parse::<Url>().ok().and_then(|u| file_name_of(&u))),
                url: file,
                size: None,
                headers,
                // A link a page handed out is often good for few connections.
                connections: 4,
            });
        }
        if started.elapsed() >= BUDGET {
            break Err("its page did not give the file in time".to_string());
        }
        if let Some(js) = recipe.as_deref() {
            if ready && !recipe_ran {
                recipe_ran = true;
                let _ = window.eval(js);
            }
        } else if ready && interactive_since.is_none() && last_press.elapsed() >= PRESS_EVERY {
            last_press = Instant::now();
            let _ = window.eval(press_js());
        }
        let due = match interactive_since {
            Some(t) => t.elapsed() >= INTERACTIVE_GRACE,
            None => started.elapsed() >= HIDDEN_FOR,
        };
        if due && !shown {
            shown = true;
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit("resolver-status", serde_json::json!({ "state": "interactive", "host": host }));
            let _ = app.emit(
                "notify",
                crate::downloads::Notice::new(
                    &format!("{host} wants a quick check"),
                    "Its page is open in a window of its own. Pass the check there and the download carries on in Kryoto.",
                    None,
                ),
            );
        }
    };

    let _ = window.destroy();
    if let Ok(mut a) = ACTIVE.lock() {
        *a = None;
    }
    let state = if outcome.is_ok() { "captured" } else { "failed" };
    let _ = app.emit("resolver-status", serde_json::json!({ "state": state, "host": host }));
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_download_buttons_only() {
        assert!(presses("Download"));
        assert!(presses("Download · 19.7 GB"));
        assert!(presses("Start download"));
        assert!(!presses("VikingFile Mirror"));
        assert!(!presses("Download the app"));
        assert!(!presses("Upload"));
    }

    #[test]
    fn archives_count_as_files_and_pages_do_not() {
        assert!(looks_like_file(&"https://x.com/a/game.7z".parse().unwrap()));
        assert!(looks_like_file(&"https://x.com/a/game.part.001".parse().unwrap()));
        assert!(!looks_like_file(&"https://x.com/f/abc".parse().unwrap()));
        assert!(!looks_like_file(&"https://x.com/download.html".parse().unwrap()));
    }

    #[test]
    fn mocha_share_links_carry_their_token() {
        let u = |s: &str| s.parse::<Url>().unwrap();
        assert_eq!(mocha_share_token(&u("https://mocha.my/share/pvpgaoYk-6So_SEvo")).as_deref(), Some("pvpgaoYk-6So_SEvo"));
        assert_eq!(mocha_share_token(&u("https://mocha.my/files")), None);
        assert_eq!(mocha_share_token(&u("https://evil.my/share/x")), None);
        assert!(mocha_js("abc").contains(MOCHA_SITEKEY));
    }

    #[test]
    fn a_hosts_subdomains_are_one_site() {
        let u = |s: &str| s.parse::<Url>().unwrap();
        assert_eq!(site_of(&u("https://api.mocha.my/x")), site_of(&u("https://mocha.my/share/y")));
        assert_ne!(site_of(&u("https://ads.example.com/")), site_of(&u("https://mocha.my/")));
    }

    #[test]
    fn a_mirror_page_keeps_its_own_site_and_checks_but_not_ads() {
        let site = site_of(&"https://dropdrive.qsnetwork.dev/d/x".parse().unwrap());
        // Its own pages, files and storage nodes.
        assert!(!blocked(&site, "https://dropdrive.qsnetwork.dev/js/download.js"));
        assert!(!blocked(&site, "https://dd-cdn-eu01.qsnetwork.dev/files/x"));
        // The checks in front of the file, and public libraries.
        assert!(!blocked(&site, "https://challenges.cloudflare.com/turnstile/v0/api.js"));
        assert!(!blocked(&site, "https://unpkg.com/lucide@latest"));
        assert!(!blocked(&site, "https://newassets.hcaptcha.com/c/x.js"));
        // The ads.
        assert!(blocked(&site, "https://intermediatenormalconfederate.com/60/50/94/x.js"));
        assert!(blocked(&site, "https://acscdn.com/script/aclib.js"));
        assert!(blocked(&site, "https://dd-plausible.example.net/js/pa.js"));
        // Nothing that is not a web address.
        assert!(!blocked(&site, "data:image/png;base64,AAAA"));
    }

    #[test]
    fn a_probe_round_trips_through_the_title() {
        let json = r#"{"r":"complete","u":"UA","t":true,"n":false}"#;
        let title = format!("{PROBE_MARK}{}", base64::engine::general_purpose::STANDARD.encode(json));
        let p = decode_probe(&title).unwrap();
        assert!(p.t && p.r == "complete" && p.u == "UA");
        assert!(decode_probe("Some page").is_none());
    }
}
