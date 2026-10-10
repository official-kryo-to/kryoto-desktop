//! Controllers: reading them, telling them apart, setting them up and
//! remapping them for games. Behind kryo.to's `controller` feature flag:
//! nothing here runs until the shell, seeing the flag on the account, calls
//! `pad_start`.
//!
//! - Reading is SDL2 (statically linked), the same input library Steam and
//!   most PC games use: XInput, RawInput, HIDAPI and DirectInput on Windows,
//!   evdev on Linux, with the SDL_GameControllerDB of known pads built in. It
//!   reads in the background too; Kryoto only acts on presses while its own
//!   window is in front.
//! - A pad SDL does not know (or one it maps wrong) is set up once with the
//!   step-by-step setup in Settings > Controller: it watches the raw buttons,
//!   axes and hats (`pad_capture`) and saves an SDL mapping for that model
//!   (`pad_mapping_set`), which Kryoto and SDL games then use.
//! - Telling pads apart, naming their buttons and remapping are kryoto-padmap,
//!   our open-source resolver (../../kryoto-padmap).
//! - A pad never seen before gets a system notification: "Kryoto detected a
//!   controller ... Configure", which opens Settings > Controller on it.
//! - Haptics: a light tick as the selection moves, a firmer one on select,
//!   a bump at the edge (`pad_haptic`).
//! - Games get the setup and the remaps:
//!   - Linux: as `SDL_GAMECONTROLLERCONFIG`, which SDL games read, and so do
//!     Wine and Proton, whose controller driver is built on SDL ([`game_env`]).
//!   - Windows: through a virtual Xbox controller (ViGEmBus) while a game
//!     runs, so every game that reads Xbox controllers gets the pad as set up
//!     and remapped, rumble included ([`virt`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use padmap::{Control, Family, Model, Remap};
use sdl2::controller::{Axis as SAxis, Button as SButton, GameController};
use sdl2::event::Event;
use sdl2::joystick::{HatState, Joystick};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// What is plugged in, and the reader thread's mailbox.
#[derive(Default)]
pub struct Pads {
    list: Mutex<Vec<PadInfo>>,
    tx: Mutex<Option<mpsc::Sender<Req>>>,
    /// Whether the virtual-controller driver answered, when last asked.
    #[cfg_attr(not(windows), allow(dead_code))]
    driver: Mutex<Option<bool>>,
}

enum Req {
    Start,
    Stop,
    Rumble(u32),
    Haptic(u32, Haptic),
    /// Report the raw inputs of this pad (the setup), or stop.
    Capture(Option<u32>),
    /// The saved setup of this model changed: apply it.
    Mapping(String),
    /// A game asked the virtual controller to rumble.
    #[cfg_attr(not(windows), allow(dead_code))]
    VirtualRumble(u32, u8, u8),
}

/// The feel of moving around Kryoto by pad.
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
    /// SDL's instance id: this connection's, for rumble and input events.
    id: u32,
    /// SDL's GUID: the same for every pad of one model, and the key its
    /// settings are kept under.
    guid: String,
    /// The name to show: the pad's own, or its maker's when the system only
    /// gave a placeholder ("HID-compliant game controller").
    name: String,
    /// What the system calls it.
    system_name: String,
    vendor: Option<u16>,
    product: Option<u16>,
    /// What padmap made of it.
    model: Model,
    /// SDL knows its buttons (its database, or a setup saved here).
    mapped: bool,
    /// Its buttons come from a setup saved here.
    custom: bool,
    /// An Xbox pad: games read it natively, so no virtual controller.
    xinput: bool,
    rumble: bool,
    battery: Option<u8>,
    charging: bool,
    /// What the setup can read: raw buttons, axes and hats.
    buttons: u32,
    axes: u32,
    hats: u32,
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
    /// Hand the setup and remaps to games.
    pub games: bool,
    /// Feel the selection move (light rumble).
    pub haptics: bool,
    /// Windows: play as a virtual Xbox controller while a game runs (needs
    /// the ViGEmBus driver; pads that are Xbox pads already are left alone).
    pub virtual_pad: bool,
    /// By GUID.
    pub pads: BTreeMap<String, PadPrefs>,
}

impl Default for PadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            navigate: true,
            notify: true,
            games: true,
            haptics: true,
            virtual_pad: true,
            pads: BTreeMap::new(),
        }
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
    /// The setup saved here: an SDL mapping for this model, used by Kryoto,
    /// by SDL games (Linux) and by the virtual controller (Windows).
    pub mapping: Option<String>,
    /// SDL's own mapping for it (its database), as last seen: the base a
    /// remap rewrites when there is no setup, and what Reset goes back to.
    pub base: Option<String>,
    /// The SDL mapping games get: the setup or the base, under `remap`.
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

/// The mapping games get for a pad: its setup, or SDL's own under a remap.
fn sdl_line(prefs: &PadPrefs) -> Option<String> {
    let source = prefs.mapping.as_deref().or(prefs.base.as_deref())?;
    if prefs.mapping.is_none() && prefs.remap.is_identity() {
        return None; // SDL already knows it, unchanged
    }
    let parsed = padmap::SdlMapping::parse(source).ok()?;
    Some(parsed.remapped(&prefs.remap).to_line())
}

/// What Play adds to a game's environment on Linux (a setup or a remap saved):
///
/// - `SDL_GAMECONTROLLERCONFIG`: the mappings, which SDL reads before its own
///   database.
/// - `SDL_JOYSTICK_HIDAPI=0`: SDL's HIDAPI drivers address PlayStation and
///   Switch pads by another GUID than evdev's, so the mapping would not reach
///   them; this keeps SDL on evdev, where it does.
/// - `PROTON_PREFER_SDL=1`: likewise for Proton, which otherwise hands some
///   pads to Windows games raw (hidraw), around SDL.
///
/// Windows games get the virtual controller instead ([`virt`]). Values in the
/// game's own launch options win (`launch::set_default`).
pub fn game_env<R: Runtime>(app: &AppHandle<R>) -> Vec<(String, String)> {
    if cfg!(windows) {
        return Vec::new();
    }
    let cfg = load_config(app);
    if !cfg.enabled || !cfg.games {
        return Vec::new();
    }
    let lines: Vec<&str> = cfg.pads.values().filter_map(|p| p.sdl.as_deref()).collect();
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

fn send(pads: &State<'_, Pads>, req: Req) {
    if let Ok(slot) = pads.tx.lock() {
        if let Some(tx) = slot.as_ref() {
            let _ = tx.send(req);
        }
    }
}

/// Start reading pads (the account has the flag). Returns what is plugged in.
///
/// One thread for the app's life: SDL may only be used from the thread that
/// started it, so stopping pauses that thread rather than ending it.
#[tauri::command]
pub fn pad_start(app: AppHandle, pads: State<'_, Pads>) -> Vec<PadInfo> {
    let _ = update_config(&app, |c| c.enabled = true);
    if let Ok(mut slot) = pads.tx.lock() {
        match slot.as_ref() {
            Some(tx) => {
                let _ = tx.send(Req::Start);
            }
            None => {
                let (tx, rx) = mpsc::channel();
                let app2 = app.clone();
                let back = tx.clone();
                if std::thread::Builder::new().name("pads".into()).spawn(move || run(&app2, rx, back)).is_ok() {
                    *slot = Some(tx);
                }
            }
        }
    }
    pads.list.lock().map(|l| l.clone()).unwrap_or_default()
}

/// Stop (the flag went off, or the account signed out).
#[tauri::command]
pub fn pad_stop(app: AppHandle, pads: State<'_, Pads>) {
    let _ = update_config(&app, |c| c.enabled = false);
    send(&pads, Req::Stop);
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

/// Save Settings > Controller. Remaps are kept permutations, the setup and
/// base mappings are kept as they were (only `pad_mapping_set` and the reader
/// change those), and what games get is rewritten here, so Play only reads.
#[tauri::command]
pub fn pad_config_set(app: AppHandle, config: PadConfig) -> Result<PadConfig, String> {
    update_config(&app, |c| {
        let enabled = c.enabled;
        let before = std::mem::take(&mut c.pads);
        *c = config;
        c.enabled = enabled;
        for (guid, p) in c.pads.iter_mut() {
            let old = before.get(guid);
            p.mapping = old.and_then(|o| o.mapping.clone());
            p.base = old.and_then(|o| o.base.clone());
            p.remap = Remap::from_pairs(p.remap.changes().collect::<Vec<_>>());
            p.sdl = sdl_line(p);
        }
    })
}

/// A short buzz, to tell which pad is which.
#[tauri::command]
pub fn pad_rumble(pads: State<'_, Pads>, id: u32) {
    send(&pads, Req::Rumble(id));
}

/// A haptic pulse on one pad, as the shell moves around.
#[tauri::command]
pub fn pad_haptic(pads: State<'_, Pads>, id: u32, kind: Haptic) {
    send(&pads, Req::Haptic(id, kind));
}

/// The setup listens to this pad's raw inputs (`pad-raw`), or stops.
#[tauri::command]
pub fn pad_capture(pads: State<'_, Pads>, id: Option<u32>) {
    send(&pads, Req::Capture(id));
}

/// What the setup can save for one control: a button (`b3`), a hat
/// direction (`h0.4`), an axis (`a2`), half an axis (`+a2`, `-a2`) or an
/// inverted one (`a1~`).
fn valid_source(s: &str) -> bool {
    let (body, rest) = match s.strip_prefix(['+', '-']) {
        Some(r) => (r, true),
        None => (s, false),
    };
    let body = body.strip_suffix('~').unwrap_or(body);
    let num = |n: &str| !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit());
    match body.split_at_checked(1) {
        Some(("b", n)) => !rest && num(n),
        Some(("a", n)) => num(n),
        Some(("h", n)) => !rest && n.split_once('.').is_some_and(|(h, m)| num(h) && matches!(m, "1" | "2" | "4" | "8")),
        _ => false,
    }
}

/// Save (or, with `None`, forget) the setup of one model of pad: SDL field
/// name -> source, as the setup heard it. Kryoto uses it at once.
#[tauri::command]
pub fn pad_mapping_set(
    app: AppHandle,
    pads: State<'_, Pads>,
    guid: String,
    name: String,
    bindings: Option<BTreeMap<String, String>>,
) -> Result<PadConfig, String> {
    if guid.len() != 32 || !guid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("That controller has no valid id.".into());
    }
    let line = match bindings {
        None => None,
        Some(b) => {
            const AXES: [&str; 4] = ["leftx", "lefty", "rightx", "righty"];
            let mut out = format!("{guid},{},", name.replace(',', " ").trim());
            for (key, value) in &b {
                let known = Control::from_sdl_name(key).is_some() || AXES.contains(&key.as_str());
                if !known || !valid_source(value) {
                    return Err(format!("The setup sent {key}:{value}, which is not something a pad has."));
                }
                out.push_str(&format!("{key}:{value},"));
            }
            if !b.keys().any(|k| k == "a") {
                return Err("The setup needs at least the bottom face button.".into());
            }
            Some(out)
        }
    };
    let cfg = update_config(&app, |c| {
        let p = c.pads.entry(guid.clone()).or_default();
        p.mapping = line;
        p.sdl = sdl_line(p);
    })?;
    send(&pads, Req::Mapping(guid));
    Ok(cfg)
}

/// Windows: is the virtual-controller driver (ViGEmBus) there? Elsewhere
/// always false: Linux games get the setup through SDL instead.
#[tauri::command]
pub fn pad_virtual_driver(pads: State<'_, Pads>) -> bool {
    #[cfg(windows)]
    {
        let found = vigem_client::Client::connect().is_ok();
        if let Ok(mut d) = pads.driver.lock() {
            *d = Some(found);
        }
        found
    }
    #[cfg(not(windows))]
    {
        let _ = pads;
        false
    }
}

/* ── The reader thread ────────────────────────────────────── */

fn control_of(b: SButton) -> Control {
    match b {
        SButton::A => Control::South,
        SButton::B => Control::East,
        SButton::X => Control::West,
        SButton::Y => Control::North,
        SButton::Back => Control::Select,
        SButton::Guide => Control::Guide,
        SButton::Start => Control::Start,
        SButton::LeftStick => Control::LeftStick,
        SButton::RightStick => Control::RightStick,
        SButton::LeftShoulder => Control::LeftBumper,
        SButton::RightShoulder => Control::RightBumper,
        SButton::DPadUp => Control::DpadUp,
        SButton::DPadDown => Control::DpadDown,
        SButton::DPadLeft => Control::DpadLeft,
        SButton::DPadRight => Control::DpadRight,
        SButton::Misc1 => Control::Misc,
        SButton::Paddle1 => Control::PaddleUpperRight,
        SButton::Paddle2 => Control::PaddleUpperLeft,
        SButton::Paddle3 => Control::PaddleLowerRight,
        SButton::Paddle4 => Control::PaddleLowerLeft,
        SButton::Touchpad => Control::Touchpad,
    }
}

/// Sticks and triggers, -1..1 (sticks, y up) and 0..1 (triggers).
#[derive(Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Axes {
    id: u32,
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
    id: u32,
    control: Control,
    pressed: bool,
}

/// A raw input, for the setup: `button` (index, 1 = down), `axis` (index,
/// -1..1) or `hat` (index, SDL's direction bits).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    id: u32,
    kind: &'static str,
    index: u32,
    value: f32,
}

/// Where every raw input rests, when the setup starts listening: so a
/// trigger resting at -1 is not taken for a pull.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RawState {
    id: u32,
    axes: Vec<f32>,
    hats: Vec<u8>,
}

struct Dev {
    joy: Joystick,
    ctl: Option<GameController>,
    info: PadInfo,
    /// The last value sent for each raw axis while capturing.
    raw_axes: Vec<f32>,
}

fn guid_of(joy: &Joystick) -> String {
    joy.guid().string()
}

fn battery_of(joy: &Joystick) -> (Option<u8>, bool) {
    use sdl2::joystick::PowerLevel::*;
    match joy.power_level() {
        Ok(Empty) => (Some(5), false),
        Ok(Low) => (Some(20), false),
        Ok(Medium) => (Some(55), false),
        Ok(Full) => (Some(100), false),
        _ => (None, false),
    }
}

/// The name to show: a model padmap knows by its ids, else the pad's own
/// name (SDL's database name when it has one), else its maker's.
fn display_name(model: &Model, sdl_name: &str) -> String {
    if model.matched == padmap::Matched::Ids || padmap::is_generic_name(sdl_name) {
        model.name.clone()
    } else {
        sdl_name.trim().to_string()
    }
}

struct Reader<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    js: sdl2::JoystickSubsystem,
    gc: sdl2::GameControllerSubsystem,
    devs: HashMap<u32, Dev>,
    /// Our own virtual controllers, as SDL sees them: never listed.
    ignored: HashSet<u32>,
    capture: Option<u32>,
    focused: bool,
    #[cfg(windows)]
    virt: virt::Virtual,
}

impl<R: Runtime> Reader<'_, R> {
    fn publish(&self) {
        let mut list: Vec<PadInfo> = self.devs.values().map(|d| d.info.clone()).collect();
        list.sort_by_key(|p| p.id);
        if let Ok(mut l) = self.app.state::<Pads>().list.lock() {
            *l = list.clone();
        }
        let _ = self.app.emit("pad-list", list);
    }

    /// Open device `index` (SDL's enumeration index), unless it is open or ours.
    fn open(&mut self, index: u32) -> Option<u32> {
        let i = index as i32;
        let id = unsafe { sdl2::sys::SDL_JoystickGetDeviceInstanceID(i) };
        if id < 0 {
            return None;
        }
        let id = id as u32;
        if self.devs.contains_key(&id) || self.ignored.contains(&id) {
            return None;
        }
        let vendor = unsafe { sdl2::sys::SDL_JoystickGetDeviceVendor(i) };
        let product = unsafe { sdl2::sys::SDL_JoystickGetDeviceProduct(i) };
        #[cfg(windows)]
        if self.virt.is_ours(vendor, product) {
            self.ignored.insert(id);
            return None;
        }
        let kind = unsafe { sdl2::sys::SDL_GameControllerTypeForIndex(i) };
        let joy = self.js.open(index).ok()?;
        let ctl = if self.gc.is_game_controller(index) { self.gc.open(index).ok() } else { None };
        let system_name = ctl.as_ref().map(|c| c.name()).unwrap_or_else(|| joy.name());
        let some = |v: u16| (v != 0).then_some(v);
        let model = padmap::identify(some(vendor), some(product), &system_name);
        let guid = guid_of(&joy);
        let custom = load_config(self.app).pads.get(&guid).is_some_and(|p| p.mapping.is_some());
        let (battery, charging) = battery_of(&joy);
        use sdl2::sys::SDL_GameControllerType::*;
        let info = PadInfo {
            id,
            guid: guid.clone(),
            name: display_name(&model, &system_name),
            system_name: system_name.clone(),
            vendor: some(vendor),
            product: some(product),
            model,
            mapped: ctl.is_some(),
            custom,
            xinput: matches!(kind, SDL_CONTROLLER_TYPE_XBOX360 | SDL_CONTROLLER_TYPE_XBOXONE),
            rumble: ctl.as_ref().map(|c| c.has_rumble()).unwrap_or_else(|| joy.has_rumble()),
            battery,
            charging,
            buttons: joy.num_buttons(),
            axes: joy.num_axes(),
            hats: joy.num_hats(),
        };
        // Remember SDL's own mapping (not a setup saved here) as the base.
        let base = if custom { None } else { ctl.as_ref().map(|c| c.mapping()) };
        let fresh = {
            let mut fresh = false;
            let _ = update_config(self.app, |c| {
                let p = c.pads.entry(guid.clone()).or_insert_with(|| {
                    fresh = true;
                    PadPrefs::default()
                });
                p.name = info.name.clone();
                if base.is_some() {
                    p.base = base.clone();
                }
                p.sdl = sdl_line(p);
            });
            fresh
        };
        let axes = joy.num_axes() as usize;
        self.devs.insert(id, Dev { joy, ctl, info: info.clone(), raw_axes: vec![0.0; axes] });
        if fresh && load_config(self.app).notify {
            ask_to_configure(self.app, &info);
        }
        Some(id)
    }

    fn open_all(&mut self) {
        let n = self.js.num_joysticks().unwrap_or(0);
        for i in 0..n {
            self.open(i);
        }
        self.publish();
    }

    fn close_all(&mut self) {
        self.devs.clear();
        self.capture = None;
        #[cfg(windows)]
        self.virt.unplug_all();
        self.publish();
    }

    /// A setup was saved (or forgotten) for `guid`: tell SDL, and open the
    /// pads of that model as controllers if they were not yet.
    fn apply_mapping(&mut self, guid: &str) {
        let prefs = load_config(self.app).pads.get(guid).cloned().unwrap_or_default();
        // SDL cannot forget a mapping; going back means re-adding its own.
        if let Some(line) = prefs.mapping.as_deref().or(prefs.base.as_deref()) {
            if let Err(e) = self.gc.add_mapping(line) {
                crate::logging::error("pads", &format!("mapping refused: {e}"));
            }
        }
        let ids: Vec<u32> = self.devs.iter().filter(|(_, d)| d.info.guid == guid).map(|(id, _)| *id).collect();
        for id in ids {
            // Reopen: the controller (and its name, and whether it is mapped)
            // follows the new mapping.
            self.devs.remove(&id);
            let n = self.js.num_joysticks().unwrap_or(0);
            if let Some(index) = (0..n).find(|i| unsafe { sdl2::sys::SDL_JoystickGetDeviceInstanceID(*i as i32) } as u32 == id) {
                self.open(index);
            }
        }
        self.publish();
    }

    fn rumble(&mut self, id: u32, low: u16, high: u16, ms: u32) {
        if let Some(d) = self.devs.get_mut(&id) {
            let _ = match d.ctl.as_mut() {
                Some(c) => c.set_rumble(low, high, ms),
                None => d.joy.set_rumble(low, high, ms),
            };
        }
    }

    fn axes_of(&self, id: u32) -> Option<Axes> {
        let c = self.devs.get(&id)?.ctl.as_ref()?;
        let n = |v: i16| (v as f32 / 32767.0).clamp(-1.0, 1.0);
        Some(Axes {
            id,
            lx: n(c.axis(SAxis::LeftX)),
            ly: -n(c.axis(SAxis::LeftY)),
            rx: n(c.axis(SAxis::RightX)),
            ry: -n(c.axis(SAxis::RightY)),
            lt: n(c.axis(SAxis::TriggerLeft)).max(0.0),
            rt: n(c.axis(SAxis::TriggerRight)).max(0.0),
        })
    }

    fn raw_state(&self, id: u32) -> Option<RawState> {
        let d = self.devs.get(&id)?;
        Some(RawState {
            id,
            axes: (0..d.joy.num_axes()).map(|a| d.joy.axis(a).map(|v| v as f32 / 32767.0).unwrap_or(0.0)).collect(),
            hats: (0..d.joy.num_hats()).map(|h| d.joy.hat(h).map(|s| s as u8).unwrap_or(0)).collect(),
        })
    }

    fn handle(&mut self, ev: Event, dirty: &mut HashSet<u32>) {
        match ev {
            Event::JoyDeviceAdded { which, .. } => {
                if self.open(which).is_some() {
                    self.publish();
                }
            }
            Event::JoyDeviceRemoved { which, .. } => {
                self.ignored.remove(&which);
                if self.devs.remove(&which).is_some() {
                    #[cfg(windows)]
                    self.virt.unplug(which);
                    if self.capture == Some(which) {
                        self.capture = None;
                    }
                    self.publish();
                }
            }
            Event::ControllerButtonDown { which, button, .. } | Event::ControllerButtonUp { which, button, .. } => {
                let pressed = matches!(ev, Event::ControllerButtonDown { .. });
                if self.focused && self.devs.contains_key(&which) {
                    let _ = self.app.emit("pad-button", ButtonEvent { id: which, control: control_of(button), pressed });
                }
            }
            Event::ControllerAxisMotion { which, .. } => {
                dirty.insert(which);
            }
            Event::JoyButtonDown { which, button_idx, .. } | Event::JoyButtonUp { which, button_idx, .. }
                if self.capture == Some(which) =>
            {
                let value = if matches!(ev, Event::JoyButtonDown { .. }) { 1.0 } else { 0.0 };
                let _ = self.app.emit("pad-raw", RawEvent { id: which, kind: "button", index: button_idx as u32, value });
            }
            Event::JoyAxisMotion { which, axis_idx, value, .. } if self.capture == Some(which) => {
                let v = value as f32 / 32767.0;
                if let Some(d) = self.devs.get_mut(&which) {
                    let last = d.raw_axes.get(axis_idx as usize).copied().unwrap_or(0.0);
                    // Only real movement: a resting stick wobbles.
                    if (v - last).abs() > 0.08 {
                        if let Some(slot) = d.raw_axes.get_mut(axis_idx as usize) {
                            *slot = v;
                        }
                        let _ = self.app.emit("pad-raw", RawEvent { id: which, kind: "axis", index: axis_idx as u32, value: v });
                    }
                }
            }
            Event::JoyHatMotion { which, hat_idx, state, .. } if self.capture == Some(which) => {
                let bits = match state {
                    HatState::Centered => 0,
                    s => s as u8,
                };
                let _ = self.app.emit("pad-raw", RawEvent { id: which, kind: "hat", index: hat_idx as u32, value: bits as f32 });
            }
            _ => {}
        }
    }
}

/// First sight of a pad (`fresh`): "Kryoto detected a controller", with a
/// Configure button that opens Settings > Controller on it.
fn ask_to_configure<R: Runtime>(app: &AppHandle<R>, pad: &PadInfo) {
    let mut note = notify_rust::Notification::new();
    note.summary("Kryoto detected a controller");
    if pad.mapped {
        note.body(&format!("{} is connected. Would you like to configure this controller?", pad.name));
    } else {
        note.body(&format!("{} is connected. Set it up once so its buttons work in Kryoto and your games.", pad.name));
    }
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
        Err(e) => crate::logging::error("notify", &e.to_string()),
    });
}

fn run<R: Runtime>(app: &AppHandle<R>, rx: mpsc::Receiver<Req>, back: mpsc::Sender<Req>) {
    // Read in the background too: the virtual controller needs the pad while
    // a game has the focus. Kryoto itself still only acts while in front.
    sdl2::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    let sdl = match sdl2::init() {
        Ok(s) => s,
        Err(e) => return crate::logging::error("pads", &format!("controllers unavailable: {e}")),
    };
    let (js, gc, mut pump) = match (sdl.joystick(), sdl.game_controller(), sdl.event_pump()) {
        (Ok(j), Ok(g), Ok(p)) => (j, g, p),
        _ => return crate::logging::error("pads", "controllers unavailable: SDL did not start"),
    };
    // Setups saved here, before any pad is opened.
    for prefs in load_config(app).pads.values() {
        if let Some(line) = prefs.mapping.as_deref() {
            let _ = gc.add_mapping(line);
        }
    }
    let _ = &back;
    let mut r = Reader {
        app,
        js,
        gc,
        devs: HashMap::new(),
        ignored: HashSet::new(),
        capture: None,
        focused: false,
        #[cfg(windows)]
        virt: virt::Virtual::new(back),
    };
    r.open_all();

    let mut active = true;
    let mut focus_at = Instant::now() - Duration::from_secs(1);
    let mut sent: HashMap<u32, Axes> = HashMap::new();
    let mut sent_at = Instant::now();
    let mut dirty: HashSet<u32> = HashSet::new();

    loop {
        // Asks from the shell.
        loop {
            let req = if active {
                match rx.try_recv() {
                    Ok(r) => r,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            } else {
                // Paused: wait for Start without spinning.
                match rx.recv() {
                    Ok(r) => r,
                    Err(_) => return,
                }
            };
            match req {
                Req::Start => {
                    if !active {
                        active = true;
                        for _ in pump.poll_iter() {}
                    }
                    r.open_all();
                }
                Req::Stop => {
                    active = false;
                    r.close_all();
                }
                _ if !active => {}
                Req::Rumble(id) => r.rumble(id, 0xb000, 0xb000, 300),
                Req::Haptic(id, kind) => {
                    let (low, high, ms) = match kind {
                        Haptic::Tick => (0, 0x5000, 40),
                        Haptic::Select => (0x5000, 0x3000, 60),
                        Haptic::Edge => (0x9000, 0, 110),
                    };
                    r.rumble(id, low, high, ms);
                }
                Req::Capture(id) => {
                    r.capture = id;
                    if let Some(state) = id.and_then(|i| r.raw_state(i)) {
                        if let Some(d) = r.devs.get_mut(&state.id) {
                            d.raw_axes = state.axes.clone();
                        }
                        let _ = app.emit("pad-raw-state", state);
                    }
                }
                Req::Mapping(guid) => r.apply_mapping(&guid),
                Req::VirtualRumble(id, large, small) => {
                    r.rumble(id, u16::from(large) * 257, u16::from(small) * 257, 250);
                }
            }
        }
        if !active {
            continue;
        }

        if focus_at.elapsed() > Duration::from_millis(200) {
            focus_at = Instant::now();
            r.focused = app.get_window("main").and_then(|w| w.is_focused().ok()).unwrap_or(false);
        }

        if let Some(ev) = pump.wait_event_timeout(8) {
            r.handle(ev, &mut dirty);
            for ev in pump.poll_iter() {
                r.handle(ev, &mut dirty);
            }
        }

        // Sticks and triggers at most 30 times a second, while in front.
        if r.focused && sent_at.elapsed() >= Duration::from_millis(33) && !dirty.is_empty() {
            sent_at = Instant::now();
            for id in dirty.drain() {
                if let Some(a) = r.axes_of(id) {
                    if sent.get(&id) != Some(&a) {
                        sent.insert(id, a);
                        let _ = app.emit("pad-axes", a);
                    }
                }
            }
        }

        #[cfg(windows)]
        r.feed_virtual();
    }
}

#[cfg(windows)]
impl<R: Runtime> Reader<'_, R> {
    /// While a game runs: every set-up, non-Xbox pad plays as a virtual Xbox
    /// controller, with its remap. Unplugged again when the games close.
    fn feed_virtual(&mut self) {
        let cfg_on = self.virt.check_settings(|| load_config(self.app).virtual_pad);
        let playing = self.app.state::<crate::library::Running>().0.lock().map(|m| !m.is_empty()).unwrap_or(false);
        if !(cfg_on && playing) {
            self.virt.unplug_all();
            return;
        }
        let cfg = self.virt.cached_config(|| load_config(self.app));
        for (id, d) in &self.devs {
            let Some(ctl) = d.ctl.as_ref() else { continue };
            if d.info.xinput {
                continue;
            }
            let remap = cfg.pads.get(&d.info.guid).map(|p| p.remap.clone()).unwrap_or_default();
            self.virt.feed(*id, ctl, &remap);
        }
        let live: HashSet<u32> = self.devs.keys().copied().collect();
        self.virt.retain(&live);
        if let Some(found) = self.virt.driver() {
            if let Ok(mut d) = self.app.state::<Pads>().driver.lock() {
                *d = Some(found);
            }
        }
    }
}

/// The virtual Xbox controller (Windows, ViGEmBus).
#[cfg(windows)]
mod virt {
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use padmap::{Control, Remap};
    use sdl2::controller::{Axis as SAxis, Button as SButton, GameController};
    use vigem_client::{Client, TargetId, XButtons, XGamepad, Xbox360Wired};

    use super::{PadConfig, Req};

    pub struct Virtual {
        client: Option<Rc<Client>>,
        /// No driver: do not try again for a while.
        retry_at: Instant,
        driver: Option<bool>,
        targets: HashMap<u32, (Xbox360Wired<Rc<Client>>, XGamepad)>,
        /// Plugged in, not yet seen by SDL: the next such Xbox 360 pad is ours.
        expecting: Vec<Instant>,
        back: mpsc::Sender<Req>,
        settings: (Instant, bool),
        config: Option<(Instant, PadConfig)>,
    }

    impl Virtual {
        pub fn new(back: mpsc::Sender<Req>) -> Self {
            let past = Instant::now() - Duration::from_secs(60);
            Self {
                client: None,
                retry_at: past,
                driver: None,
                targets: HashMap::new(),
                expecting: Vec::new(),
                back,
                settings: (past, false),
                config: None,
            }
        }

        /// The setting, read at most once a second.
        pub fn check_settings(&mut self, read: impl FnOnce() -> bool) -> bool {
            if self.settings.0.elapsed() > Duration::from_secs(1) {
                self.settings = (Instant::now(), read());
            }
            self.settings.1
        }

        /// The remaps, read at most once a second while playing.
        pub fn cached_config(&mut self, read: impl FnOnce() -> PadConfig) -> PadConfig {
            match &self.config {
                Some((at, c)) if at.elapsed() < Duration::from_secs(1) => c.clone(),
                _ => {
                    let c = read();
                    self.config = Some((Instant::now(), c.clone()));
                    c
                }
            }
        }

        pub fn driver(&self) -> Option<bool> {
            self.driver
        }

        /// Our own targets appear to SDL as Xbox 360 pads a moment after
        /// being plugged in; those are not listed or read.
        pub fn is_ours(&mut self, vendor: u16, product: u16) -> bool {
            self.expecting.retain(|t| t.elapsed() < Duration::from_secs(3));
            if vendor == 0x045e && product == 0x028e && !self.expecting.is_empty() {
                self.expecting.remove(0);
                return true;
            }
            false
        }

        fn client(&mut self) -> Option<Rc<Client>> {
            if self.client.is_none() && Instant::now() >= self.retry_at {
                match Client::connect() {
                    Ok(c) => {
                        self.client = Some(Rc::new(c));
                        self.driver = Some(true);
                    }
                    Err(_) => {
                        self.driver = Some(false);
                        self.retry_at = Instant::now() + Duration::from_secs(10);
                    }
                }
            }
            self.client.clone()
        }

        pub fn feed(&mut self, id: u32, ctl: &GameController, remap: &Remap) {
            let report = report(ctl, remap);
            if !self.targets.contains_key(&id) {
                let Some(client) = self.client() else { return };
                let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
                if target.plugin().is_err() || target.wait_ready().is_err() {
                    return;
                }
                self.expecting.push(Instant::now());
                // Rumble from the game goes to the real pad.
                if let Ok(note) = target.request_notification() {
                    let back = self.back.clone();
                    note.spawn_thread(move |_, n| {
                        let _ = back.send(Req::VirtualRumble(id, n.large_motor, n.small_motor));
                    });
                }
                self.targets.insert(id, (target, XGamepad::default()));
            }
            if let Some((target, last)) = self.targets.get_mut(&id) {
                if *last != report && target.update(&report).is_ok() {
                    *last = report;
                }
            }
        }

        pub fn unplug(&mut self, id: u32) {
            if let Some((mut target, _)) = self.targets.remove(&id) {
                let _ = target.unplug();
            }
        }

        pub fn retain(&mut self, live: &HashSet<u32>) {
            let gone: Vec<u32> = self.targets.keys().filter(|id| !live.contains(id)).copied().collect();
            for id in gone {
                self.unplug(id);
            }
        }

        pub fn unplug_all(&mut self) {
            let all: Vec<u32> = self.targets.keys().copied().collect();
            for id in all {
                self.unplug(id);
            }
        }
    }

    fn button_of(c: Control) -> Option<SButton> {
        Some(match c {
            Control::South => SButton::A,
            Control::East => SButton::B,
            Control::West => SButton::X,
            Control::North => SButton::Y,
            Control::Select => SButton::Back,
            Control::Start => SButton::Start,
            Control::Guide => SButton::Guide,
            Control::LeftStick => SButton::LeftStick,
            Control::RightStick => SButton::RightStick,
            Control::LeftBumper => SButton::LeftShoulder,
            Control::RightBumper => SButton::RightShoulder,
            Control::DpadUp => SButton::DPadUp,
            Control::DpadDown => SButton::DPadDown,
            Control::DpadLeft => SButton::DPadLeft,
            Control::DpadRight => SButton::DPadRight,
            _ => return None,
        })
    }

    /// How far a physical control is pressed, 0..1.
    fn amount(ctl: &GameController, c: Control) -> f32 {
        match c {
            Control::LeftTrigger => (ctl.axis(SAxis::TriggerLeft) as f32 / 32767.0).clamp(0.0, 1.0),
            Control::RightTrigger => (ctl.axis(SAxis::TriggerRight) as f32 / 32767.0).clamp(0.0, 1.0),
            other => button_of(other).map(|b| if ctl.button(b) { 1.0 } else { 0.0 }).unwrap_or(0.0),
        }
    }

    /// The Xbox report for a pad: each job done by the control the remap
    /// gives it, sticks as they are.
    pub fn report(ctl: &GameController, remap: &Remap) -> XGamepad {
        let job = |c: Control| amount(ctl, remap.done_by(c));
        const BITS: [(Control, u16); 15] = [
            (Control::South, XButtons::A),
            (Control::East, XButtons::B),
            (Control::West, XButtons::X),
            (Control::North, XButtons::Y),
            (Control::LeftBumper, XButtons::LB),
            (Control::RightBumper, XButtons::RB),
            (Control::Select, XButtons::BACK),
            (Control::Start, XButtons::START),
            (Control::Guide, XButtons::GUIDE),
            (Control::LeftStick, XButtons::LTHUMB),
            (Control::RightStick, XButtons::RTHUMB),
            (Control::DpadUp, XButtons::UP),
            (Control::DpadDown, XButtons::DOWN),
            (Control::DpadLeft, XButtons::LEFT),
            (Control::DpadRight, XButtons::RIGHT),
        ];
        let mut raw = 0u16;
        for (c, bit) in BITS {
            if job(c) > 0.5 {
                raw |= bit;
            }
        }
        // SDL's y grows downwards, XInput's upwards.
        let flip = |v: i16| v.saturating_neg().max(-32767);
        XGamepad {
            buttons: XButtons { raw },
            left_trigger: (job(Control::LeftTrigger) * 255.0) as u8,
            right_trigger: (job(Control::RightTrigger) * 255.0) as u8,
            thumb_lx: ctl.axis(SAxis::LeftX),
            thumb_ly: flip(ctl.axis(SAxis::LeftY)),
            thumb_rx: ctl.axis(SAxis::RightX),
            thumb_ry: flip(ctl.axis(SAxis::RightY)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_old_files() {
        let c: PadConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.enabled && c.navigate && c.notify && c.games && c.haptics && c.virtual_pad);
        let c: PadConfig = serde_json::from_str(r#"{"pads":{"abc":{"name":"Pad","family":"nintendo","remap":{"a":"b","b":"a"}}}}"#).unwrap();
        let p = &c.pads["abc"];
        assert_eq!(p.family, Some(Family::Nintendo));
        assert_eq!(p.remap.acts_as(Control::South), Control::East);
    }

    #[test]
    fn what_games_get() {
        let base = "030000004c050000e60c000011810000,DualSense,a:b0,b:b1,x:b3,y:b2,platform:Linux,".to_string();
        // SDL knows it and nothing changed: nothing to hand over.
        let mut p = PadPrefs { base: Some(base.clone()), ..Default::default() };
        assert_eq!(sdl_line(&p), None);
        // A remap rewrites SDL's own mapping.
        p.remap.assign(Control::South, Control::East);
        let m = padmap::SdlMapping::parse(&sdl_line(&p).unwrap()).unwrap();
        assert_eq!((m.get("a"), m.get("b")), (Some("b1"), Some("b0")));
        // A setup saved here is handed over even without a remap.
        let p = PadPrefs { mapping: Some("03008231110100003414000000000000,SteelSeries,a:b0,b:b1,".into()), ..Default::default() };
        assert!(sdl_line(&p).unwrap().contains("a:b0"));
    }

    #[test]
    fn setup_sources() {
        for ok in ["b0", "b15", "h0.1", "h0.8", "a2", "+a2", "-a5", "a1~", "+a3~"] {
            assert!(valid_source(ok), "{ok}");
        }
        for bad in ["", "b", "x1", "h0.3", "+b1", "a", "b1;rm", "a99999", "h.1"] {
            assert!(!valid_source(bad), "{bad}");
        }
    }

    #[test]
    fn a_placeholder_name_gives_way() {
        let model = padmap::identify(Some(0x0111), Some(0x1434), "As: 6 knop: 16 gamepad met kapschakelaar");
        assert_eq!(display_name(&model, "As: 6 knop: 16 gamepad met kapschakelaar"), "SteelSeries controller");
        let model = padmap::identify(None, None, "8BitDo Pro 2");
        assert_eq!(display_name(&model, "8BitDo Pro 2"), "8BitDo Pro 2");
    }
}
