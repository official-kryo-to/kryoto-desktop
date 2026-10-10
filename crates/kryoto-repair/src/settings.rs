use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emulator {
    /// Offline. The default, and correct for almost every release.
    #[default]
    GbeFork,
    /// Online play, via the Spacewar spoof. Requires a running Steam client.
    Online,
    /// RUNE, in its three flavours.
    ///
    /// Three variants rather than one carrying a profile field, because the
    /// profiles are not a parameter of one thing - they ship as separate
    /// downloads with different config filenames and different runtime
    /// behaviour, and the release label has to name which was used. Three
    /// fieldless variants keep `Emulator` `Copy` and make every match over it
    /// exhaustive at compile time, which is what stops a fourth emulator being
    /// half-added.
    ///
    /// Unlike gbe_fork, RUNE does not simply replace the Steamworks dll: it
    /// RENAMES the original to `steam_api64.rne` and forwards to it, so that
    /// file is part of the release rather than a backup, and must ship.
    Rune,
    /// RUNE's Steakclient profile. Its own config file, `steak_emu.ini`.
    RuneSteak,
    /// RUNE's Steamclient profile.
    RuneSteamclient,
    /// Nothing automatic: the pipeline stops with the tree on disk and waits
    /// for a person to patch it by hand.
    ///
    /// The two emulators above cover almost everything, and when they do not
    /// the alternative used to be abandoning the run and doing the download,
    /// the scan and the archive again by hand somewhere else. This keeps all of
    /// that: the download is done, the DRM scan is done, and the only step
    /// handed back is the one no tool here can do. What was applied is typed in
    /// afterwards and becomes the release's `source` label, so the catalogue
    /// says what is actually in the build rather than guessing.
    Custom,
}
impl Emulator {
    /// The name used in the release's `source` label on kryo.to, so a visitor
    /// can tell an online build from an offline one before downloading it.
    ///
    /// `Custom` has no fixed name - whatever was applied by hand is typed in at
    /// the pause and carried in `PipelineConfig::custom_emulator_label`, which
    /// is what the label ends up reading. This is the fallback for a build that
    /// somehow reaches a label with nothing typed.
    pub fn label(self) -> &'static str {
        match self {
            Emulator::GbeFork => "gbe_fork",
            Emulator::Online => "Kryoto Online",
            Emulator::Rune => "RUNE",
            Emulator::RuneSteak => "RUNE (Steakclient)",
            Emulator::RuneSteamclient => "RUNE (Steamclient)",
            Emulator::Custom => "custom emulator",
        }
    }

    /// Is this one of the RUNE profiles?
    pub fn is_rune(self) -> bool {
        matches!(
            self,
            Emulator::Rune | Emulator::RuneSteak | Emulator::RuneSteamclient
        )
    }

    /// The config file this RUNE profile reads, or None for anything else.
    ///
    /// Steakclient's is a DIFFERENT FILE NAME, and writing the wrong one leaves
    /// a template full of placeholders that the emulator ignores - a failure
    /// that only shows up on the player's machine.
    pub fn rune_ini_name(self) -> Option<&'static str> {
        match self {
            Emulator::Rune | Emulator::RuneSteamclient => Some("steam_emu.ini"),
            Emulator::RuneSteak => Some("steak_emu.ini"),
            _ => None,
        }
    }

    /// Does the pipeline apply this itself?
    ///
    /// False for `Custom`, which is the whole point of it: the crack stage is
    /// skipped, the tree is left alone, and the run stops so somebody can do it.
    pub fn is_automatic(self) -> bool {
        !matches!(self, Emulator::Custom)
    }
}
