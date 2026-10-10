//! Keybinding table mapped from the classic `config.h` (subset).

use dwm_core::{Command, LayoutKind, TagMask, tag_bit};
use smithay::input::keyboard::{Keysym, ModifiersState};

#[derive(Debug, Clone)]
pub struct Binding {
    pub mods: Mods,
    pub key: Keysym,
    pub cmd: Cmd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub logo: bool,
    pub alt: bool,
}

impl Mods {
    pub const LOGO: Self = Self {
        ctrl: false,
        shift: false,
        logo: true,
        alt: false,
    };
    pub const LOGO_SHIFT: Self = Self {
        ctrl: false,
        shift: true,
        logo: true,
        alt: false,
    };

    pub fn matches(self, state: &ModifiersState) -> bool {
        state.ctrl == self.ctrl
            && state.shift == self.shift
            && state.logo == self.logo
            && state.alt == self.alt
    }
}

/// Commands that may spawn external processes in addition to dwm-core Commands.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Core(Command),
    SpawnTerminal,
    SpawnDmenu,
}

pub fn default_bindings() -> Vec<Binding> {
    let mut out = vec![
        Binding {
            mods: Mods::LOGO,
            key: Keysym::j,
            cmd: Cmd::Core(Command::FocusStack { delta: 1 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::k,
            cmd: Cmd::Core(Command::FocusStack { delta: -1 }),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::j,
            cmd: Cmd::Core(Command::PushStack { delta: 1 }),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::k,
            cmd: Cmd::Core(Command::PushStack { delta: -1 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::v,
            cmd: Cmd::Core(Command::PushStack { delta: 0 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::t,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::Tile)),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::t,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::BottomStack)),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::y,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::Spiral)),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::y,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::Dwindle)),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::u,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::Deck)),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::u,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::Monocle)),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::i,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::CenteredMaster)),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::i,
            cmd: Cmd::Core(Command::SetLayout(LayoutKind::CenteredFloatingMaster)),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::f,
            cmd: Cmd::Core(Command::ToggleFloating),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::s,
            cmd: Cmd::Core(Command::ToggleSticky),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::a,
            cmd: Cmd::Core(Command::ToggleGaps),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::a,
            cmd: Cmd::Core(Command::DefaultGaps),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::o,
            cmd: Cmd::Core(Command::IncNMaster { delta: 1 }),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::o,
            cmd: Cmd::Core(Command::IncNMaster { delta: -1 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::h,
            cmd: Cmd::Core(Command::SetMFact { delta: -0.05 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::l,
            cmd: Cmd::Core(Command::SetMFact { delta: 0.05 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::g,
            cmd: Cmd::Core(Command::ShiftView { delta: -1 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::semicolon,
            cmd: Cmd::Core(Command::ShiftView { delta: 1 }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::Tab,
            cmd: Cmd::Core(Command::View {
                tags: TagMask::EMPTY,
            }),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::q,
            cmd: Cmd::Core(Command::KillFocused),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::q,
            cmd: Cmd::Core(Command::Quit),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::b,
            cmd: Cmd::Core(Command::ToggleBar),
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::Return,
            cmd: Cmd::SpawnTerminal,
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::d,
            cmd: Cmd::SpawnDmenu,
        },
        Binding {
            mods: Mods::LOGO,
            key: Keysym::_0,
            cmd: Cmd::Core(Command::ViewAll),
        },
        Binding {
            mods: Mods::LOGO_SHIFT,
            key: Keysym::_0,
            cmd: Cmd::Core(Command::TagAll),
        },
    ];

    let tag_keys = [
        Keysym::_1,
        Keysym::_2,
        Keysym::_3,
        Keysym::_4,
        Keysym::_5,
        Keysym::_6,
        Keysym::_7,
        Keysym::_8,
        Keysym::_9,
    ];
    for (i, key) in tag_keys.into_iter().enumerate() {
        let tags = tag_bit(i as u32);
        out.push(Binding {
            mods: Mods::LOGO,
            key,
            cmd: Cmd::Core(Command::View { tags }),
        });
        out.push(Binding {
            mods: Mods::LOGO_SHIFT,
            key,
            cmd: Cmd::Core(Command::Tag { tags }),
        });
    }

    out
}

pub fn lookup(bindings: &[Binding], mods: &ModifiersState, key: Keysym) -> Option<Cmd> {
    bindings
        .iter()
        .find(|b| b.key == key && b.mods.matches(mods))
        .map(|b| b.cmd.clone())
}
