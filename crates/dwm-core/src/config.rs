use crate::layout::LayoutKind;
use crate::monitor::Gaps;

pub const DEFAULT_TAGS: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Modifier key used for bindings (Super/Mod4 by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKey {
    Super,
    Alt,
}

/// Runtime configuration mirrored from the classic `config.h`.
#[derive(Debug, Clone)]
pub struct Config {
    pub border_px: u32,
    pub snap: u32,
    pub gaps: Gaps,
    pub smart_gaps: bool,
    pub show_bar: bool,
    pub top_bar: bool,
    pub mfact: f32,
    pub nmaster: usize,
    pub lock_fullscreen: bool,
    pub default_layout: LayoutKind,
    pub terminal: String,
    pub browser: String,
    pub mod_key: ModKey,
    pub tags: Vec<String>,
    pub norm_bg: String,
    pub norm_fg: String,
    pub norm_border: String,
    pub sel_bg: String,
    pub sel_fg: String,
    pub sel_border: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            border_px: 3,
            snap: 32,
            gaps: Gaps {
                inner_h: 20,
                inner_v: 10,
                outer_h: 10,
                outer_v: 30,
            },
            smart_gaps: false,
            show_bar: true,
            top_bar: true,
            mfact: 0.55,
            nmaster: 1,
            lock_fullscreen: true,
            default_layout: LayoutKind::Tile,
            terminal: "ghostty".into(),
            browser: "zen-browser".into(),
            mod_key: ModKey::Super,
            tags: DEFAULT_TAGS.iter().map(|s| (*s).to_string()).collect(),
            norm_bg: "#222222".into(),
            norm_fg: "#bbbbbb".into(),
            norm_border: "#444444".into(),
            sel_bg: "#4C4C4C".into(),
            sel_fg: "#eeeeee".into(),
            sel_border: "#E0AA00".into(),
        }
    }
}
