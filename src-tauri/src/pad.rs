//! Controllers: reading them, telling them apart, and remapping them for
//! games. Behind kryo.to's `controller` feature flag: nothing here runs until
//! the shell, seeing the flag on the account, calls `pad_start`.
//!
//! - Reading is gilrs: Windows Gaming Input on Windows (which only reports
//!   presses while our window is in front, so a game never drives Kryoto),
//!   evdev on Linux (where we check for ourselves that Kryoto is in front).
//! - Telling them apart, naming their buttons and remapping are
//!   kryoto-padmap, our open-source resolver (../../kryoto-padmap).
//! - A pad never seen before gets a system notification: "Kryoto detected a
//!   controller ... Configure", which opens Settings > Controller on it.
//! - Haptics: a light tick as the selection moves, a firmer one on select,
//!   a bump at the edge (`pad_haptic`), so moving around by pad can be felt.
//! - Remaps reach games on Linux the way Steam's do: as
//!   `SDL_GAMECONTROLLERCONFIG`, which SDL games read, and so do Wine and
//!   Proton, whose controller driver is built on SDL ([`game_env`]).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use padmap::{Control, Family, Model, Remap};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// What is plugged in, and the reader thread's mailbox (`None`: not reading).
#[derive(Default)]
pub struct Pads {
    /// Which reader is the current one: each start and stop moves it on, so
    /// a reader still finishing never overwrites the next one's list.
    generation: AtomicU64,
    list: Mutex<Vec<PadInfo>>,
    tx: Mutex<Option<mpsc::Sender<Req>>>,
}

enum Req {
    Rumble(usize),
    Haptic(usize, Haptic),
    Stop,
}

/// The feel of moving around Kryoto by pad: short, light pulses (gilrs plays
/// force feedback in 50 ms ticks, so one tick is the shortest there is).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Haptic {
    /// The selection moved.
    Tick,
    /// Something was pressed, or the page changed.
    Select,
    /// Nothing further that way.
    Edge,
}

/// One connected pad, as the shell sees it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PadInfo {
    /// This session's id for it (gilrs'), for rumble and input events.
    id: usize,
    /// SDL's GUID: the same for every pad of one model, and the key its
    /// settings are kept under.
    guid: String,
    /// What the system calls it.
    name: String,
    vendor: Option<u16>,
    product: Option<u16>,
    /// What padmap made of it.
    model: Model,
    rumble: bool,
    /// Charge in percent, when it runs on a battery and says.
    battery: Option<u8>,
    charging: bool,
}

/// Settings > Controller, in `controllers.json`.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PadConfig {
    /// The account has the feature (set by `pad_start` / `pad_stop`), so a
    /// game started while it is off gets nothing from here.
    pub enabled: bool,
    /// Move around Kryoto with a pad.
    pub navigate: bool,
    /// Say so when a pad is plugged in for the first time.
    pub notify: bool,
    /// Hand remaps to games (Linux).
    pub games: bool,
    /// Feel the selection move (light rumble).
    pub haptics: bool,
    /// By GUID.
    pub pads: BTreeMap<String, PadPrefs>,
}

impl Default for PadConfig {
    fn default() -> Self {
        Self { enabled: false, navigate: true, notify: true, games: true, haptics: true, pads: BTreeMap::new() }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PadPrefs {
    /// The name it had when last seen, for listing it while unplugged.
    pub name: String,
    /// Drawn and named as this family instead of the one it was told to be.
    pub family: Option<Family>,
    pub remap: Remap,
    /// The SDL mapping games get for it: the database's, rewritten under
    /// `remap`. Linux only; `None` without a remap or a database entry.
    pub sdl: Option<String>,
}

fn config_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("controllers.json"))
}

static CONFIG_LOCK: Mutex<()> = Mutex::new(());

pub fn load_config<R: Runtime>(app: &AppHandle<R>) -> PadConfig {
    let mut cfg: PadConfig = config_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    // A hand-edited file could hold a remap that is not a permutation.
    for p in cfg.pads.values_mut() {
        p.remap = Remap::from_pairs(p.remap.changes().collect::<Vec<_>>());
    }
    cfg
}

fn save_config<R: Runtime>(app: &AppHandle<R>, cfg: &PadConfig) -> Result<(), String> {
    let path = config_path(app).ok_or("No app data folder.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Change the config under the lock, save it and return it.
fn update_config<R: Runtime>(app: &AppHandle<R>, f: impl FnOnce(&mut PadConfig)) -> Result<PadConfig, String> {
    let _guard = CONFIG_LOCK.lock().map_err(|_| "Controller settings are busy.")?;
    let mut cfg = load_config(app);
    f(&mut cfg);
    save_config(app, &cfg)?;
    Ok(cfg)
}

/// The SDL mapping a game gets for a pad under `remap` (Linux).
fn sdl_line(guid: &str, remap: &Remap) -> Option<String> {
    if remap.is_identity() || !cfg!(target_os = "linux") {
        return None;
    }
    padmap::lookup(guid).map(|m| m.remapped(remap).to_line())
}

/// What Play adds to a game's environment (Linux, with remaps saved):
///
/// - `SDL_GAMECONTROLLERCONFIG`: the remapped mappings, which SDL reads before
///   its own database.
/// - `SDL_JOYSTICK_HIDAPI=0`: SDL's HIDAPI drivers address PlayStation and
///   Switch pads by another GUID than the database's, so the remap would not
///   reach them; this keeps SDL on evdev, where it does.
/// - `PROTON_PREFER_SDL=1`: likewise for Proton, which otherwise hands some
///   pads to Windows games raw (hidraw), around SDL.
///
/// Values in the game's own launch options win (`launch::set_default`).
pub fn game_env<R: Runtime>(app: &AppHandle<R>) -> Vec<(String, String)> {
    if cfg!(windows) {
        return Vec::new();
    }
    let cfg = load_config(app);
    if !cfg.enabled || !cfg.games {
        return Vec::new();
    }
    let lines: Vec<&str> =
        cfg.pads.values().filter(|p| !p.remap.is_identity()).filter_map(|p| p.sdl.as_deref()).collect();
    if lines.is_empty() {
        return Vec::new();
    }
    vec![
        ("SDL_GAMECONTROLLERCONFIG".into(), padmap::config_env(lines)),
        ("SDL_JOYSTICK_HIDAPI".into(), "0".into()),
        ("PROTON_PREFER_SDL".into(), "1".into()),
    ]
}

/* ── Commands ─────────────────────────────────────────────── */

/// Start reading pads (the account has the flag). Returns what is plugged in.
#[tauri::command]
pub fn pad_start(app: AppHandle, pads: State<'_, Pads>) -> Vec<PadInfo> {
    let _ = update_config(&app, |c| c.enabled = true);
    // A stop just before (the shell drawn again) leaves that reader to finish
    // on its own; this one takes over.
    if let Ok(mut slot) = pads.tx.lock() {
        if slot.is_none() {
            let (tx, rx) = mpsc::channel();
            let app2 = app.clone();
            let generation = pads.generation.fetch_add(1, Ordering::SeqCst) + 1;
            if std::thread::Builder::new().name("pads".into()).spawn(move || run(&app2, rx, generation)).is_ok() {
                *slot = Some(tx);
            }
        }
    }
    pads.list.lock().map(|l| l.clone()).unwrap_or_default()
}

/// Stop (the flag went off, or the account signed out).
#[tauri::command]
pub fn pad_stop(app: AppHandle, pads: State<'_, Pads>) {
    let _ = update_config(&app, |c| c.enabled = false);
    if let Ok(mut slot) = pads.tx.lock() {
        if let Some(tx) = slot.take() {
            let _ = tx.send(Req::Stop);
        }
    }
    pads.generation.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut l) = pads.list.lock() {
        l.clear();
    }
}

#[tauri::command]
pub fn pad_list(pads: State<'_, Pads>) -> Vec<PadInfo> {
    pads.list.lock().map(|l| l.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn pad_config_get(app: AppHandle) -> PadConfig {
    load_config(&app)
}

/// Save Settings > Controller. Remaps are kept permutations and their SDL
/// mappings rewritten here, so Play only reads.
#[tauri::command]
pub fn pad_config_set(app: AppHandle, config: PadConfig) -> Result<PadConfig, String> {
    update_config(&app, |c| {
        let enabled = c.enabled;
        *c = config;
        c.enabled = enabled;
        for (guid, p) in c.pads.iter_mut() {
            p.remap = Remap::from_pairs(p.remap.changes().collect::<Vec<_>>());
            p.sdl = sdl_line(guid, &p.remap);
        }
    })
}

/// A short buzz, to tell which pad is which.
#[tauri::command]
pub fn pad_rumble(pads: State<'_, Pads>, id: usize) {
    if let Ok(slot) = pads.tx.lock() {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(Req::Rumble(id));
        }
    }
}

/// A haptic pulse on one pad, as the shell moves around.
#[tauri::command]
pub fn pad_haptic(pads: State<'_, Pads>, id: usize, kind: Haptic) {
    if let Ok(slot) = pads.tx.lock() {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(Req::Haptic(id, kind));
        }
    }
}

/* ── The reader thread ────────────────────────────────────── */

/// gilrs' buttons as padmap's controls. gilrs calls the bumpers "triggers"
/// and the triggers "triggers 2".
fn control_of(b: Button) -> Option<Control> {
    Some(match b {
        Button::South => Control::South,
        Button::East => Control::East,
        Button::West => Control::West,
        Button::North => Control::North,
        Button::LeftTrigger => Control::LeftBumper,
        Button::RightTrigger => Control::RightBumper,
        Button::LeftTrigger2 => Control::LeftTrigger,
        Button::RightTrigger2 => Control::RightTrigger,
        Button::Select => Control::Select,
        Button::Start => Control::Start,
        Button::Mode => Control::Guide,
        Button::LeftThumb => Control::LeftStick,
        Button::RightThumb => Control::RightStick,
        Button::DPadUp => Control::DpadUp,
        Button::DPadDown => Control::DpadDown,
        Button::DPadLeft => Control::DpadLeft,
        Button::DPadRight => Control::DpadRight,
        _ => return None,
    })
}

/// Sticks and triggers, -1..1 (sticks, y up) and 0..1 (triggers).
#[derive(Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Axes {
    id: usize,
    lx: f32,
    ly: f32,
    rx: f32,
    ry: f32,
    lt: f32,
    rt: f32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ButtonEvent {
    id: usize,
    control: Control,
    pressed: bool,
}

fn info(gilrs: &Gilrs, id: GamepadId) -> Option<PadInfo> {
    let pad = gilrs.connected_gamepad(id)?;
    let guid: String = pad.uuid().iter().map(|b| format!("{b:02x}")).collect();
    let (battery, charging) = match pad.power_info() {
        gilrs::PowerInfo::Discharging(p) => (Some(p), false),
        gilrs::PowerInfo::Charging(p) => (Some(p), true),
        gilrs::PowerInfo::Charged => (Some(100), false),
        _ => (None, false),
    };
    Some(PadInfo {
        id: usize::from(id),
        guid,
        name: pad.name().to_string(),
        vendor: pad.vendor_id(),
        product: pad.product_id(),
        model: padmap::identify(pad.vendor_id(), pad.product_id(), pad.name()),
        rumble: pad.is_ff_supported(),
        battery,
        charging,
    })
}

fn publish<R: Runtime>(app: &AppHandle<R>, gilrs: &Gilrs, generation: u64) {
    let state = app.state::<Pads>();
    if state.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let list: Vec<PadInfo> = gilrs.gamepads().filter_map(|(id, _)| info(gilrs, id)).collect();
    if let Ok(mut l) = state.list.lock() {
        *l = list.clone();
    }
    let _ = app.emit("pad-list", list);
}

/// First sight of a pad: remember it, and when it is new, say so.
fn greet<R: Runtime>(app: &AppHandle<R>, pad: &PadInfo) {
    let mut fresh = false;
    let cfg = update_config(app, |c| {
        let prefs = c.pads.entry(pad.guid.clone()).or_insert_with(|| {
            fresh = true;
            PadPrefs::default()
        });
        prefs.name = pad.name.clone();
    });
    let Ok(cfg) = cfg else { return };
    if fresh && cfg.notify {
        ask_to_configure(app, pad);
    }
}

/// "Kryoto detected a controller": a system notification with a Configure
/// button, which opens Settings > Controller on that pad.
fn ask_to_configure<R: Runtime>(app: &AppHandle<R>, pad: &PadInfo) {
    let mut note = notify_rust::Notification::new();
    note.summary("Kryoto detected a controller");
    note.body(&format!("{} is connected. Would you like to configure this controller?", pad.model.name));
    note.action("default", "Configure");
    note.action("configure", "Configure");
    note.auto_icon();
    // As `system::os_notify`: a toast filed under the installed app's ID.
    #[cfg(windows)]
    if !cfg!(debug_assertions) {
        note.app_id(&app.config().identifier);
    }
    let app = app.clone();
    let guid = pad.guid.clone();
    tauri::async_runtime::spawn_blocking(move || match note.show() {
        Ok(handle) => handle.wait_for_action(|action| {
            if action == "configure" || action == "default" {
                crate::system::show_main(&app);
                let _ = app.emit("pad-configure", guid);
            }
        }),
        Err(e) => crate::logging::error("pads", &e.to_string()),
    });
}

fn run<R: Runtime>(app: &AppHandle<R>, rx: mpsc::Receiver<Req>, generation: u64) {
    let mut gilrs = match Gilrs::new() {
        Ok(g) => g,
        // No backend for this system: an empty reader that never reports.
        Err(gilrs::Error::NotImplemented(g)) => g,
        Err(e) => {
            crate::logging::error("pads", &format!("controllers unavailable: {e}"));
            return;
        }
    };
    publish(app, &gilrs, generation);
    let present: Vec<PadInfo> = gilrs.gamepads().filter_map(|(id, _)| info(&gilrs, id)).collect();
    for pad in &present {
        greet(app, pad);
    }

    let mut focused = false;
    let mut focus_at = Instant::now() - Duration::from_secs(1);
    let mut axes: HashMap<usize, Axes> = HashMap::new();
    let mut sent: HashMap<usize, Axes> = HashMap::new();
    let mut sent_at = Instant::now();
    let mut effects: Vec<(Instant, gilrs::ff::Effect)> = Vec::new();
    // Haptic pulses are made once per pad and replayed: a device has only a
    // few effect slots, and making one each move would use them up.
    let mut pulses: HashMap<(usize, Haptic), gilrs::ff::Effect> = HashMap::new();

    loop {
        match rx.try_recv() {
            Ok(Req::Stop) | Err(mpsc::TryRecvError::Disconnected) => break,
            Ok(Req::Rumble(id)) => {
                if let Some((gid, _)) = gilrs.gamepads().find(|(g, p)| usize::from(*g) == id && p.is_ff_supported()) {
                    if let Some(effect) = buzz(&mut gilrs, gid) {
                        effects.push((Instant::now(), effect));
                    }
                }
            }
            Ok(Req::Haptic(id, kind)) => {
                if let std::collections::hash_map::Entry::Vacant(slot) = pulses.entry((id, kind)) {
                    let gid = gilrs.gamepads().find(|(g, p)| usize::from(*g) == id && p.is_ff_supported()).map(|(g, _)| g);
                    if let Some(effect) = gid.and_then(|g| pulse(&mut gilrs, g, kind)) {
                        slot.insert(effect);
                    }
                }
                if let Some(effect) = pulses.get(&(id, kind)) {
                    let _ = effect.play();
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        // Only drive Kryoto while it is in front (Linux reads pads whatever
        // has focus; Windows Gaming Input already only reports then).
        if focus_at.elapsed() > Duration::from_millis(200) {
            focus_at = Instant::now();
            focused = app.get_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
        }

        let mut next = gilrs.next_event_blocking(Some(Duration::from_millis(16)));
        while let Some(ev) = next {
            let id = usize::from(ev.id);
            match ev.event {
                EventType::Connected => {
                    publish(app, &gilrs, generation);
                    if let Some(pad) = info(&gilrs, ev.id) {
                        greet(app, &pad);
                    }
                }
                EventType::Disconnected => {
                    pulses.retain(|(pad, _), _| *pad != id);
                    axes.remove(&id);
                    sent.remove(&id);
                    publish(app, &gilrs, generation);
                }
                EventType::ButtonPressed(b, _) | EventType::ButtonReleased(b, _) if focused => {
                    if let Some(control) = control_of(b) {
                        let pressed = matches!(ev.event, EventType::ButtonPressed(..));
                        let _ = app.emit("pad-button", ButtonEvent { id, control, pressed });
                    }
                }
                EventType::ButtonChanged(b, v, _) => {
                    let a = axes.entry(id).or_insert(Axes { id, ..Default::default() });
                    match b {
                        Button::LeftTrigger2 => a.lt = v,
                        Button::RightTrigger2 => a.rt = v,
                        _ => {}
                    }
                }
                EventType::AxisChanged(axis, v, _) => {
                    let a = axes.entry(id).or_insert(Axes { id, ..Default::default() });
                    match axis {
                        Axis::LeftStickX => a.lx = v,
                        Axis::LeftStickY => a.ly = v,
                        Axis::RightStickX => a.rx = v,
                        Axis::RightStickY => a.ry = v,
                        _ => {}
                    }
                }
                _ => {}
            }
            next = gilrs.next_event();
        }

        // Sticks and triggers at most 30 times a second, and only as they move.
        if focused && sent_at.elapsed() >= Duration::from_millis(33) {
            sent_at = Instant::now();
            for (id, a) in &axes {
                if sent.get(id) != Some(a) {
                    sent.insert(*id, *a);
                    let _ = app.emit("pad-axes", *a);
                }
            }
        }
        effects.retain(|(at, _)| at.elapsed() < Duration::from_secs(1));
    }
}

fn pulse(gilrs: &mut Gilrs, id: GamepadId, kind: Haptic) -> Option<gilrs::ff::Effect> {
    use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
    let (kind, ms) = match kind {
        Haptic::Tick => (BaseEffectType::Weak { magnitude: 16_000 }, 50),
        Haptic::Select => (BaseEffectType::Strong { magnitude: 22_000 }, 50),
        Haptic::Edge => (BaseEffectType::Strong { magnitude: 34_000 }, 100),
    };
    EffectBuilder::new()
        .add_effect(BaseEffect { kind, scheduling: Replay { play_for: Ticks::from_ms(ms), ..Default::default() }, envelope: Default::default() })
        .repeat(Repeat::For(Ticks::from_ms(ms)))
        .gamepads(&[id])
        .finish(gilrs)
        .ok()
}

fn buzz(gilrs: &mut Gilrs, id: GamepadId) -> Option<gilrs::ff::Effect> {
    use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Repeat, Replay, Ticks};
    let effect = EffectBuilder::new()
        .add_effect(BaseEffect {
            kind: BaseEffectType::Strong { magnitude: 45_000 },
            scheduling: Replay { play_for: Ticks::from_ms(220), ..Default::default() },
            envelope: Default::default(),
        })
        .add_effect(BaseEffect {
            kind: BaseEffectType::Weak { magnitude: 45_000 },
            scheduling: Replay { play_for: Ticks::from_ms(220), ..Default::default() },
            envelope: Default::default(),
        })
        .repeat(Repeat::For(Ticks::from_ms(220)))
        .gamepads(&[id])
        .finish(gilrs)
        .ok()?;
    effect.play().ok()?;
    Some(effect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gilrs_button_we_name_is_distinct() {
        let all = [
            Button::South, Button::East, Button::West, Button::North, Button::LeftTrigger, Button::RightTrigger,
            Button::LeftTrigger2, Button::RightTrigger2, Button::Select, Button::Start, Button::Mode,
            Button::LeftThumb, Button::RightThumb, Button::DPadUp, Button::DPadDown, Button::DPadLeft, Button::DPadRight,
        ];
        let mut seen: Vec<Control> = all.iter().filter_map(|b| control_of(*b)).collect();
        assert_eq!(seen.len(), all.len());
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), all.len());
        assert_eq!(control_of(Button::LeftTrigger), Some(Control::LeftBumper));
    }

    #[test]
    fn config_defaults_and_old_files() {
        let c: PadConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.enabled && c.navigate && c.notify && c.games && c.haptics);
        let c: PadConfig = serde_json::from_str(r#"{"pads":{"abc":{"name":"Pad","family":"nintendo","remap":{"a":"b","b":"a"}}}}"#).unwrap();
        let p = &c.pads["abc"];
        assert_eq!(p.family, Some(Family::Nintendo));
        assert_eq!(p.remap.acts_as(Control::South), Control::East);
    }

    #[test]
    fn sdl_lines_only_for_remaps_on_linux() {
        let guid = "030000004c050000e60c000011810000";
        assert_eq!(sdl_line(guid, &Remap::new()), None);
        let mut r = Remap::new();
        r.assign(Control::South, Control::East);
        let line = sdl_line(guid, &r);
        if cfg!(target_os = "linux") {
            let m = padmap::SdlMapping::parse(&line.unwrap()).unwrap();
            assert_eq!(m.get("a"), Some("b1"));
            assert_eq!(m.get("b"), Some("b0"));
        } else {
            assert_eq!(line, None);
        }
    }
}
