//! Pointer grabs for floating move / resize (classic dwm Mod+Btn1 / Mod+Btn3).

use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

use dwm_core::{ClientId, Rect};

use crate::state::DwmState;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<DwmState>,
    pub client: ClientId,
    pub initial: Rect,
}

impl PointerGrab<DwmState> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        let delta = event.location - self.start_data.location;
        let nx = self.initial.x + delta.x.round() as i32;
        let ny = self.initial.y + delta.y.round() as i32;
        if let Some(c) = data.monitor.client_mut(self.client) {
            c.floating = true;
            c.float_geom = Rect::new(nx, ny, self.initial.w, self.initial.h);
            c.geom = c.float_geom;
        }
        data.pointer_location = event.location;
        data.rearrange();
    }

    fn relative_motion(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if !handle.current_pressed().contains(&BTN_LEFT) {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut DwmState, handle: &mut PointerInnerHandle<'_, DwmState>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<DwmState> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut DwmState) {}
}

pub struct ResizeSurfaceGrab {
    pub start_data: PointerGrabStartData<DwmState>,
    pub client: ClientId,
    pub initial: Rect,
}

impl PointerGrab<DwmState> for ResizeSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        let delta = event.location - self.start_data.location;
        let nw = (self.initial.w + delta.x.round() as i32).max(100);
        let nh = (self.initial.h + delta.y.round() as i32).max(60);
        if let Some(c) = data.monitor.client_mut(self.client) {
            c.floating = true;
            c.float_geom = Rect::new(self.initial.x, self.initial.y, nw, nh);
            c.geom = c.float_geom;
        }
        data.pointer_location = event.location;
        data.rearrange();
    }

    fn relative_motion(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if !handle.current_pressed().contains(&BTN_RIGHT)
            && !handle.current_pressed().contains(&BTN_LEFT)
        {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut DwmState, handle: &mut PointerInnerHandle<'_, DwmState>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut DwmState,
        handle: &mut PointerInnerHandle<'_, DwmState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<DwmState> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut DwmState) {}
}
