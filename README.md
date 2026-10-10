# dwm-wayland (Rust)

A [dwm](https://dwm.suckless.org/)-inspired **tiling Wayland compositor** rewritten in Rust.

Window-management logic (tags, layouts, vanity gaps, stacker, shiftview, sticky, …)
lives in a pure library crate so it can be tested without a GPU. The compositor
binary uses [smithay](https://github.com/Smithay/smithay) for the Wayland frontend
and the winit backend for nested sessions.

Clients are asked for **server-side decorations** (no CSD title bars / close
buttons). Chrome is classic dwm: a colored border (`selborder` / `normborder`)
around each tiled window.

The original X11 C sources are preserved under [`legacy/`](legacy/).

## Crates

| Crate | Role |
|-------|------|
| `dwm-core` | Pure WM state machine + layout algorithms |
| `dwm-wayland` | Wayland compositor (`smithay` + winit / optional DRM session) |

## Configuration

Still edit-and-recompile (suckless-style), split across:

| Concern | File |
|---------|------|
| Gaps, colors, terminal, tags, **scratchpads**, bar | `crates/dwm-core/src/config.rs` |
| Key bindings | `crates/dwm-wayland/src/bindings.rs` |

### Scratchpads

Same idea as Luke’s build:

| Keys | Action |
|------|--------|
| `Mod+Shift+Return` | Toggle `spterm` scratchpad (spawn if needed) |
| `Mod+'` | Toggle `spcalc` scratchpad |

Override the terminal binary with `DWM_WAYLAND_TERM=foot` (uses `-T title`). Default is `ghostty` (`--title=`).

### Status bar

Top bar shows tags · layout · focused title · status text.

Write status the same way dwmblocks wrote the root name — into a file:

```bash
echo "vol 50% | $(date +%H:%M)" > "${XDG_RUNTIME_DIR:-/tmp}/dwm-wayland.status"
```

Path defaults to `$XDG_RUNTIME_DIR/dwm-wayland.status` (see `Config::status_file`).

## Build

```bash
# System deps (Debian/Ubuntu) — nested (winit) path
sudo apt install pkg-config libwayland-dev wayland-protocols \
  libxkbcommon-dev libxkbcommon-x11-0 libegl1-mesa-dev xwayland

# Extra deps for TTY session backend (`--session`, cargo feature `session`)
sudo apt install libseat-dev libinput-dev libgbm-dev libdrm-dev \
  libudev-dev

# Nested sessions also need a writable XDG_RUNTIME_DIR
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/runtime-$USER}
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"

cargo build --release
# Nested-only binary (no libseat/DRM, no XWayland):
cargo build -p dwm-wayland --no-default-features --release
```

## Test

```bash
cargo test --workspace
cargo run -p dwm-wayland -- --check   # protocol + core smoke test (no GPU loop)
```

## Run nested (default / recommended for development)

Under an existing X11 or Wayland session (winit window). The nested path runs on a
**calloop** event loop (`ListeningSocketSource` + winit source), same shape as
smithay’s smallvil example:

```bash
# Default features include `session` + `xwayland`, but omitting `--session` still uses winit:
cargo run -p dwm-wayland
# Explicit nested-only build (no XWayland / session):
cargo run -p dwm-wayland --no-default-features
# or
./target/release/dwm-wayland
```

Set `DWM_WAYLAND_NO_AUTOSTART=1` to skip spawning the default terminal.

## XWayland

Enabled by the default cargo feature `xwayland` (needs `Xwayland` on `PATH`).

On both the nested (winit) and `--session` (udev) paths the compositor:

1. Spawns XWayland via smithay’s `XWayland::spawn` and inserts it into calloop
2. On `Ready`, starts `X11Wm` and sets `DISPLAY=:<n>` for child processes
3. Maps X11 windows into `dwm-core` clients (tile/float/fullscreen via arrange)
4. Implements minimal `XwmHandler` + `XWaylandShellHandler`

Disable with `--no-default-features` or by building without the `xwayland` feature.

```bash
# X11 app under nested compositor (after dwm-wayland is running with xwayland):
DISPLAY=:0 xterm   # use the DISPLAY value logged at "XWayland ready"
```

Clipboard bridging between X11 and Wayland selections is not fully wired yet
(selection access defaults to deny).

## Run on a TTY (`--session`) — experimental

The DRM/libseat/libinput backend is **experimental**. Prefer a spare TTY (e.g. Ctrl+Alt+F3), not your daily desktop session, until it stabilizes.

```bash
# From a spare virtual terminal (log in as your user):
cargo run -p dwm-wayland -- --session
# Optional: pin a DRM node
DWM_WAYLAND_DRM_DEVICE=/dev/dri/card0 cargo run -p dwm-wayland -- --session
```

Requirements: a working seat (seatd or systemd-logind), `/dev/dri`, and at least one connected display. If `LibSeatSession` or DRM open fails, the process exits with an error (it does not fall back to winit).

**Status:** single-GPU / first-connector modeset, libinput, session pause/resume, XWayland (with feature), and a clear+client render loop are wired. Multi-output layout, DRM leasing, dmabuf feedback on the session path, and polished VT switching are still gaps — do not treat this as a daily driver yet.

### Key bindings (Super / Mod4, or Alt in nested sessions)

Host window managers often grab Super; nested runs also accept **Alt** as the mod key.

| Keys | Action |
|------|--------|
| `j` / `k` | Focus next / previous |
| `Shift+j` / `k` | Push down / up stack |
| `1`…`9` | View tag |
| `Shift+1`…`9` | Move client to tag |
| `t` / `Shift+t` | Tile / bottomstack |
| `y` / `Shift+y` | Spiral / dwindle |
| `u` / `Shift+u` | Deck / monocle |
| `i` / `Shift+i` | Centered master / centered floating master |
| `f` | Toggle floating |
| `s` | Toggle sticky |
| `a` | Toggle gaps |
| `h` / `l` | Shrink / grow master |
| `o` / `Shift+o` | Inc / dec `nmaster` |
| `Return` | Spawn terminal (`ghostty`) |
| `Shift+Return` | Toggle `spterm` scratchpad |
| `'` | Toggle `spcalc` scratchpad |
| `b` | Toggle status bar |
| `d` | `dmenu_run` |
| `q` / `Shift+q` | Kill client / quit |

## Layouts

Ported from the vanitygaps-patched build in `legacy/`:

`[]=` tile · `TTT` bstack · `[@]` spiral · `[\]` dwindle · `[D]` deck · `[M]` monocle · `|M|` centeredmaster · `>M>` centeredfloatingmaster · `><>` floating

## Production readiness

Honest status versus replacing X11 dwm as a daily driver:

### Works today

- Nested winit session on a calloop loop (socket + display + winit sources)
- dwm-core tiling: tags, layouts, vanity gaps, stacker, sticky, floating, fullscreen
- Server-side decorations / colored borders, status bar, scratchpads
- Pointer focus-follows-mouse, Mod+mouse move/resize, keyboard bindings
- xdg-shell toplevels + popups, wl_shm, data-device + primary selection globals
- linux-dmabuf v3 global on the winit path (formats from GlesRenderer)
- XWayland spawn + X11Wm mapping into dwm clients (`DISPLAY` set on Ready) — nested and session
- Experimental DRM/libseat/libinput TTY session (`--session`)

### Remaining gaps (not a drop-in X11 dwm yet)

- Multi-monitor / per-output tags and layout
- X11↔Wayland clipboard/primary bridging and drag-and-drop polish
- Layer-shell (panels, on-screen keyboards), idle/inhibit, gamma/color management
- Session path: multi-GPU, dmabuf feedback / scanout plane, DRM leasing, robust VT switch
- Config is still recompile-time (no runtime `config.h` reload / IPC beyond the status file)
- Crash recovery, screenshot/screencopy portals, security-context / sandboxed clients
- Popup seat grabs, tablet/touch, gesture bindings
- Hardening / crash recovery for long-running seats

### Display manager session

After `cargo build --release -p dwm-wayland`:

```bash
sudo ./contrib/install-session.sh
```

Installs `/usr/local/bin/dwm-wayland` and
`/usr/local/share/wayland-sessions/dwm-wayland.desktop` so GDM/SDDM/LightDM can
offer **dwm-wayland** (runs `dwm-wayland --session`). Prefer a test user / spare
machine until the session backend is battle-tested.

## Legacy X11 dwm

```bash
cd legacy && make
sudo make install
```
