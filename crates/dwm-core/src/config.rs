use crate::layout::LayoutKind;
use crate::monitor::Gaps;

pub const DEFAULT_TAGS: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Modifier key used for bindings (Super/Mod4 by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKey {
    Super,
    Alt,
}

/// One scratchpad entry (classic `Sp` / `scratchpads[]` from `config.h`).
#[derive(Debug, Clone)]
pub struct ScratchpadConfig {
    /// Short name used for matching (`spterm`, `spcalc`).
    pub name: String,
    /// Substring matched against the client's title or app_id.
    pub match_title: String,
    /// Command used to spawn the scratchpad if it isn't running.
    pub command: Vec<String>,
    /// Floating size when shown.
    pub width: i32,
    pub height: i32,
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
    pub bar_height: i32,
    pub mfact: f32,
    pub nmaster: usize,
    pub lock_fullscreen: bool,
    pub default_layout: LayoutKind,
    pub terminal: String,
    pub browser: String,
    pub mod_key: ModKey,
    pub tags: Vec<String>,
    pub scratchpads: Vec<ScratchpadConfig>,
    /// Path to a UTF-8 status file (dwmblocks-style). Read periodically.
    pub status_file: String,
    pub norm_bg: String,
    pub norm_fg: String,
    pub norm_border: String,
    pub sel_bg: String,
    pub sel_fg: String,
    pub sel_border: String,
}

impl Default for Config {
    fn default() -> Self {
        let terminal =
            std::env::var("DWM_WAYLAND_TERM").unwrap_or_else(|_| "ghostty".to_string());
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
            bar_height: 24,
            mfact: 0.55,
            nmaster: 1,
            lock_fullscreen: true,
            default_layout: LayoutKind::Tile,
            browser: "zen-browser".into(),
            mod_key: ModKey::Super,
            tags: DEFAULT_TAGS.iter().map(|s| (*s).to_string()).collect(),
            scratchpads: vec![
                ScratchpadConfig {
                    name: "spterm".into(),
                    match_title: "spterm".into(),
                    // Modest grid so nested/small outputs still float with margins;
                    // compositor configure + clip enforce the pixel size.
                    command: scratch_spawn(&terminal, "spterm", &[], 80, 24),
                    width: 960,
                    height: 640,
                },
                ScratchpadConfig {
                    name: "spcalc".into(),
                    match_title: "spcalc".into(),
                    command: scratch_spawn(
                        &terminal,
                        "spcalc",
                        &["-e", "bc", "-lq"],
                        50,
                        20,
                    ),
                    width: 640,
                    height: 400,
                },
            ],
            status_file: default_status_path(),
            terminal,
            norm_bg: "#222222".into(),
            norm_fg: "#bbbbbb".into(),
            norm_border: "#444444".into(),
            sel_bg: "#4C4C4C".into(),
            sel_fg: "#eeeeee".into(),
            sel_border: "#E0AA00".into(),
        }
    }
}

/// Build a terminal spawn line that sets the window title for scratchpad matching.
fn scratch_spawn(
    terminal: &str,
    title: &str,
    extra: &[&str],
    cols: u32,
    rows: u32,
) -> Vec<String> {
    let base = terminal.rsplit('/').next().unwrap_or(terminal);
    let mut cmd = match base {
        "foot" => vec![
            terminal.to_string(),
            "-T".into(),
            title.into(),
            "-W".into(),
            format!("{cols}x{rows}"),
        ],
        _ => {
            // ghostty / xterm-style
            vec![
                terminal.to_string(),
                format!("--title={title}"),
                format!("--window-width={cols}"),
                format!("--window-height={rows}"),
            ]
        }
    };
    if extra.is_empty() {
        // Keep an interactive shell so the pad doesn't exit immediately.
        match base {
            "foot" => cmd.extend(["-e".into(), "sh".into()]),
            _ => {}
        }
    } else {
        cmd.extend(extra.iter().map(|s| (*s).to_string()));
    }
    cmd
}

fn default_status_path() -> String {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        format!("{runtime}/dwm-wayland.status")
    } else {
        "/tmp/dwm-wayland.status".into()
    }
}
