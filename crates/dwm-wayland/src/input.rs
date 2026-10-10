//! Keyboard + pointer processing (dwm-style focus and Mod+mouse).

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, Focus, GrabStartData, MotionEvent, RelativeMotionEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};

use crate::bindings::{self, Cmd};
use crate::grabs::{MoveSurfaceGrab, ResizeSurfaceGrab};
use crate::state::DwmState;

const BTN_LEFT: u32 = 0x110;
const BTN_MIDDLE: u32 = 0x112;
const BTN_RIGHT: u32 = 0x111;

impl DwmState {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let pressed = event.state() == KeyState::Pressed;
                let keyboard = self.seat.get_keyboard().unwrap();
                keyboard.input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |state, modifiers, handle| {
                        if pressed {
                            let keysym = handle
                                .raw_syms()
                                .first()
                                .copied()
                                .unwrap_or_else(|| handle.modified_sym());
                            if let Some(cmd) = bindings::lookup(&state.bindings, modifiers, keysym)
                            {
                                match cmd {
                                    Cmd::Core(c) => state.apply_command(c),
                                    Cmd::SpawnTerminal => {
                                        crate::state::spawn_program(&state.config.terminal, &[])
                                    }
                                    Cmd::SpawnDmenu => {
                                        crate::state::spawn_program("dmenu_run", &[])
                                    }
                                }
                                return FilterResult::Intercept(());
                            }
                        }
                        FilterResult::Forward
                    },
                );
            }
            InputEvent::PointerMotion { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let mut pos = pointer.current_location();
                pos.x += event.delta_x();
                pos.y += event.delta_y();
                // Clamp to screen.
                pos.x = pos.x.clamp(0.0, self.monitor.screen.w as f64);
                pos.y = pos.y.clamp(0.0, self.monitor.screen.h as f64);
                self.pointer_motion(pos, event.time_msec());
                pointer.relative_motion(
                    self,
                    self.surface_under(pos),
                    &RelativeMotionEvent {
                        delta: event.delta(),
                        delta_unaccel: event.delta_unaccel(),
                        utime: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let size = (
                    self.monitor.screen.w as f64,
                    self.monitor.screen.h as f64,
                );
                let pos = event.position_transformed(size.into());
                self.pointer_motion(pos, event.time_msec());
                self.seat.get_pointer().unwrap().frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                self.pointer_button(event.button_code(), event.state(), event.time_msec());
            }
            InputEvent::PointerAxis { event, .. } => {
                self.pointer_axis(&event);
            }
            _ => {}
        }
    }

    fn pointer_motion(&mut self, pos: Point<f64, Logical>, time: u32) {
        self.pointer_location = pos;
        let serial = SERIAL_COUNTER.next_serial();
        let under = self.surface_under(pos);
        // Focus-follows-mouse (classic dwm EnterNotify behaviour).
        if let Some(id) = self.client_at(pos) {
            if self.monitor.sel != Some(id) && !self.seat.get_pointer().unwrap().is_grabbed() {
                self.monitor.focus(Some(id));
                self.focus_client(id);
                let (geoms, symbol) = self.monitor.arrange();
                self.layout_symbol = symbol;
                self.push_configures(&geoms);
            }
        }
        let pointer = self.seat.get_pointer().unwrap();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial,
                time,
            },
        );
    }

    fn pointer_button(&mut self, button: u32, button_state: ButtonState, time: u32) {
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        let mods = self
            .seat
            .get_keyboard()
            .map(|k| k.modifier_state())
            .unwrap_or_default();
        let mod_down = mods.logo || mods.alt;

        if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
            if let Some(id) = self.client_at(self.pointer_location) {
                self.monitor.focus(Some(id));
                self.focus_client(id);
                let (geoms, symbol) = self.monitor.arrange();
                self.layout_symbol = symbol;
                self.push_configures(&geoms);

                if mod_down {
                    let geom = self
                        .monitor
                        .client(id)
                        .map(|c| c.geom)
                        .unwrap_or_default();
                    let start_data = GrabStartData {
                        focus: None,
                        button,
                        location: self.pointer_location,
                    };
                    if button == BTN_LEFT {
                        // Ensure floating before move (dwm movemouse).
                        if let Some(c) = self.monitor.client_mut(id) {
                            if !c.floating {
                                c.floating = true;
                                c.float_geom = c.geom;
                            }
                        }
                        pointer.set_grab(
                            self,
                            MoveSurfaceGrab {
                                start_data,
                                client: id,
                                initial: geom,
                            },
                            serial,
                            Focus::Clear,
                        );
                        return;
                    } else if button == BTN_RIGHT {
                        if let Some(c) = self.monitor.client_mut(id) {
                            if !c.floating {
                                c.floating = true;
                                c.float_geom = c.geom;
                            }
                        }
                        pointer.set_grab(
                            self,
                            ResizeSurfaceGrab {
                                start_data,
                                client: id,
                                initial: geom,
                            },
                            serial,
                            Focus::Clear,
                        );
                        return;
                    } else if button == BTN_MIDDLE {
                        self.apply_command(dwm_core::Command::DefaultGaps);
                        return;
                    }
                }
            }
        }

        pointer.button(
            self,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time,
            },
        );
        pointer.frame(self);
    }

    fn pointer_axis<I: InputBackend>(&mut self, event: &impl PointerAxisEvent<I>) {
        let source = event.source();
        let horizontal_amount = event
            .amount(Axis::Horizontal)
            .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
        let vertical_amount = event
            .amount(Axis::Vertical)
            .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
        let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
        let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

        let mut frame = AxisFrame::new(event.time_msec()).source(source);
        if horizontal_amount != 0.0 {
            frame = frame.value(Axis::Horizontal, horizontal_amount);
            if let Some(discrete) = horizontal_amount_discrete {
                frame = frame.v120(Axis::Horizontal, discrete as i32);
            }
        }
        if vertical_amount != 0.0 {
            frame = frame.value(Axis::Vertical, vertical_amount);
            if let Some(discrete) = vertical_amount_discrete {
                frame = frame.v120(Axis::Vertical, discrete as i32);
            }
        }
        if source == AxisSource::Finger {
            if event.amount(Axis::Horizontal) == Some(0.0) {
                frame = frame.stop(Axis::Horizontal);
            }
            if event.amount(Axis::Vertical) == Some(0.0) {
                frame = frame.stop(Axis::Vertical);
            }
        }
        let pointer = self.seat.get_pointer().unwrap();
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    pub fn client_at(&self, pos: Point<f64, Logical>) -> Option<dwm_core::ClientId> {
        let tagset = self.monitor.tagset();
        let x = pos.x as i32;
        let y = pos.y as i32;
        // Topmost first: floating, then tiled reverse order.
        let mut clients: Vec<_> = self
            .monitor
            .visible_clients()
            .into_iter()
            .filter(|c| c.geom.w > 0 && c.geom.h > 0 && c.is_visible(tagset))
            .collect();
        clients.sort_by_key(|c| !(c.floating || c.fullscreen));
        for c in clients.into_iter().rev() {
            let g = c.geom;
            if x >= g.x && y >= g.y && x < g.x + g.w && y < g.y + g.h {
                return Some(c.id);
            }
        }
        None
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        Point<f64, Logical>,
    )> {
        use smithay::desktop::PopupManager;

        let id = self.client_at(pos)?;
        let toplevel = self.toplevels.get(&id)?;
        let c = self.monitor.client(id)?;
        let win_loc = (c.geom.x as f64, c.geom.y as f64);
        let relative = Point::from((pos.x - win_loc.0, pos.y - win_loc.1));

        // Prefer popups belonging to this toplevel.
        for (popup, popup_offset) in PopupManager::popups_for_surface(toplevel.wl_surface()) {
            let geo = popup.geometry();
            let origin = Point::from((
                win_loc.0 + popup_offset.x as f64 + geo.loc.x as f64,
                win_loc.1 + popup_offset.y as f64 + geo.loc.y as f64,
            ));
            if pos.x >= origin.x
                && pos.y >= origin.y
                && pos.x < origin.x + geo.size.w as f64
                && pos.y < origin.y + geo.size.h as f64
            {
                return Some((
                    popup.wl_surface().clone(),
                    (pos.x - origin.x, pos.y - origin.y).into(),
                ));
            }
        }

        Some((toplevel.wl_surface().clone(), relative))
    }
}
