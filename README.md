# dwm-wayland (Rust)

A [dwm](https://dwm.suckless.org/)-inspired **tiling Wayland compositor** rewritten in Rust.

Window-management logic (tags, layouts, vanity gaps, stacker, shiftview, sticky, …)
lives in a pure library crate so it can be tested without a GPU. The compositor
binary uses [smithay](https://github.com/Smithay/smithay) for the Wayland frontend
and the winit backend for nested sessions.

The original X11 C sources are preserved under [`legacy/`](legacy/).

## Crates

| Crate | Role |
|-------|------|
| `dwm-core` | Pure WM state machine + layout algorithms |
| `dwm-wayland` | Wayland compositor (`smithay` + winit) |

## Build

```bash
# System deps (Debian/Ubuntu)
sudo apt install pkg-config libwayland-dev wayland-protocols \
  libxkbcommon-dev libxkbcommon-x11-0 libegl1-mesa-dev

# Nested sessions also need a writable XDG_RUNTIME_DIR
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/runtime-$USER}
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"

cargo build --release
```

## Test

```bash
cargo test --workspace
cargo run -p dwm-wayland -- --check   # protocol + core smoke test (no GPU loop)
```

## Run (nested)

Under an existing X11 or Wayland session:

```bash
cargo run -p dwm-wayland
# or
./target/release/dwm-wayland
```

Set `DWM_WAYLAND_NO_AUTOSTART=1` to skip spawning the default terminal.

### Key bindings (Super / Mod4)

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
| `d` | `dmenu_run` |
| `q` / `Shift+q` | Kill client / quit |

## Layouts

Ported from the vanitygaps-patched build in `legacy/`:

`[]=` tile · `TTT` bstack · `[@]` spiral · `[\]` dwindle · `[D]` deck · `[M]` monocle · `|M|` centeredmaster · `>M>` centeredfloatingmaster · `><>` floating

## Legacy X11 dwm

```bash
cd legacy && make
sudo make install
```
