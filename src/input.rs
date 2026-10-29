//! Turns hardware input frames into taps, slider changes and Touch ID
//! cancellation. Touch ID handling is kept identical to the built-in
//! renderer: a press on the overlay cancels, covered taps are ignored.

use std::collections::BTreeMap;
use std::time::Duration;

use t1_touchbar::gesture::{
    ContactId, DisplayContact, Gesture, GestureEngine, GestureSettings,
};
use t1_touchbar::overlay_state::OverlayState;
use t1_touchbar::touch_id_overlay::TouchIdOverlay;
use t1_touchbar_hw::digitizer::DisplayDimensions;
use t1_touchbar_hw::wire::{self, InputContact, WireInputFrame};

use crate::bar::{Action, Bar, Contextual, SliderKind, Surface, Target};
use crate::slider::{AUTO_CLOSE_AFTER, SliderState};
use crate::touch_id_view;

/// The lock key must be held this long; a shorter touch does nothing.
pub const LOCK_HOLD: Duration = Duration::from_millis(1_800);
/// Holding the Bluetooth key this long opens the device list instead of
/// toggling Bluetooth.
pub const BLUETOOTH_HOLD: Duration = Duration::from_millis(500);
/// The device list stays open longer than the output picker: discovery
/// and connections take a few seconds.
const DEVICES_IDLE: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputError;

#[derive(Debug, Default, PartialEq)]
pub struct InputOutcome {
    pub redraw: bool,
    pub actions: Vec<Action>,
    /// New slider values, in order; the session forwards the newest.
    pub changes: Vec<(SliderKind, f64)>,
    pub cancel_touch_id: bool,
}

pub struct InputInterpreter {
    bar: Bar,
    touch_id: TouchIdOverlay,
    dimensions: DisplayDimensions,
    settings: GestureSettings,
    gestures: GestureEngine,
    fn_pressed: bool,
    /// Set when touches were cancelled under a finger (screen off): input
    /// stays ignored until every finger has lifted.
    pub wait_for_touch_release: bool,
    pressed: Option<Target>,
    press_targets: BTreeMap<ContactId, (Surface, Target)>,
    slider: SliderState,
    /// Contact dragging along the now-playing title, and its position.
    scrub: Option<(ContactId, Option<f64>)>,
    /// Last activity while the audio output picker is open.
    outputs_open: Option<Duration>,
    /// Contact holding the lock key, and when it started.
    lock_hold: Option<(ContactId, Duration)>,
    /// Contact holding the Bluetooth key, and when it started.
    bluetooth_hold: Option<(ContactId, Duration)>,
    /// Last activity while the Bluetooth device picker is open.
    devices_open: Option<Duration>,
}

impl InputInterpreter {
    /// # Errors
    ///
    /// Rejects dimensions that produce invalid gesture settings.
    pub fn new(dimensions: DisplayDimensions, bar: Bar) -> Result<Self, InputError> {
        let drag_distance = (f64::from(dimensions.height()) / 4.0).max(1.0);
        let settings = GestureSettings::new(drag_distance, Duration::MAX, wire::MAX_CONTACTS)
            .map_err(|_| InputError)?;
        Ok(Self {
            bar,
            touch_id: TouchIdOverlay::new(dimensions.width(), dimensions.height()),
            dimensions,
            settings,
            gestures: GestureEngine::new(settings),
            fn_pressed: false,
            wait_for_touch_release: false,
            pressed: None,
            press_targets: BTreeMap::new(),
            slider: SliderState::default(),
            scrub: None,
            outputs_open: None,
            lock_hold: None,
            bluetooth_hold: None,
            devices_open: None,
        })
    }

    #[must_use]
    pub const fn pressed(&self) -> Option<Target> {
        self.pressed
    }

    #[must_use]
    pub const fn slider(&self) -> &SliderState {
        &self.slider
    }

    pub fn slider_mut(&mut self) -> &mut SliderState {
        &mut self.slider
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        !self.wait_for_touch_release && self.gestures.is_idle()
    }

    /// Fill of the hold-to-lock bar, 0.0..=1.0, while the key is held.
    #[must_use]
    pub fn lock_progress(&self, now: Duration) -> Option<f64> {
        self.lock_hold.map(|(_, since)| {
            (now.saturating_sub(since).as_secs_f64() / LOCK_HOLD.as_secs_f64()).min(1.0)
        })
    }

    /// True once when the hold completes; the hold then ends.
    pub fn take_lock(&mut self, now: Duration) -> bool {
        let done = self.lock_progress(now).is_some_and(|progress| progress >= 1.0);
        if done {
            self.lock_hold = None;
        }
        done
    }

    /// Opens the device picker once the Bluetooth key has been held long
    /// enough; the lift of that finger then toggles nothing.
    pub fn take_bluetooth_hold(&mut self, now: Duration) -> bool {
        let Some((holder, since)) = self.bluetooth_hold else {
            return false;
        };
        if now.saturating_sub(since) < BLUETOOTH_HOLD {
            return false;
        }
        self.bluetooth_hold = None;
        self.press_targets.remove(&holder);
        self.slider.close();
        self.outputs_open = None;
        self.devices_open = Some(now);
        true
    }

    /// A Bluetooth key hold is being timed.
    #[must_use]
    pub const fn bluetooth_holding(&self) -> bool {
        self.bluetooth_hold.is_some()
    }

    #[must_use]
    pub const fn outputs_open(&self) -> bool {
        self.outputs_open.is_some()
    }

    #[must_use]
    pub const fn devices_open(&self) -> bool {
        self.devices_open.is_some()
    }

    fn close_pickers(&mut self) {
        self.outputs_open = None;
        self.devices_open = None;
    }

    /// Closes a picker after its idle timeout; returns whether one closed.
    pub fn expire_pickers(&mut self, now: Duration) -> bool {
        let idle = self.gestures.is_idle();
        let outputs = self
            .outputs_open
            .is_some_and(|since| idle && now.saturating_sub(since) >= AUTO_CLOSE_AFTER);
        let devices = self
            .devices_open
            .is_some_and(|since| idle && now.saturating_sub(since) >= DEVICES_IDLE);
        if outputs || devices {
            self.close_pickers();
        }
        outputs || devices
    }

    /// Seek preview while the title is being dragged.
    #[must_use]
    pub fn scrub(&self) -> Option<f64> {
        self.scrub.and_then(|(_, fraction)| fraction)
    }

    /// Updates which contextual elements exist; safe under a finger, since
    /// taps are matched against the target recorded at press time.
    pub fn set_contextual(&mut self, contextual: Contextual) {
        self.bar = self.bar.with_contextual(contextual);
        if !contextual.now_playing {
            self.scrub = None;
        }
        if contextual.outputs == 0 {
            self.outputs_open = None;
        }
        if !contextual.bluetooth {
            self.devices_open = None;
            self.bluetooth_hold = None;
        }
    }

    /// Replaces the capability-dependent geometry; only call while idle.
    pub fn set_bar(&mut self, bar: Bar) {
        let bar = bar.with_contextual(self.bar.contextual());
        self.bar = bar;
        if let Some(kind) = self.slider.kind()
            && !bar.slider_available(kind)
        {
            self.slider.close();
        }
    }

    /// The surface input is interpreted against.
    #[must_use]
    pub fn surface(&self) -> Surface {
        if self.fn_pressed {
            Surface::Function
        } else if self.devices_open.is_some() {
            Surface::Devices
        } else if self.outputs_open.is_some() {
            Surface::Outputs
        } else {
            self.slider.kind().map_or(Surface::Normal, Surface::Slider)
        }
    }

    /// Feeds one frame. `level` returns the current value of a control when
    /// its slider opens.
    ///
    /// # Errors
    ///
    /// Rejects malformed contacts or gesture sequences.
    pub fn ingest(
        &mut self,
        frame: &WireInputFrame,
        overlay_state: Option<OverlayState>,
        now: Duration,
        mut level: impl FnMut(SliderKind) -> f64,
    ) -> Result<InputOutcome, InputError> {
        let mut outcome = InputOutcome {
            redraw: self.fn_pressed != frame.fn_pressed,
            ..InputOutcome::default()
        };
        if frame.fn_pressed && !self.fn_pressed {
            self.slider.close();
            self.close_pickers();
        }
        self.fn_pressed = frame.fn_pressed;
        let contacts = frame
            .contacts
            .iter()
            .filter(|contact| contact.tip)
            .map(display_contact)
            .collect::<Result<Vec<_>, _>>()?;
        let gestures = self
            .gestures
            .ingest_frame(Duration::from_nanos(frame.monotonic_ns), &contacts)
            .map_err(|_| InputError)?;

        for gesture in gestures {
            let leo_camp_cancel = matches!(gesture, Gesture::Press(contact) if touch_id_view::cancels(
                self.dimensions.width(),
                self.dimensions.height(),
                overlay_state,
                contact.x(),
            ));
            if leo_camp_cancel
                || self
                    .touch_id
                    .cancellation_for_gesture(gesture, overlay_state, self.fn_pressed)
            {
                outcome.cancel_touch_id = true;
                continue;
            }
            match gesture {
                Gesture::Press(contact) => {
                    if !self.covered(contact, overlay_state) {
                        self.press(contact, now, &mut level, &mut outcome);
                    }
                }
                Gesture::DragStart(start) => {
                    if let Some((contact, None)) = self.scrub
                        && contact == start.id()
                    {
                        self.scrub = Some((contact, Some(self.bar.now_playing_fraction(start.x()))));
                    }
                }
                Gesture::Drag { to, .. }
                    if self.scrub.is_some_and(|(contact, _)| contact == to.id()) =>
                {
                    self.scrub = Some((to.id(), Some(self.bar.now_playing_fraction(to.x()))));
                    outcome.redraw = true;
                }
                Gesture::Move { to, .. } | Gesture::Drag { to, .. } => {
                    if let Some(fraction) = self.slider.drag_to(
                        to.id(),
                        to.x(),
                        self.bar.track_fraction(to.x()),
                        self.bar.track_width(),
                        now,
                    ) && let Some(kind) = self.slider.kind()
                    {
                        outcome.changes.push((kind, fraction));
                        outcome.redraw = true;
                    }
                }
                Gesture::Release(contact) => {
                    self.slider.release(contact.id(), now);
                    if let Some((scrubbing, position)) = self.scrub
                        && scrubbing == contact.id()
                    {
                        self.scrub = None;
                        outcome.redraw = true;
                        if let Some(position) = position {
                            outcome.actions.push(Action::Seek(to_permille(position)));
                        }
                    }
                }
                Gesture::Tap(contact) => {
                    if !self.covered(contact, overlay_state) {
                        self.tap(contact, now, &mut outcome);
                    }
                }
                Gesture::Hold(_) => {}
            }
        }
        self.press_targets
            .retain(|id, _| contacts.iter().any(|contact| contact.id() == *id));
        // The hold ends when the finger lifts or slides off the lock key.
        if let Some((holder, _)) = self.lock_hold {
            let still_on_key = contacts.iter().any(|contact| {
                contact.id() == holder
                    && self.bar.target_at(contact.x(), contact.y(), self.surface())
                        == Some(Target::Action(Action::LockScreen))
            });
            if !still_on_key {
                self.lock_hold = None;
                outcome.redraw = true;
            }
        }
        if let Some((holder, _)) = self.bluetooth_hold {
            let still_on_key = contacts.iter().any(|contact| {
                contact.id() == holder
                    && self.bar.target_at(contact.x(), contact.y(), self.surface())
                        == Some(Target::Action(Action::ToggleBluetooth))
            });
            if !still_on_key {
                self.bluetooth_hold = None;
            }
        }

        let surface = self.surface();
        let pressed = contacts
            .first()
            .filter(|contact| !self.covered(**contact, overlay_state))
            .and_then(|contact| self.bar.target_at(contact.x(), contact.y(), surface))
            .filter(|target| {
                matches!(
                    target,
                    Target::Action(_) | Target::Close | Target::NowPlaying | Target::OutputsButton
                )
            });
        outcome.redraw |= self.pressed != pressed;
        self.pressed = pressed;
        Ok(outcome)
    }

    fn press(
        &mut self,
        contact: DisplayContact,
        now: Duration,
        level: &mut impl FnMut(SliderKind) -> f64,
        outcome: &mut InputOutcome,
    ) {
        let surface = self.surface();
        let Some(target) = self.bar.target_at(contact.x(), contact.y(), surface) else {
            self.slider.touch(now);
            return;
        };
        self.press_targets.insert(contact.id(), (surface, target));
        match target {
            Target::SliderButton(kind) if self.slider.kind().is_none() => {
                // macOS: the slider opens on touch and can be dragged at once.
                self.slider.open(kind, level(kind), now);
                self.slider.begin_relative(contact.id(), contact.x());
                outcome.redraw = true;
            }
            Target::Action(Action::LockScreen) => {
                self.lock_hold = Some((contact.id(), now));
                outcome.redraw = true;
            }
            Target::Action(Action::ToggleBluetooth) => {
                self.bluetooth_hold = Some((contact.id(), now));
            }
            Target::NowPlaying => {
                if self.bar.contextual().seekable && self.scrub.is_none() {
                    self.scrub = Some((contact.id(), None));
                }
            }
            Target::Track if !self.slider.dragging() => {
                let fraction = self.bar.track_fraction(contact.x());
                if self.slider.begin_absolute(contact.id(), fraction, now)
                    && let Some(kind) = self.slider.kind()
                {
                    outcome.changes.push((kind, fraction));
                    outcome.redraw = true;
                }
            }
            _ => {
                self.slider.touch(now);
                if self.outputs_open.is_some() {
                    self.outputs_open = Some(now);
                }
                if self.devices_open.is_some() {
                    self.devices_open = Some(now);
                }
            }
        }
    }

    /// Taps fire only on the target that was pressed, on the surface that
    /// was visible at the time: opening a slider must not turn the lift of
    /// the same finger into a track tap.
    fn tap(&mut self, contact: DisplayContact, now: Duration, outcome: &mut InputOutcome) {
        let Some((surface, target)) = self.press_targets.remove(&contact.id()) else {
            return;
        };
        if self.bar.target_at(contact.x(), contact.y(), surface) != Some(target) {
            return;
        }
        match target {
            // Locking needs the full hold, never a tap.
            Target::Action(Action::LockScreen) => {}
            Target::Action(action) => {
                self.slider.touch(now);
                if matches!(action, Action::SelectOutput(_)) {
                    self.outputs_open = None;
                    outcome.redraw = true;
                }
                // The device list stays open to show the connection happen.
                if matches!(action, Action::SelectDevice(_)) {
                    self.devices_open = Some(now);
                }
                if action == Action::ToggleBluetooth {
                    self.bluetooth_hold = None;
                }
                if !outcome.actions.contains(&action) {
                    outcome.actions.push(action);
                }
            }
            Target::Close => {
                self.slider.close();
                self.close_pickers();
                outcome.redraw = true;
            }
            Target::OutputsButton => {
                self.slider.close();
                self.devices_open = None;
                self.outputs_open = Some(now);
                outcome.redraw = true;
            }
            Target::NowPlaying => {
                if !outcome.actions.contains(&Action::NowPlayingToggle) {
                    outcome.actions.push(Action::NowPlayingToggle);
                }
            }
            Target::SliderButton(_) | Target::Track => {}
        }
    }

    fn covered(&self, contact: DisplayContact, overlay_state: Option<OverlayState>) -> bool {
        self.touch_id
            .covers_contact(contact, overlay_state, self.fn_pressed)
    }

    /// Drops every active touch without taps (screen turned off).
    pub fn cancel_touches(&mut self) {
        self.wait_for_touch_release |= !self.gestures.is_idle();
        self.gestures = GestureEngine::new(self.settings);
        self.press_targets.clear();
        self.pressed = None;
        self.scrub = None;
        self.lock_hold = None;
        self.bluetooth_hold = None;
        self.slider.close();
        self.close_pickers();
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_permille(fraction: f64) -> u16 {
    (fraction.clamp(0.0, 1.0) * 1_000.0).round() as u16
}

fn display_contact(contact: &InputContact) -> Result<DisplayContact, InputError> {
    DisplayContact::new(
        u32::from(contact.id),
        f64::from(contact.x),
        f64::from(contact.y),
    )
    .map_err(|_| InputError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use t1_touchbar::stock_slice::{FunctionKey, StockCapabilities};

    const WIDTH: u32 = 2_170;
    const HEIGHT: u32 = 60;

    fn interpreter() -> InputInterpreter {
        let dimensions = DisplayDimensions::new(WIDTH, HEIGHT).unwrap();
        let bar = Bar::new(
            WIDTH,
            HEIGHT,
            StockCapabilities::DISPLAY_BRIGHTNESS
                | StockCapabilities::KEYBOARD_BACKLIGHT
                | StockCapabilities::AUDIO
                | StockCapabilities::MEDIA,
        )
        .unwrap();
        InputInterpreter::new(dimensions, bar).unwrap()
    }

    fn frame(ms: u64, fn_pressed: bool, contacts: &[(u8, u32)]) -> WireInputFrame {
        WireInputFrame {
            monotonic_ns: ms * 1_000_000,
            fn_pressed,
            contacts: contacts
                .iter()
                .map(|&(id, x)| InputContact {
                    id,
                    x,
                    y: 30,
                    tip: true,
                    in_range: true,
                })
                .collect(),
        }
    }

    fn feed(input: &mut InputInterpreter, frame: &WireInputFrame) -> InputOutcome {
        let now = Duration::from_nanos(frame.monotonic_ns);
        input.ingest(frame, None, now, |_| 0.5).unwrap()
    }

    #[test]
    fn tap_on_escape_fires_once() {
        let mut input = interpreter();
        assert!(feed(&mut input, &frame(0, false, &[(1, 20)])).actions.is_empty());
        assert_eq!(input.pressed(), Some(Target::Action(Action::Escape)));
        let outcome = feed(&mut input, &frame(50, false, &[]));
        assert_eq!(outcome.actions, vec![Action::Escape]);
        assert_eq!(input.pressed(), None);
    }

    #[test]
    fn touching_volume_opens_slider_and_tap_keeps_it_open() {
        let mut input = interpreter();
        feed(&mut input, &frame(0, false, &[(1, 1_970)]));
        assert_eq!(input.surface(), Surface::Slider(SliderKind::Volume));
        let lift = feed(&mut input, &frame(40, false, &[]));
        assert!(lift.changes.is_empty(), "lifting must not set the level");
        assert!(lift.actions.is_empty());
        assert_eq!(input.surface(), Surface::Slider(SliderKind::Volume));
    }

    #[test]
    fn press_and_slide_adjusts_relative_to_the_current_level() {
        let mut input = interpreter();
        feed(&mut input, &frame(0, false, &[(1, 1_970)]));
        let outcome = feed(&mut input, &frame(10, false, &[(1, 1_225)]));
        let (kind, value) = *outcome.changes.last().unwrap();
        assert_eq!(kind, SliderKind::Volume);
        // 745 px left on a 1490 px track: from 0.5 down to 0.0.
        assert!(value.abs() < 1e-9, "{value}");
    }

    #[test]
    fn track_touch_jumps_and_close_button_closes() {
        let mut input = interpreter();
        feed(&mut input, &frame(0, false, &[(1, 1_400)]));
        feed(&mut input, &frame(10, false, &[]));
        assert_eq!(input.surface(), Surface::Slider(SliderKind::Display));
        let outcome = feed(&mut input, &frame(20, false, &[(2, 1_960)]));
        assert_eq!(outcome.changes, vec![(SliderKind::Display, 1.0)]);
        feed(&mut input, &frame(30, false, &[]));
        feed(&mut input, &frame(40, false, &[(3, 250)]));
        assert_eq!(input.pressed(), Some(Target::Close));
        feed(&mut input, &frame(50, false, &[]));
        assert_eq!(input.surface(), Surface::Normal);
    }

    #[test]
    fn fn_shows_function_row_and_closes_slider() {
        let mut input = interpreter();
        feed(&mut input, &frame(0, false, &[(1, 1_970)]));
        feed(&mut input, &frame(10, false, &[]));
        feed(&mut input, &frame(20, true, &[]));
        assert_eq!(input.surface(), Surface::Function);
        feed(&mut input, &frame(30, true, &[(2, 250)]));
        let outcome = feed(&mut input, &frame(40, true, &[]));
        assert_eq!(outcome.actions, vec![Action::Function(FunctionKey::F1)]);
        feed(&mut input, &frame(50, false, &[]));
        assert_eq!(input.surface(), Surface::Normal);
    }

    #[test]
    fn now_playing_tap_pauses_and_drag_seeks_on_release() {
        let mut input = interpreter();
        input.set_contextual(Contextual {
            now_playing: true,
            seekable: true,
            microphone: false,
            outputs: 0,
            bluetooth: true,
            devices: 0,
        });
        feed(&mut input, &frame(0, false, &[(1, 1_000)]));
        let outcome = feed(&mut input, &frame(30, false, &[]));
        assert_eq!(outcome.actions, vec![Action::NowPlayingToggle]);

        feed(&mut input, &frame(100, false, &[(2, 1_000)]));
        let moving = feed(&mut input, &frame(110, false, &[(2, 1_060)]));
        assert!(moving.actions.is_empty(), "seek waits for the release");
        assert!((input.scrub().unwrap() - 0.5).abs() < 1e-9);
        let release = feed(&mut input, &frame(120, false, &[]));
        assert_eq!(release.actions, vec![Action::Seek(500)]);
        assert_eq!(input.scrub(), None);
    }

    #[test]
    fn output_picker_opens_selects_and_closes() {
        let mut input = interpreter();
        input.set_contextual(Contextual {
            now_playing: false,
            seekable: false,
            microphone: false,
            outputs: 2,
            bluetooth: true,
            devices: 0,
        });
        feed(&mut input, &frame(0, false, &[(1, 1_700)]));
        feed(&mut input, &frame(30, false, &[]));
        assert_eq!(input.surface(), Surface::Outputs);
        feed(&mut input, &frame(60, false, &[(2, 900)]));
        let outcome = feed(&mut input, &frame(90, false, &[]));
        assert_eq!(outcome.actions, vec![Action::SelectOutput(1)]);
        assert_eq!(input.surface(), Surface::Normal);

        feed(&mut input, &frame(200, false, &[(3, 1_700)]));
        feed(&mut input, &frame(230, false, &[]));
        assert!(!input.expire_pickers(Duration::from_millis(1_000)));
        assert!(input.expire_pickers(Duration::from_millis(4_300)));
        assert_eq!(input.surface(), Surface::Normal);
    }

    #[test]
    fn lock_needs_a_full_hold_on_the_key() {
        let mut input = interpreter();
        // A tap does nothing.
        feed(&mut input, &frame(0, false, &[(1, 480)]));
        let outcome = feed(&mut input, &frame(40, false, &[]));
        assert!(outcome.actions.is_empty());
        assert_eq!(input.lock_progress(Duration::from_millis(50)), None);

        // Lifting early cancels.
        feed(&mut input, &frame(100, false, &[(2, 480)]));
        assert!(input.lock_progress(Duration::from_millis(1_000)).unwrap() > 0.4);
        feed(&mut input, &frame(1_000, false, &[]));
        assert!(!input.take_lock(Duration::from_millis(3_000)));

        // Sliding off the key cancels.
        feed(&mut input, &frame(3_000, false, &[(3, 480)]));
        feed(&mut input, &frame(3_100, false, &[(3, 900)]));
        assert_eq!(input.lock_progress(Duration::from_millis(3_200)), None);
        feed(&mut input, &frame(3_200, false, &[]));

        // A full hold locks once.
        feed(&mut input, &frame(5_000, false, &[(4, 480)]));
        assert!(!input.take_lock(Duration::from_millis(6_700)));
        assert!(input.take_lock(Duration::from_millis(6_800)));
        assert!(!input.take_lock(Duration::from_millis(6_900)));
    }

    #[test]
    fn bluetooth_tap_toggles_and_hold_opens_the_device_list() {
        let mut input = interpreter();
        input.set_contextual(Contextual {
            now_playing: false,
            seekable: false,
            microphone: false,
            outputs: 0,
            bluetooth: true,
            devices: 2,
        });
        feed(&mut input, &frame(0, false, &[(1, 720)]));
        assert!(!input.take_bluetooth_hold(Duration::from_millis(100)));
        let outcome = feed(&mut input, &frame(150, false, &[]));
        assert_eq!(outcome.actions, vec![Action::ToggleBluetooth]);
        assert!(!input.take_bluetooth_hold(Duration::from_millis(1_000)));

        // Held: the list opens and the lift toggles nothing.
        feed(&mut input, &frame(2_000, false, &[(2, 720)]));
        assert!(input.take_bluetooth_hold(Duration::from_millis(2_500)));
        assert_eq!(input.surface(), Surface::Devices);
        let lift = feed(&mut input, &frame(2_600, false, &[]));
        assert!(lift.actions.is_empty());

        // Picking a device keeps the list open.
        feed(&mut input, &frame(2_700, false, &[(3, 900)]));
        let pick = feed(&mut input, &frame(2_730, false, &[]));
        assert_eq!(pick.actions, vec![Action::SelectDevice(1)]);
        assert_eq!(input.surface(), Surface::Devices);
        assert!(!input.expire_pickers(Duration::from_millis(10_000)));
        assert!(input.expire_pickers(Duration::from_millis(23_000)));
        assert_eq!(input.surface(), Surface::Normal);
    }

    #[test]
    fn media_tap_does_not_reach_a_slider() {
        let mut input = interpreter();
        input.set_contextual(Contextual {
            now_playing: true,
            seekable: true,
            microphone: false,
            outputs: 0,
            bluetooth: true,
            devices: 0,
        });
        feed(&mut input, &frame(0, false, &[(1, 880)]));
        let outcome = feed(&mut input, &frame(30, false, &[]));
        assert_eq!(outcome.actions, vec![Action::MediaPrevious]);
        assert_eq!(input.surface(), Surface::Normal);
    }
}
