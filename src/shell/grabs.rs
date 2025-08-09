use std::cell::RefCell;

use smithay::{
    desktop::{space::SpaceElement, WindowSurface},
    input::{
        pointer::{
            AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
            GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
            GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab, PointerInnerHandle,
            RelativeMotionEvent,
        },
        touch::{GrabStartData as TouchGrabStartData, TouchGrab},
    },
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{IsAlive, Logical, Point, Serial, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};
#[cfg(feature = "xwayland")]
use smithay::{utils::Rectangle, xwayland::xwm::ResizeEdge as X11ResizeEdge};

use super::{SurfaceData, WindowElement};
use crate::{
    focus::PointerFocusTarget,
    state::{Backend, StilchState},
};

pub struct PointerMoveSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: PointerGrabStartData<StilchState<BackendData>>,
    pub window: WindowElement,
    pub initial_window_location: Point<i32, Logical>,
}

impl<BackendData: Backend> PointerGrab<StilchState<BackendData>>
    for PointerMoveSurfaceGrab<BackendData>
{
    fn motion(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;

        // Use centralized window manager for position updates
        data.window_manager
            .update_element_position(&self.window, new_location.to_i32_round());

        // Queue redraw for affected outputs
    }

    fn relative_motion(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn frame(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
    ) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<StilchState<BackendData>> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut StilchState<BackendData>) {
        // Mark any moved windows for update
        data.window_manager.mark_moved_windows();

        // Find the window in the registry and update workspace layout if needed
        if let Some(window_id) = data.window_registry().find_by_element(&self.window) {
            if let Some(managed_window) = data.window_registry().get(window_id) {
                let workspace_id = managed_window.workspace;

                // Get the final position from space
                if let Some(_final_position) = data.space().element_location(&self.window) {
                    // Update the workspace layout with the new position
                    // This ensures the layout system knows about the manual position change
                    if let Some(workspace) = data.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        // For now, just mark that the workspace needs relayout
                        // In the future, we might want to update the layout directly
                        workspace.relayout();
                    }
                }
            }
        }
    }
}

pub struct TouchMoveSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: TouchGrabStartData<StilchState<BackendData>>,
    pub window: WindowElement,
    pub initial_window_location: Point<i32, Logical>,
}

impl<BackendData: Backend> TouchGrab<StilchState<BackendData>>
    for TouchMoveSurfaceGrab<BackendData>
{
    fn down(
        &mut self,
        _data: &mut StilchState<BackendData>,
        _handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        _focus: Option<(
            <StilchState<BackendData> as smithay::input::SeatHandler>::TouchFocus,
            Point<f64, Logical>,
        )>,
        _event: &smithay::input::touch::DownEvent,
        _seq: Serial,
    ) {
    }

    fn up(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        event: &smithay::input::touch::UpEvent,
        seq: Serial,
    ) {
        if event.slot != self.start_data.slot {
            return;
        }

        handle.up(data, event, seq);
        handle.unset_grab(self, data);
    }

    fn motion(
        &mut self,
        data: &mut StilchState<BackendData>,
        _handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        _focus: Option<(
            <StilchState<BackendData> as smithay::input::SeatHandler>::TouchFocus,
            Point<f64, Logical>,
        )>,
        event: &smithay::input::touch::MotionEvent,
        _seq: Serial,
    ) {
        if event.slot != self.start_data.slot {
            return;
        }

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;

        // Use centralized window manager for position updates
        data.window_manager
            .update_element_position(&self.window, new_location.to_i32_round());

        // Queue redraw for affected outputs
    }

    fn frame(
        &mut self,
        _data: &mut StilchState<BackendData>,
        _handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        _seq: Serial,
    ) {
    }

    fn cancel(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        seq: Serial,
    ) {
        handle.cancel(data, seq);
        handle.unset_grab(self, data);
    }

    fn shape(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        event: &smithay::input::touch::ShapeEvent,
        seq: Serial,
    ) {
        handle.shape(data, event, seq);
    }

    fn orientation(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        event: &smithay::input::touch::OrientationEvent,
        seq: Serial,
    ) {
        handle.orientation(data, event, seq);
    }

    fn start_data(&self) -> &smithay::input::touch::GrabStartData<StilchState<BackendData>> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut StilchState<BackendData>) {
        // Mark any moved windows for update
        data.window_manager.mark_moved_windows();

        // Find the window in the registry and update workspace layout if needed
        if let Some(window_id) = data.window_registry().find_by_element(&self.window) {
            if let Some(managed_window) = data.window_registry().get(window_id) {
                let workspace_id = managed_window.workspace;

                // Get the final position from space
                if let Some(_final_position) = data.space().element_location(&self.window) {
                    // Update the workspace layout with the new position
                    // This ensures the layout system knows about the manual position change
                    if let Some(workspace) = data.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        // For now, just mark that the workspace needs relayout
                        // In the future, we might want to update the layout directly
                        workspace.relayout();
                    }
                }
            }
        }
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const NONE = 0;
        const TOP = 1;
        const BOTTOM = 2;
        const LEFT = 4;
        const TOP_LEFT = 5;
        const BOTTOM_LEFT = 6;
        const RIGHT = 8;
        const TOP_RIGHT = 9;
        const BOTTOM_RIGHT = 10;
    }
}

impl From<xdg_toplevel::ResizeEdge> for ResizeEdge {
    #[inline]
    fn from(x: xdg_toplevel::ResizeEdge) -> Self {
        Self::from_bits(x as u32).unwrap_or(ResizeEdge::empty())
    }
}

impl From<ResizeEdge> for xdg_toplevel::ResizeEdge {
    #[inline]
    fn from(x: ResizeEdge) -> Self {
        Self::try_from(x.bits()).unwrap_or(Self::None)
    }
}

#[cfg(feature = "xwayland")]
impl From<X11ResizeEdge> for ResizeEdge {
    #[inline]
    fn from(edge: X11ResizeEdge) -> Self {
        match edge {
            X11ResizeEdge::Bottom => ResizeEdge::BOTTOM,
            X11ResizeEdge::BottomLeft => ResizeEdge::BOTTOM_LEFT,
            X11ResizeEdge::BottomRight => ResizeEdge::BOTTOM_RIGHT,
            X11ResizeEdge::Left => ResizeEdge::LEFT,
            X11ResizeEdge::Right => ResizeEdge::RIGHT,
            X11ResizeEdge::Top => ResizeEdge::TOP,
            X11ResizeEdge::TopLeft => ResizeEdge::TOP_LEFT,
            X11ResizeEdge::TopRight => ResizeEdge::TOP_RIGHT,
        }
    }
}

/// Information about the resize operation.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ResizeData {
    /// The edges the surface is being resized with.
    pub edges: ResizeEdge,
    /// The initial window location.
    pub initial_window_location: Point<i32, Logical>,
    /// The initial window size (geometry width and height).
    pub initial_window_size: Size<i32, Logical>,
}

/// State of the resize operation.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum ResizeState {
    /// The surface is not being resized.
    #[default]
    NotResizing,
    /// The surface is currently being resized.
    Resizing(ResizeData),
    /// The resize has finished, and the surface needs to ack the final configure.
    WaitingForFinalAck(ResizeData, Serial),
    /// The resize has finished, and the surface needs to commit its final state.
    WaitingForCommit(ResizeData),
}

pub struct PointerResizeSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: PointerGrabStartData<StilchState<BackendData>>,
    pub window: WindowElement,
    pub edges: ResizeEdge,
    pub initial_window_location: Point<i32, Logical>,
    pub initial_window_size: Size<i32, Logical>,
    pub last_window_size: Size<i32, Logical>,
}

impl<BackendData: Backend> PointerGrab<StilchState<BackendData>>
    for PointerResizeSurfaceGrab<BackendData>
{
    fn motion(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        _focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        // It is impossible to get `min_size` and `max_size` of dead toplevel, so we return early.
        if !self.window.alive() {
            handle.unset_grab(self, data, event.serial, event.time, true);
            return;
        }

        let (mut dx, mut dy) = (event.location - self.start_data.location).into();

        let mut new_window_width = self.initial_window_size.w;
        let mut new_window_height = self.initial_window_size.h;

        let left_right = ResizeEdge::LEFT | ResizeEdge::RIGHT;
        let top_bottom = ResizeEdge::TOP | ResizeEdge::BOTTOM;

        if self.edges.intersects(left_right) {
            if self.edges.intersects(ResizeEdge::LEFT) {
                dx = -dx;
            }

            new_window_width = (self.initial_window_size.w as f64 + dx) as i32;
        }

        if self.edges.intersects(top_bottom) {
            if self.edges.intersects(ResizeEdge::TOP) {
                dy = -dy;
            }

            new_window_height = (self.initial_window_size.h as f64 + dy) as i32;
        }

        let (min_size, max_size) = if let Some(surface) = self.window.wl_surface() {
            with_states(&surface, |states| {
                let mut guard = states.cached_state.get::<SurfaceCachedState>();
                let data = guard.current();
                (data.min_size, data.max_size)
            })
        } else {
            ((0, 0).into(), (0, 0).into())
        };

        let min_width = min_size.w.max(1);
        let min_height = min_size.h.max(1);
        let max_width = if max_size.w == 0 {
            i32::MAX
        } else {
            max_size.w
        };
        let max_height = if max_size.h == 0 {
            i32::MAX
        } else {
            max_size.h
        };

        new_window_width = new_window_width.max(min_width).min(max_width);
        new_window_height = new_window_height.max(min_height).min(max_height);

        self.last_window_size = (new_window_width, new_window_height).into();

        match &self.window.0.underlying_surface() {
            WindowSurface::Wayland(xdg) => {
                xdg.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Resizing);
                    state.size = Some(self.last_window_size);
                });
                xdg.send_pending_configure();
            }
            #[cfg(feature = "xwayland")]
            WindowSurface::X11(x11) => {
                let location = data
                    .space()
                    .element_location(&self.window)
                    .unwrap_or_else(|| {
                        tracing::warn!("No location for window during resize");
                        Point::from((0, 0))
                    });
                if let Err(e) = x11.configure(Rectangle::new(location, self.last_window_size)) {
                    tracing::error!("Failed to configure X11 window during resize: {:?}", e);
                }
            }
        }
    }

    fn relative_motion(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        focus: Option<(PointerFocusTarget, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(self, data, event.serial, event.time, true);

            // If toplevel is dead, we can't resize it, so we return early.
            if !self.window.alive() {
                return;
            }

            match &self.window.0.underlying_surface() {
                WindowSurface::Wayland(xdg) => {
                    xdg.with_pending_state(|state| {
                        state.states.unset(xdg_toplevel::State::Resizing);
                        state.size = Some(self.last_window_size);
                    });
                    xdg.send_pending_configure();
                    if self.edges.intersects(ResizeEdge::TOP_LEFT) {
                        let geometry = self.window.geometry();
                        let mut location = data
                            .space()
                            .element_location(&self.window)
                            .unwrap_or_else(|| {
                                tracing::warn!("No location for window during resize");
                                Point::from((0, 0))
                            });

                        if self.edges.intersects(ResizeEdge::LEFT) {
                            location.x = self.initial_window_location.x
                                + (self.initial_window_size.w - geometry.size.w);
                        }
                        if self.edges.intersects(ResizeEdge::TOP) {
                            location.y = self.initial_window_location.y
                                + (self.initial_window_size.h - geometry.size.h);
                        }

                        // Use centralized window manager for position updates
                        data.window_manager
                            .update_element_position(&self.window, location);
                    }

                    if let Some(surface) = self.window.wl_surface() {
                        with_states(&surface, |states| {
                            let mut data = states
                                .data_map
                                .get::<RefCell<SurfaceData>>()
                                // SAFETY: SurfaceData is always initialized for compositor surfaces
                                .expect("SurfaceData should exist")
                                .borrow_mut();
                            if let ResizeState::Resizing(resize_data) = data.resize_state {
                                data.resize_state =
                                    ResizeState::WaitingForFinalAck(resize_data, event.serial);
                            } else {
                                tracing::error!("Invalid resize state: {:?}", data.resize_state);
                                // Reset to idle state to recover
                                data.resize_state = ResizeState::NotResizing;
                            }
                        });
                    }
                }
                #[cfg(feature = "xwayland")]
                WindowSurface::X11(x11) => {
                    let mut location =
                        data.space()
                            .element_location(&self.window)
                            .unwrap_or_else(|| {
                                tracing::warn!("No location for X11 window during resize");
                                Point::from((0, 0))
                            });
                    if self.edges.intersects(ResizeEdge::TOP_LEFT) {
                        let geometry = self.window.geometry();

                        if self.edges.intersects(ResizeEdge::LEFT) {
                            location.x = self.initial_window_location.x
                                + (self.initial_window_size.w - geometry.size.w);
                        }
                        if self.edges.intersects(ResizeEdge::TOP) {
                            location.y = self.initial_window_location.y
                                + (self.initial_window_size.h - geometry.size.h);
                        }

                        // Use centralized window manager for position updates
                        data.window_manager
                            .update_element_position(&self.window, location);
                    }
                    if let Err(e) = x11.configure(Rectangle::new(location, self.last_window_size)) {
                        tracing::error!("Failed to configure X11 window: {:?}", e);
                    }

                    let Some(surface) = self.window.wl_surface() else {
                        // X11 Window got unmapped, abort
                        return;
                    };
                    with_states(&surface, |states| {
                        let mut data = states
                            .data_map
                            .get::<RefCell<SurfaceData>>()
                            // SAFETY: SurfaceData is always initialized for compositor surfaces
                            .expect("SurfaceData should exist")
                            .borrow_mut();
                        if let ResizeState::Resizing(resize_data) = data.resize_state {
                            data.resize_state = ResizeState::WaitingForCommit(resize_data);
                        } else {
                            tracing::error!("Invalid resize state: {:?}", data.resize_state);
                            // Reset to idle state to recover
                            data.resize_state = ResizeState::NotResizing;
                        }
                    });
                }
            }
        }
    }

    fn axis(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn frame(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
    ) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut PointerInnerHandle<'_, StilchState<BackendData>>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &PointerGrabStartData<StilchState<BackendData>> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut StilchState<BackendData>) {
        // Mark any moved windows for update
        data.window_manager.mark_moved_windows();

        // Find the window in the registry and update workspace layout if needed
        if let Some(window_id) = data.window_registry().find_by_element(&self.window) {
            if let Some(managed_window) = data.window_registry().get(window_id) {
                let workspace_id = managed_window.workspace;

                // Get the final position from space
                if let Some(_final_position) = data.space().element_location(&self.window) {
                    // Update the workspace layout with the new position
                    // This ensures the layout system knows about the manual position change
                    if let Some(workspace) = data.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        // For now, just mark that the workspace needs relayout
                        // In the future, we might want to update the layout directly
                        workspace.relayout();
                    }
                }
            }
        }
    }
}

pub struct TouchResizeSurfaceGrab<BackendData: Backend + 'static> {
    pub start_data: TouchGrabStartData<StilchState<BackendData>>,
    pub window: WindowElement,
    pub edges: ResizeEdge,
    pub initial_window_location: Point<i32, Logical>,
    pub initial_window_size: Size<i32, Logical>,
    pub last_window_size: Size<i32, Logical>,
}

impl<BackendData: Backend> TouchGrab<StilchState<BackendData>>
    for TouchResizeSurfaceGrab<BackendData>
{
    fn down(
        &mut self,
        _data: &mut StilchState<BackendData>,
        _handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        _focus: Option<(
            <StilchState<BackendData> as smithay::input::SeatHandler>::TouchFocus,
            Point<f64, Logical>,
        )>,
        _event: &smithay::input::touch::DownEvent,
        _seq: Serial,
    ) {
    }

    fn up(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        event: &smithay::input::touch::UpEvent,
        _seq: Serial,
    ) {
        if event.slot != self.start_data.slot {
            return;
        }
        handle.unset_grab(self, data);

        // If toplevel is dead, we can't resize it, so we return early.
        if !self.window.alive() {
            return;
        }

        match self.window.0.underlying_surface() {
            WindowSurface::Wayland(xdg) => {
                xdg.with_pending_state(|state| {
                    state.states.unset(xdg_toplevel::State::Resizing);
                    state.size = Some(self.last_window_size);
                });
                xdg.send_pending_configure();
                if self.edges.intersects(ResizeEdge::TOP_LEFT) {
                    let geometry = self.window.geometry();
                    let mut location =
                        data.space()
                            .element_location(&self.window)
                            .unwrap_or_else(|| {
                                tracing::warn!("No location for X11 window during resize");
                                Point::from((0, 0))
                            });

                    if self.edges.intersects(ResizeEdge::LEFT) {
                        location.x = self.initial_window_location.x
                            + (self.initial_window_size.w - geometry.size.w);
                    }
                    if self.edges.intersects(ResizeEdge::TOP) {
                        location.y = self.initial_window_location.y
                            + (self.initial_window_size.h - geometry.size.h);
                    }

                    data.window_manager
                        .update_element_position(&self.window, location);
                }

                if let Some(wl_surface) = self.window.wl_surface() {
                    with_states(&wl_surface, |states| {
                        if let Some(surface_data) = states.data_map.get::<RefCell<SurfaceData>>() {
                            let mut data = surface_data.borrow_mut();
                            if let ResizeState::Resizing(resize_data) = data.resize_state {
                                data.resize_state =
                                    ResizeState::WaitingForFinalAck(resize_data, event.serial);
                            } else {
                                tracing::error!("Invalid resize state: {:?}", data.resize_state);
                                // Reset to idle state to recover
                                data.resize_state = ResizeState::NotResizing;
                            }
                        }
                    });
                }
            }
            #[cfg(feature = "xwayland")]
            WindowSurface::X11(x11) => {
                let mut location =
                    data.space()
                        .element_location(&self.window)
                        .unwrap_or_else(|| {
                            tracing::warn!("Window has no location, using origin");
                            Point::from((0, 0))
                        });
                if self.edges.intersects(ResizeEdge::TOP_LEFT) {
                    let geometry = self.window.geometry();

                    if self.edges.intersects(ResizeEdge::LEFT) {
                        location.x = self.initial_window_location.x
                            + (self.initial_window_size.w - geometry.size.w);
                    }
                    if self.edges.intersects(ResizeEdge::TOP) {
                        location.y = self.initial_window_location.y
                            + (self.initial_window_size.h - geometry.size.h);
                    }

                    data.window_manager
                        .update_element_position(&self.window, location);
                }
                if let Err(e) = x11.configure(Rectangle::new(location, self.last_window_size)) {
                    tracing::warn!("Failed to configure X11 window: {:?}", e);
                }

                let Some(surface) = self.window.wl_surface() else {
                    // X11 Window got unmapped, abort
                    return;
                };
                with_states(&surface, |states| {
                    if let Some(surface_data) = states.data_map.get::<RefCell<SurfaceData>>() {
                        let mut data = surface_data.borrow_mut();
                        if let ResizeState::Resizing(resize_data) = data.resize_state {
                            data.resize_state = ResizeState::WaitingForCommit(resize_data);
                        } else {
                            tracing::error!("Invalid resize state: {:?}", data.resize_state);
                            // Reset to idle state to recover
                            data.resize_state = ResizeState::NotResizing;
                        }
                    }
                });
            }
        }
    }

    fn motion(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        _focus: Option<(
            <StilchState<BackendData> as smithay::input::SeatHandler>::TouchFocus,
            Point<f64, Logical>,
        )>,
        event: &smithay::input::touch::MotionEvent,
        _seq: Serial,
    ) {
        if event.slot != self.start_data.slot {
            return;
        }

        // It is impossible to get `min_size` and `max_size` of dead toplevel, so we return early.
        if !self.window.alive() {
            handle.unset_grab(self, data);
            return;
        }

        let (mut dx, mut dy) = (event.location - self.start_data.location).into();

        let mut new_window_width = self.initial_window_size.w;
        let mut new_window_height = self.initial_window_size.h;

        let left_right = ResizeEdge::LEFT | ResizeEdge::RIGHT;
        let top_bottom = ResizeEdge::TOP | ResizeEdge::BOTTOM;

        if self.edges.intersects(left_right) {
            if self.edges.intersects(ResizeEdge::LEFT) {
                dx = -dx;
            }

            new_window_width = (self.initial_window_size.w as f64 + dx) as i32;
        }

        if self.edges.intersects(top_bottom) {
            if self.edges.intersects(ResizeEdge::TOP) {
                dy = -dy;
            }

            new_window_height = (self.initial_window_size.h as f64 + dy) as i32;
        }

        let (min_size, max_size) = if let Some(surface) = self.window.wl_surface() {
            with_states(&surface, |states| {
                let mut guard = states.cached_state.get::<SurfaceCachedState>();
                let data = guard.current();
                (data.min_size, data.max_size)
            })
        } else {
            ((0, 0).into(), (0, 0).into())
        };

        let min_width = min_size.w.max(1);
        let min_height = min_size.h.max(1);
        let max_width = if max_size.w == 0 {
            i32::MAX
        } else {
            max_size.w
        };
        let max_height = if max_size.h == 0 {
            i32::MAX
        } else {
            max_size.h
        };

        new_window_width = new_window_width.max(min_width).min(max_width);
        new_window_height = new_window_height.max(min_height).min(max_height);

        self.last_window_size = (new_window_width, new_window_height).into();

        match self.window.0.underlying_surface() {
            WindowSurface::Wayland(xdg) => {
                xdg.with_pending_state(|state| {
                    state.states.set(xdg_toplevel::State::Resizing);
                    state.size = Some(self.last_window_size);
                });
                xdg.send_pending_configure();
            }
            #[cfg(feature = "xwayland")]
            WindowSurface::X11(x11) => {
                let location = data
                    .space()
                    .element_location(&self.window)
                    .unwrap_or_else(|| {
                        tracing::warn!("No location for window during resize");
                        Point::from((0, 0))
                    });
                if let Err(e) = x11.configure(Rectangle::new(location, self.last_window_size)) {
                    tracing::error!("Failed to configure X11 window during resize: {:?}", e);
                }
            }
        }
    }

    fn frame(
        &mut self,
        _data: &mut StilchState<BackendData>,
        _handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        _seq: Serial,
    ) {
    }

    fn cancel(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        seq: Serial,
    ) {
        handle.cancel(data, seq);
        handle.unset_grab(self, data);
    }

    fn shape(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        event: &smithay::input::touch::ShapeEvent,
        seq: Serial,
    ) {
        handle.shape(data, event, seq);
    }

    fn orientation(
        &mut self,
        data: &mut StilchState<BackendData>,
        handle: &mut smithay::input::touch::TouchInnerHandle<'_, StilchState<BackendData>>,
        event: &smithay::input::touch::OrientationEvent,
        seq: Serial,
    ) {
        handle.orientation(data, event, seq);
    }

    fn start_data(&self) -> &smithay::input::touch::GrabStartData<StilchState<BackendData>> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut StilchState<BackendData>) {
        // Mark any moved windows for update
        data.window_manager.mark_moved_windows();

        // Find the window in the registry and update workspace layout if needed
        if let Some(window_id) = data.window_registry().find_by_element(&self.window) {
            if let Some(managed_window) = data.window_registry().get(window_id) {
                let workspace_id = managed_window.workspace;

                // Get the final position from space
                if let Some(_final_position) = data.space().element_location(&self.window) {
                    // Update the workspace layout with the new position
                    // This ensures the layout system knows about the manual position change
                    if let Some(workspace) = data.workspace_manager.get_workspace_mut(workspace_id)
                    {
                        // For now, just mark that the workspace needs relayout
                        // In the future, we might want to update the layout directly
                        workspace.relayout();
                    }
                }
            }
        }
    }
}
