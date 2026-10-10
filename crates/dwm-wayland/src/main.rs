//! dwm-wayland — a dwm-inspired tiling compositor for Wayland.
//!
//! Built on [smithay](https://github.com/Smithay/smithay) with window-management
//! logic in the `dwm-core` crate (layouts, tags, vanity gaps, stacker, …).
//!
//! Run nested under an existing X11/Wayland session:
//!
//! ```text
//! cargo run -p dwm-wayland --no-default-features
//! ```
//!
//! Headless smoke-test (protocols only, no GPU loop):
//!
//! ```text
//! cargo run -p dwm-wayland -- --check
//! ```
//!
//! Experimental DRM/libseat TTY session (feature `session`; use a spare VT):
//!
//! ```text
//! cargo run -p dwm-wayland -- --session
//! ```

mod bar;
mod bindings;
mod color;
mod grabs;
mod input;
mod state;

#[cfg(feature = "session")]
mod backends;

#[cfg(feature = "xwayland")]
mod xwayland;

use std::env;
use std::process;

use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();

    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    if args.iter().any(|a| a == "--check") {
        match state::check_protocols() {
            Ok(msg) => {
                println!("{msg}");
                process::exit(0);
            }
            Err(e) => {
                eprintln!("check failed: {e}");
                process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--session") {
        #[cfg(feature = "session")]
        {
            if let Err(e) = backends::udev::run() {
                eprintln!("dwm-wayland session error: {e}");
                process::exit(1);
            }
            return;
        }
        #[cfg(not(feature = "session"))]
        {
            eprintln!("--session requires the 'session' cargo feature");
            process::exit(1);
        }
    }

    #[cfg(feature = "xwayland")]
    xwayland::announce();

    if let Err(e) = state::run() {
        eprintln!("dwm-wayland error: {e}");
        process::exit(1);
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn print_help() {
    println!(
        "\
dwm-wayland — dwm-inspired tiling Wayland compositor

USAGE:
    dwm-wayland [OPTIONS]

OPTIONS:
    --check     Initialize Wayland globals and dwm-core state, then exit
    --session   Experimental DRM/libseat TTY session (feature: session; spare VT)
    -h, --help  Show this help

KEY BINDINGS (Super / Mod4, or Alt in nested sessions):
    j / k           Focus next / previous client
    Shift+j / k     Push client down / up the stack
    1..9            View tag
    Shift+1..9      Tag focused client
    t / Shift+t     Tile / bottomstack
    y / Shift+y     Spiral / dwindle
    u / Shift+u     Deck / monocle
    i / Shift+i     Centered master / centered floating master
    f               Toggle floating
    s               Toggle sticky
    a               Toggle gaps
    o / Shift+o     Inc / dec nmaster
    h / l           Shrink / grow master factor
    q               Kill focused client
    Shift+q         Quit
    Return          Spawn $TERMINAL (default: ghostty)
    Shift+Return    Toggle spterm scratchpad
    '               Toggle spcalc scratchpad
    b               Toggle status bar
    d               Spawn dmenu_run

STATUS:
    Write a one-line UTF-8 string to $XDG_RUNTIME_DIR/dwm-wayland.status
"
    );
}
