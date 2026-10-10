//! Turning a library entry into a process: which exe, from where, with what.
//!
//! Pure on purpose - everything here is a function of the saved game and the
//! host, so the exact command a Play press runs can be previewed in the
//! Properties window and tested without starting anything.
//!
//! ## The launch options line
//!
//! The same grammar Steam uses in a game's LAUNCH OPTIONS box, so a line
//! copied from a guide works unchanged:
//!
//! ```text
//! WINEDLLOVERRIDES="steam_api64=n,b" gamemoderun %command% -nohmd
//! ^ environment                      ^ wrapper   ^ the game ^ its arguments
//! ```
//!
//! Without `%command%` the whole line is extra arguments, as in Steam - except
//! that leading `NAME=value` words are still read as environment, because a
//! pasted override with the `%command%` forgotten should not become arguments
//! the game chokes on.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// One of Steam's launch entries, as kryo.to stores it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LaunchEntry {
    pub executable: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(default)]
    pub workingdir: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub oslist: String,
    /// Steam's kind: `default`, `vr`, `config`... Empty when unknown.
    #[serde(default, rename = "type")]
    pub kind: String,
}

impl LaunchEntry {
    pub fn is_windows(&self) -> bool {
        let os = self.oslist.to_ascii_lowercase();
        os.is_empty() || os.contains("windows")
    }
}

/// What a Play press runs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    /// The program started: the game, or Wine/Proton, or a wrapper.
    pub program: PathBuf,
    /// Arguments before the game's own (a wrapper's rest, `run` for Proton,
    /// the exe for Wine).
    pub lead_args: Vec<String>,
    /// The game's arguments as one string. Handed to Windows as a raw command
    /// line, exactly as Steam does, and split like a shell elsewhere.
    pub game_args: String,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub exe: PathBuf,
}

impl LaunchPlan {
    /// The whole thing on one line, for the Properties preview.
    pub fn display(&self) -> String {
        let quote = |s: &str| {
            if s.contains(' ') {
                format!("\"{s}\"")
            } else {
                s.to_string()
            }
        };
        let mut parts: Vec<String> = self.env.iter().map(|(k, v)| format!("{k}={}", quote(v))).collect();
        parts.push(quote(&self.program.to_string_lossy()));
        parts.extend(self.lead_args.iter().map(|a| quote(a)));
        if !self.game_args.is_empty() {
            parts.push(self.game_args.clone());
        }
        parts.join(" ")
    }
}

/// A launch options line, taken apart.
#[derive(Debug, Default, PartialEq)]
pub struct ParsedLine {
    pub env: Vec<(String, String)>,
    pub wrappers: Vec<String>,
    /// Raw text after `%command%` (or the whole line), quoting kept.
    pub extra_args: String,
}

fn is_env_word(word: &str) -> Option<(String, String)> {
    let (name, value) = word.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

/// Split on whitespace with double quotes grouping, quotes removed.
pub fn split_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(word);
    }
    out
}

/// Byte length of the leading `NAME=value` words, quotes respected.
fn leading_env_len(text: &str) -> usize {
    let mut end = 0;
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let skipped = rest.len() - trimmed.len();
        // One word, quote-aware.
        let mut quoted = false;
        let mut len = 0;
        for (i, c) in trimmed.char_indices() {
            if c == '"' {
                quoted = !quoted;
            } else if c.is_whitespace() && !quoted {
                len = i;
                break;
            }
            len = i + c.len_utf8();
        }
        if len == 0 {
            return end;
        }
        let word: String = trimmed[..len].chars().filter(|c| *c != '"').collect();
        if is_env_word(&word).is_none() {
            return end;
        }
        end += skipped + len;
        rest = &trimmed[len..];
    }
}

pub fn parse_line(line: &str) -> ParsedLine {
    let line = line.trim();
    let mut parsed = ParsedLine::default();
    let (prefix, extra) = match line.find("%command%") {
        Some(at) => (&line[..at], line[at + "%command%".len()..].trim()),
        None => {
            let env_len = leading_env_len(line);
            (&line[..env_len], line[env_len..].trim())
        }
    };
    for word in split_words(prefix) {
        match is_env_word(&word) {
            Some(pair) if parsed.wrappers.is_empty() => parsed.env.push(pair),
            _ => parsed.wrappers.push(word),
        }
    }
    parsed.extra_args = extra.to_string();
    parsed
}

/// DLL overrides Wine needs for the online layer a release ships with - keyed
/// off the release's source label, where kryo.to records the layer.
pub fn wine_overrides_for(source: Option<&str>) -> Option<&'static str> {
    let s = source.unwrap_or("").to_ascii_lowercase();
    if s.contains("kryoto online") {
        Some("steam_api64=n,b;steam_api=n,b;kryotoO=n,b;kryotoO32=n,b;photon_universal=n,b")
    } else if s.contains("steakclient") {
        Some("steam_api64=n,b;steam_api=n,b;winmm=n,b;steakclient64=n,b")
    } else if s.contains("rune") && s.contains("steamclient") {
        Some("steam_api64=n,b;steam_api=n,b;steamclient64=n,b;steamclient=n,b;rune64=n,b;rune=n,b;GameOverlayRenderer64=n,b;GameOverlayRenderer=n,b")
    } else if s.contains("online-fix") || s.contains("onlinefix") || s.split_whitespace().any(|w| w == "ofme") {
        Some("OnlineFix64=n;SteamOverlay64=n;winmm=n,b;dnet=n;steam_api64=n")
    } else {
        None
    }
}

/// Join a relative path from Steam or the user onto the install folder,
/// refusing anything that climbs out of it.
pub fn inside(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = relative.replace('\\', "/");
    let rel = rel.trim_matches('/');
    let path = Path::new(rel);
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(format!("{relative} points outside the game's folder."));
    }
    Ok(if rel.is_empty() { root.to_path_buf() } else { root.join(path) })
}

/// Everything `plan` needs from a saved game.
pub struct PlanInput<'a> {
    pub install_dir: &'a Path,
    /// The exe to use when no Steam entry is picked, relative to the folder.
    pub executable: &'a str,
    /// Arguments the release is set up with (kryo.to's pick), used with
    /// `executable` when no entry is picked.
    pub default_args: &'a str,
    pub entry: Option<&'a LaunchEntry>,
    pub launch_options: &'a str,
    /// Linux/macOS: Wine or Proton to run the Windows exe with.
    pub compat_tool: Option<&'a Path>,
    /// Where this game's Wine prefix lives.
    pub prefix_dir: &'a Path,
    pub source: Option<&'a str>,
    pub apply_overrides: bool,
    pub windows_host: bool,
    /// Linux: umu-run, when there is one. A Proton then runs through it, inside
    /// the Steam Linux Runtime, the way Steam runs it.
    pub umu: Option<&'a Path>,
    /// Linux: wrappers from Settings (gamemoderun, mangohud), before the line's own.
    pub wrappers: Vec<String>,
    /// Linux: environment from Settings; the launch options line still wins.
    pub env: Vec<(String, String)>,
}

pub fn plan(input: &PlanInput) -> Result<LaunchPlan, String> {
    let (exe_rel, entry_args, workdir) = match input.entry {
        Some(e) => (e.executable.as_str(), e.arguments.trim(), e.workingdir.as_str()),
        None => (input.executable, input.default_args.trim(), ""),
    };
    if exe_rel.trim().is_empty() {
        return Err("No executable is set for this game. Pick one in Properties.".into());
    }
    let exe = inside(input.install_dir, exe_rel)?;
    let cwd = if workdir.trim().is_empty() {
        exe.parent().map(Path::to_path_buf).unwrap_or_else(|| input.install_dir.to_path_buf())
    } else {
        inside(input.install_dir, workdir)?
    };

    let line = parse_line(input.launch_options);
    let game_args = [entry_args, line.extra_args.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let mut env = line.env.clone();

    let mut chain: Vec<String> = Vec::new();
    if !input.windows_host {
        // Wrappers (gamemoderun, mangohud) are a Linux idea; Windows has no
        // use for them and would fail trying to start one.
        let base = |w: &str| Path::new(w).file_name().map(|n| n.to_os_string());
        for w in &input.wrappers {
            if !line.wrappers.iter().any(|l| base(l) == base(w)) {
                chain.push(w.clone());
            }
        }
        chain.extend(line.wrappers.iter().cloned());
        for (k, v) in &input.env {
            set_default(&mut env, k, v);
        }
        let tool = input.compat_tool.ok_or(
            "This is a Windows game. Get Proton in Settings, Compatibility, or pick Wine or Proton under Properties, Compatibility.",
        )?;
        let kind = crate::compat::kind_of(tool);
        let through_umu = kind == "proton" && input.umu.is_some();
        if through_umu {
            chain.push(input.umu.unwrap_or(tool).to_string_lossy().into_owned());
        } else {
            chain.push(tool.to_string_lossy().into_owned());
        }
        if through_umu {
            // Steam's way: the Proton picked, run by umu inside the runtime.
            let proton_dir = tool.parent().unwrap_or(tool);
            set_default(&mut env, "PROTONPATH", &proton_dir.to_string_lossy());
            set_default(&mut env, "WINEPREFIX", &input.prefix_dir.to_string_lossy());
            set_default(&mut env, "GAMEID", "0");
        } else if kind == "umu" {
            // umu-run is Proton outside Steam: a prefix and a game id is all it needs.
            set_default(&mut env, "WINEPREFIX", &input.prefix_dir.to_string_lossy());
            set_default(&mut env, "GAMEID", "0");
        } else if kind == "proton" {
            chain.push("run".into());
            set_default(&mut env, "STEAM_COMPAT_DATA_PATH", &input.prefix_dir.to_string_lossy());
            
            // FIX: Dynamically resolve the correct Steam client installation path.
            // Required because Arch Linux / CachyOS use ~/.local/share/Steam instead of ~/.steam/steam
            // which causes "bare" Proton to fail silently.
            let client = std::env::var("HOME")
                .map(|h| {
                    let arch_path = format!("{h}/.local/share/Steam");
                    let flatpak_path = format!("{h}/.var/app/com.valvesoftware.Steam/.local/share/Steam");
                    let ubuntu_path = format!("{h}/.steam/steam");

                    if Path::new(&arch_path).exists() {
                        arch_path
                    } else if Path::new(&flatpak_path).exists() {
                        flatpak_path
                    } else {
                        ubuntu_path // Fallback to default Ubuntu path
                    }
                })
                .unwrap_or_else(|_| "/tmp".into());
                
            set_default(&mut env, "STEAM_COMPAT_CLIENT_INSTALL_PATH", &client);
        } else {
            set_default(&mut env, "WINEPREFIX", &input.prefix_dir.to_string_lossy());
        }
        if input.apply_overrides {
            if let Some(o) = wine_overrides_for(input.source) {
                set_default(&mut env, "WINEDLLOVERRIDES", o);
            }
        }
        chain.push(exe.to_string_lossy().into_owned());
    }

    let (program, lead_args) = if chain.is_empty() {
        (exe.clone(), Vec::new())
    } else {
        (PathBuf::from(&chain[0]), chain[1..].to_vec())
    };
    Ok(LaunchPlan { program, lead_args, game_args, env, cwd, exe })
}

/// An explicit value in the launch options line wins over ours.
fn set_default(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if !env.iter().any(|(k, _)| k == key) {
        env.push((key.to_string(), value.to_string()));
    }
}

/// Build the `Command` for a plan.
pub fn command(plan: &LaunchPlan) -> std::process::Command {
    let mut cmd = std::process::Command::new(&plan.program);
    cmd.args(&plan.lead_args);
    if !plan.game_args.is_empty() {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.raw_arg(&plan.game_args);
        }
        #[cfg(not(windows))]
        {
            cmd.args(split_words(&plan.game_args));
        }
    }
    cmd.envs(plan.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    cmd.current_dir(&plan.cwd);
    // Its own process group, so Stop ends Wine/Proton and everything they
    // started, not just the wrapper.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(exe: &str, args: &str) -> LaunchEntry {
        LaunchEntry { executable: exe.into(), arguments: args.into(), ..Default::default() }
    }

    fn input<'a>(entry: Option<&'a LaunchEntry>, line: &'a str, windows: bool) -> PlanInput<'a> {
        PlanInput {
            install_dir: Path::new("/games/Captain Hardcore"),
            executable: "Captain Hardcore.exe",
            default_args: "",
            entry,
            launch_options: line,
            compat_tool: if windows { None } else { Some(Path::new("/usr/bin/wine")) },
            prefix_dir: Path::new("/data/prefixes/captain-hardcore"),
            source: Some("Steam + Kryoto Online"),
            apply_overrides: true,
            windows_host: windows,
            umu: None,
            wrappers: Vec::new(),
            env: Vec::new(),
        }
    }

    #[test]
    fn steam_lines_come_apart_like_steam_reads_them() {
        let p = parse_line(r#"WINEDLLOVERRIDES="steam_api64=n,b;kryotoO=n,b" gamemoderun %command% -nohmd --mod "My Mod""#);
        assert_eq!(p.env, vec![("WINEDLLOVERRIDES".into(), "steam_api64=n,b;kryotoO=n,b".into())]);
        assert_eq!(p.wrappers, vec!["gamemoderun".to_string()]);
        assert_eq!(p.extra_args, r#"-nohmd --mod "My Mod""#);

        // No %command%: arguments, as in Steam...
        assert_eq!(parse_line("-windowed -novid").extra_args, "-windowed -novid");
        // ...but a pasted override is still not handed to the game.
        let forgot = parse_line(r#"DXVK_HUD=1 A="b c" -x"#);
        assert_eq!(forgot.env.len(), 2);
        assert_eq!(forgot.env[1], ("A".into(), "b c".into()));
        assert_eq!(forgot.extra_args, "-x");
        assert_eq!(parse_line("   "), ParsedLine::default());
    }

    #[test]
    fn a_picked_entry_brings_its_arguments_and_the_line_adds_to_them() {
        let desktop = entry("Captain Hardcore.exe", "-nohmd");
        let p = plan(&input(Some(&desktop), "-screen-fullscreen 0", true)).unwrap();
        assert_eq!(p.program, Path::new("/games/Captain Hardcore").join("Captain Hardcore.exe"));
        assert_eq!(p.game_args, "-nohmd -screen-fullscreen 0");
        assert_eq!(p.cwd, Path::new("/games/Captain Hardcore"));
        assert!(p.env.is_empty(), "no Wine on Windows, so no overrides");
    }

    #[test]
    fn without_an_entry_the_release_default_is_used() {
        let mut i = input(None, "", true);
        i.default_args = "-nohmd";
        assert_eq!(plan(&i).unwrap().game_args, "-nohmd");
    }

    #[test]
    fn linux_runs_it_through_wine_with_the_release_overrides() {
        let desktop = entry("Captain Hardcore.exe", "-nohmd");
        let p = plan(&input(Some(&desktop), "gamemoderun %command%", false)).unwrap();
        assert_eq!(p.program, PathBuf::from("gamemoderun"));
        assert_eq!(p.lead_args[0], "/usr/bin/wine");
        assert!(p.lead_args[1].ends_with("Captain Hardcore.exe"));
        assert_eq!(p.game_args, "-nohmd");
        let env: std::collections::HashMap<_, _> = p.env.iter().cloned().collect();
        assert_eq!(env["WINEDLLOVERRIDES"], "steam_api64=n,b;kryotoO=n,b;photon_universal=n,b");
        assert_eq!(env["WINEPREFIX"], "/data/prefixes/captain-hardcore");
    }

    #[test]
    fn a_users_own_override_wins_and_proton_gets_its_variables() {
        let mut i = input(None, r#"WINEDLLOVERRIDES="winmm=n,b" %command%"#, false);
        i.compat_tool = Some(Path::new("/opt/GE-Proton9/proton"));
        let p = plan(&i).unwrap();
        assert_eq!(p.lead_args[0], "run");
        let env: std::collections::HashMap<_, _> = p.env.iter().cloned().collect();
        assert_eq!(env["WINEDLLOVERRIDES"], "winmm=n,b");
        assert_eq!(env["STEAM_COMPAT_DATA_PATH"], "/data/prefixes/captain-hardcore");
        assert!(env.contains_key("STEAM_COMPAT_CLIENT_INSTALL_PATH"));
    }

    #[test]
    fn a_proton_runs_through_umu_with_the_settings_wrappers() {
        let mut i = input(None, "gamemoderun %command%", false);
        i.compat_tool = Some(Path::new("/data/compat/GE-Proton10-3/proton"));
        i.umu = Some(Path::new("/data/compat/umu/umu-run"));
        i.wrappers = vec!["/usr/bin/gamemoderun".into(), "/usr/bin/mangohud".into()];
        i.env = vec![("WINE_FULLSCREEN_FSR".into(), "1".into())];
        let p = plan(&i).unwrap();
        // gamemoderun only once: the line already has it.
        assert_eq!(p.program, PathBuf::from("/usr/bin/mangohud"));
        assert_eq!(p.lead_args[0], "gamemoderun");
        assert_eq!(p.lead_args[1], "/data/compat/umu/umu-run");
        let env: std::collections::HashMap<_, _> = p.env.iter().cloned().collect();
        assert_eq!(env["PROTONPATH"], "/data/compat/GE-Proton10-3");
        assert_eq!(env["WINE_FULLSCREEN_FSR"], "1");
        assert!(!env.contains_key("STEAM_COMPAT_DATA_PATH"));
    }

    #[test]
    fn linux_without_a_tool_says_what_to_do() {
        let mut i = input(None, "", false);
        i.compat_tool = None;
        assert!(plan(&i).unwrap_err().contains("Wine or Proton"));
    }

    #[test]
    fn paths_cannot_climb_out_of_the_game_folder() {
        let root = Path::new("/games/x");
        assert!(inside(root, "../../etc/passwd").is_err());
        assert!(inside(root, "/abs.exe").is_ok(), "a leading slash is trimmed, not obeyed");
        assert_eq!(inside(root, "bin\\win64\\G.exe").unwrap(), root.join("bin/win64/G.exe"));
        let mut i = input(None, "", true);
        i.executable = "..\\evil.exe";
        assert!(plan(&i).is_err());
    }

    /// A real process, started the way Play starts it. Run by hand against a
    /// stand-in game that writes its arguments and folder to `launch-log.txt`:
    ///
    /// `KRYOTO_STAND_IN=<folder with "Captain Hardcore.exe"> cargo test -- --ignored`
    #[test]
    #[ignore]
    fn captain_hardcore_desktop_mode_really_gets_nohmd() {
        let dir = PathBuf::from(std::env::var("KRYOTO_STAND_IN").expect("KRYOTO_STAND_IN"));
        let log = dir.join("launch-log.txt");
        let _ = std::fs::remove_file(&log);
        // Captain Hardcore's entries as kryo.to serves them.
        let vr = LaunchEntry { executable: "Captain Hardcore.exe".into(), kind: "vr".into(), oslist: "windows".into(), ..Default::default() };
        let desktop = LaunchEntry {
            executable: "Captain Hardcore.exe".into(),
            arguments: "-nohmd".into(),
            description: "Captain Hardcore Desktop Mode".into(),
            oslist: "windows".into(),
            kind: "default".into(),
            ..Default::default()
        };
        let run = |entry: &LaunchEntry, line: &str| {
            let p = plan(&PlanInput {
                install_dir: &dir,
                executable: "Captain Hardcore.exe",
                default_args: "",
                entry: Some(entry),
                launch_options: line,
                compat_tool: None,
                prefix_dir: Path::new("unused"),
                source: Some("Steam (DRM-free)"),
                apply_overrides: true,
                windows_host: cfg!(windows),
                umu: None,
                wrappers: Vec::new(),
                env: Vec::new(),
            })
            .unwrap();
            let mut child = command(&p).spawn().unwrap();
            // The stand-in writes first and then waits; do not wait for it.
            for _ in 0..50 {
                if std::fs::read_to_string(&log).map(|s| s.lines().count()).unwrap_or(0) > 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let _ = child.kill();
            let _ = child.wait();
            let text = std::fs::read_to_string(&log).unwrap();
            let _ = std::fs::remove_file(&log);
            text
        };
        let flat = run(&desktop, "");
        assert!(flat.contains(r#"args=["-nohmd"]"#), "{flat}");
        assert!(flat.to_lowercase().contains(&dir.to_string_lossy().to_lowercase()), "started in its own folder: {flat}");
        let headset = run(&vr, "");
        assert!(headset.contains("args=[]"), "{headset}");
        let extra = run(&desktop, r#"-screen-width 1280 --name "Big Boss""#);
        assert!(extra.contains(r#"args=["-nohmd", "-screen-width", "1280", "--name", "Big Boss"]"#), "{extra}");
    }

    #[test]
    fn overrides_follow_the_release_source() {
        assert!(wine_overrides_for(Some("Steam + Kryoto Online + Steamless")).unwrap().contains("kryotoO"));
        assert!(wine_overrides_for(Some("Steam + online-fix")).unwrap().contains("OnlineFix64"));
        assert!(wine_overrides_for(Some("OFME")).unwrap().contains("OnlineFix64"));
        assert_eq!(wine_overrides_for(Some("Steam + gbe_fork")), None);
        assert_eq!(wine_overrides_for(None), None);
        assert!(wine_overrides_for(Some("Steam + RUNE steakclient")).unwrap().contains("winmm=n,b"));
        assert!(wine_overrides_for(Some("Steam + RUNE steamclient")).unwrap().contains("rune64=n,b"));
    }
}
