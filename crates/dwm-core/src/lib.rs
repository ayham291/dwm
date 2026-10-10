//! Pure dwm-style window management: clients, tags, layouts, vanity gaps.
//!
//! This crate has no Wayland/X11 dependency so layout and tagging behaviour can
//! be unit-tested in isolation. The Wayland compositor (`dwm-wayland`) drives
//! this state machine and applies the resulting geometries to surfaces.

mod config;
mod layout;
mod monitor;
mod tags;

pub use config::{Config, DEFAULT_TAGS, ModKey, ScratchpadConfig};
pub use layout::{ArrangeParams, LayoutKind, Rect, arrange};
pub use monitor::{BarModel, BarTag, Client, ClientId, Gaps, Monitor, MonitorAction};
pub use tags::{NUM_SCRATCHPADS, NUM_TAGS, TagMask, scratch_bit, tag_bit};

/// High-level commands a compositor (or test harness) can issue.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    FocusStack { delta: i32 },
    PushStack { delta: i32 },
    View { tags: TagMask },
    ToggleView { tags: TagMask },
    Tag { tags: TagMask },
    ToggleTag { tags: TagMask },
    ViewAll,
    TagAll,
    SetLayout(LayoutKind),
    CycleLayout,
    IncNMaster { delta: i32 },
    SetMFact { delta: f32 },
    ToggleFloating,
    ToggleSticky,
    ToggleGaps,
    ToggleSmartGaps,
    DefaultGaps,
    IncGaps { delta: i32 },
    ToggleBar,
    ShiftView { delta: i32 },
    /// Show/hide scratchpad `index` (0 = spterm, 1 = spcalc).
    ToggleScratch { index: usize },
    KillFocused,
    Quit,
}

/// Result of applying a command: geometries to push to the compositor, plus
/// optional side effects.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// Client geometries after arrange (visible tiled/floating clients).
    pub geometries: Vec<(ClientId, Rect)>,
    /// Layout symbol for the bar (e.g. `"[]="`, `"D 3"`).
    pub layout_symbol: String,
    /// Currently focused client, if any.
    pub focused: Option<ClientId>,
    /// Client that should be closed.
    pub kill: Option<ClientId>,
    /// Request compositor shutdown.
    pub quit: bool,
    /// Visible tag mask after the command.
    pub tagset: TagMask,
    /// Whether the status bar should be shown.
    pub show_bar: bool,
    /// Spawn this command (scratchpad not yet running).
    pub spawn: Option<Vec<String>>,
}

impl Monitor {
    /// Apply a high-level command and re-arrange.
    pub fn apply(&mut self, cmd: Command) -> Frame {
        let mut kill = None;
        let mut quit = false;
        let mut spawn = None;

        match cmd {
            Command::FocusStack { delta } => self.focus_stack(delta),
            Command::PushStack { delta } => self.push_stack(delta),
            Command::View { tags } => self.view(tags),
            Command::ToggleView { tags } => self.toggle_view(tags),
            Command::Tag { tags } => self.tag_selected(tags),
            Command::ToggleTag { tags } => self.toggle_tag_selected(tags),
            Command::ViewAll => self.view(TagMask::ALL_WORKSPACE),
            Command::TagAll => self.tag_selected(TagMask::ALL_WORKSPACE),
            Command::SetLayout(kind) => self.set_layout(kind),
            Command::CycleLayout => self.cycle_layout(),
            Command::IncNMaster { delta } => self.inc_nmaster(delta),
            Command::SetMFact { delta } => self.set_mfact(delta),
            Command::ToggleFloating => self.toggle_floating(),
            Command::ToggleSticky => self.toggle_sticky(),
            Command::ToggleGaps => self.toggle_gaps(),
            Command::ToggleSmartGaps => self.toggle_smart_gaps(),
            Command::DefaultGaps => self.default_gaps(),
            Command::IncGaps { delta } => self.inc_gaps(delta),
            Command::ToggleBar => {
                self.show_bar = !self.show_bar;
                self.recompute_window_area();
            }
            Command::ShiftView { delta } => self.shift_view(delta),
            Command::ToggleScratch { index } => spawn = self.toggle_scratch(index),
            Command::KillFocused => kill = self.sel,
            Command::Quit => quit = true,
        }

        let (geometries, layout_symbol) = self.arrange();
        Frame {
            geometries,
            layout_symbol,
            focused: self.sel,
            kill,
            quit,
            tagset: self.tagset(),
            show_bar: self.show_bar,
            spawn,
        }
    }
}
