//! One hardware connection: frame submission, input dispatch, Touch ID
//! overlay and the value writers behind the sliders. Frame, request and
//! overlay handling follow the built-in renderer (MIT, `LICENSE-t1bridge`).

use std::collections::BTreeMap;
use std::thread;
use std::time::{Duration, Instant};

use t1_platform::frame_memfd::RendererFrame;
use t1_touchbar::framebuffer::{Rectangle, Rgb, Xrgb8888Frame};
use t1_touchbar::overlay_state::{OverlayState, OverlayWatcher};
use t1_touchbar::stock_slice::{FunctionKey, StepDirection, StockCapabilities};
use t1_touchbar::touch_id_overlay::TouchIdOverlay;
use t1_touchbar_hw::digitizer::DisplayDimensions;
use t1_touchbar_hw::wire::{
    ClientMessage, Envelope, ErrorCode, FeatureBits, KeyCode, Percentage, RegisterBuffer,
    ServiceMessage, StepDirection as WireStepDirection, SubmitFrame, TapKeys, WireInputFrame,
};

use crate::bar::{Action, Bar, Contextual, NowPlayingView, SliderKind, Surface, View};
use crate::connection::{
    self, HardwareConnection, MAX_PENDING_REQUESTS, NegotiatedConnection, RendererError,
    RequestIds,
};
use crate::context::{Context, Snapshot};
use crate::input::InputInterpreter;
use crate::levels::LevelSource;
use crate::mpris::{NowPlaying, Playback};
use crate::slider::{DisplayStepper, Throttle};
use crate::system::{self, DoNotDisturb};
use crate::text::{TextMask, TextRenderer};

const BUFFER_ID: u32 = 1;
const FIRST_FRAME_ID: u64 = 1;
/// Socket polling period right after input or while something animates...
const RECEIVE_RETRY_DELAY: Duration = Duration::from_millis(5);
/// ...and while the bar sits idle (first touch latency, not frame rate).
const IDLE_RECEIVE_DELAY: Duration = Duration::from_millis(15);
const ACTIVE_AFTER_INPUT: Duration = Duration::from_secs(1);
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(50);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(2);
const OVERLAY_FADE_DURATION: Duration = Duration::from_millis(150);
const ENROLLMENT_PROGRESS_DURATION: Duration = Duration::from_millis(220);
/// Volume changes are coalesced before reaching `wpctl`.
const VOLUME_INTERVAL: Duration = Duration::from_millis(40);
/// The hardware service rejects keyboard writes closer than 20 ms.
const KEYBOARD_INTERVAL: Duration = Duration::from_millis(30);
/// How often the display level is sampled while it matters.
const LEVEL_POLL_INTERVAL: Duration = Duration::from_millis(25);
/// A volume reading may predate our own change by one poll; ignore it.
const VOLUME_ECHO_GRACE: Duration = Duration::from_millis(1_000);

/// Runs the renderer and reconnects forever with capped backoff.
pub fn run_forever() {
    let context = Context::start();
    let mut delay = INITIAL_RECONNECT_DELAY;
    loop {
        if let Err(error) = run_once(&context) {
            eprintln!("touchbar-renderer: {error}; reconnecting");
        }
        thread::sleep(delay);
        delay = delay.saturating_mul(2).min(MAX_RECONNECT_DELAY);
    }
}

fn run_once(context: &Context) -> Result<(), RendererError> {
    let NegotiatedConnection {
        mut connection,
        request_ids,
        ack,
        features,
    } = connection::negotiate()?;
    let dimensions =
        DisplayDimensions::new(ack.width, ack.height).map_err(|_| RendererError::Negotiation)?;
    let mut session = Session::new(
        dimensions,
        request_ids,
        hardware_capabilities(features, ack.minor),
        context.clone(),
    )?;
    session.register(&connection)?;
    let overlay_watcher = OverlayWatcher::start();
    let clock = Instant::now();

    loop {
        session.advance(clock.elapsed(), &connection)?;
        if let Some(observation) = overlay_watcher.take_latest() {
            let overlay = observation.overlay();
            session.set_overlay(
                overlay.map(|overlay| overlay.state),
                overlay.and_then(|overlay| overlay.enrollment_progress),
            );
            session.submit_if_ready(&connection)?;
        }
        if let Some(envelope) = connection.try_receive_service(Some(dimensions))? {
            session.handle(envelope, &connection)?;
        } else if session.active() {
            thread::sleep(RECEIVE_RETRY_DELAY);
        } else {
            thread::sleep(IDLE_RECEIVE_DELAY);
        }
    }
}

fn hardware_capabilities(features: FeatureBits, protocol_minor: u16) -> StockCapabilities {
    let mut capabilities = StockCapabilities::default();
    if protocol_minor >= 1 && features.contains(FeatureBits::DISPLAY_BRIGHTNESS) {
        capabilities = capabilities | StockCapabilities::DISPLAY_BRIGHTNESS;
    }
    if protocol_minor >= 1 && features.contains(FeatureBits::KEYBOARD_BACKLIGHT) {
        capabilities = capabilities | StockCapabilities::KEYBOARD_BACKLIGHT;
    }
    capabilities
}

fn desktop_capabilities(snapshot: &Snapshot) -> StockCapabilities {
    let mut capabilities = StockCapabilities::default();
    if snapshot.audio.is_some() {
        capabilities = capabilities | StockCapabilities::AUDIO;
    }
    if snapshot.now_playing.is_some() {
        capabilities = capabilities | StockCapabilities::MEDIA;
    }
    capabilities
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingRequest {
    Register,
    Submit { frame_id: u64 },
    TapKeys,
    CancelTouchId,
    HardwareAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InFlightFrame {
    frame_id: u64,
    acknowledged: bool,
}

/// Smoothly animated value toward a target, as in the built-in renderer.
#[derive(Clone, Copy, Debug, Default)]
struct Fade {
    value: u16,
    start: u16,
    target: u16,
    since: Duration,
}

impl Fade {
    fn retarget(&mut self, target: u16, now: Duration) {
        self.start = self.value;
        self.target = target;
        self.since = now;
    }

    /// Returns whether the value changed.
    fn advance(&mut self, now: Duration, duration: Duration) -> bool {
        if self.value == self.target {
            return false;
        }
        let total = duration.as_nanos().max(1);
        let progress = now.saturating_sub(self.since).as_nanos().min(total);
        let span = u128::from(self.start.abs_diff(self.target));
        let moved = u16::try_from(span * progress / total).unwrap_or(u16::MAX);
        self.value = if self.target >= self.start {
            self.start.saturating_add(moved).min(self.target)
        } else {
            self.start.saturating_sub(moved).max(self.target)
        };
        true
    }
}

struct Session {
    dimensions: DisplayDimensions,
    stride: u32,
    byte_length: u64,
    bar: Bar,
    hardware_capabilities: StockCapabilities,
    /// Capabilities waiting for every finger to lift before relayout.
    pending_capabilities: Option<StockCapabilities>,
    last_input: Duration,
    /// Locked or blanked desktop: dark panel, touches ignored.
    screen_off: bool,
    volume: Option<u8>,
    muted: bool,
    touch_id: TouchIdOverlay,
    overlay_state: Option<OverlayState>,
    overlay_visual_state: Option<OverlayState>,
    overlay: Fade,
    enrollment_progress: Option<u8>,
    enrollment: Fade,
    now: Duration,
    input: InputInterpreter,
    frame: RendererFrame,
    registered: bool,
    dirty: bool,
    in_flight: Option<InFlightFrame>,
    next_frame_id: u64,
    request_ids: RequestIds,
    pending: BTreeMap<u32, PendingRequest>,
    display_level: Option<LevelSource>,
    keyboard_level: Option<LevelSource>,
    stepper: DisplayStepper,
    last_level_poll: Duration,
    keyboard: Throttle,
    volume_writer: Throttle,
    volume_set_at: Option<Duration>,
    do_not_disturb: DoNotDisturb,
    shown_do_not_disturb: bool,
    context: Context,
    context_generation: u64,
    snapshot: Snapshot,
    text: Option<TextRenderer>,
    /// The rendered now-playing label and the string it was made from.
    label: Option<(String, TextMask)>,
    progress_pixel: Option<u32>,
}

impl Session {
    fn new(
        dimensions: DisplayDimensions,
        request_ids: RequestIds,
        capabilities: StockCapabilities,
        context: Context,
    ) -> Result<Self, RendererError> {
        let stride = dimensions
            .width()
            .checked_mul(4)
            .ok_or(RendererError::Negotiation)?;
        let byte_length = u64::from(stride)
            .checked_mul(u64::from(dimensions.height()))
            .ok_or(RendererError::Negotiation)?;
        let frame = RendererFrame::new(byte_length).map_err(|_| RendererError::Frame)?;
        let bar = Bar::new(dimensions.width(), dimensions.height(), capabilities)
            .map_err(|_| RendererError::Negotiation)?;
        let input = InputInterpreter::new(dimensions, bar).map_err(|_| RendererError::Gesture)?;
        Ok(Self {
            dimensions,
            stride,
            byte_length,
            bar,
            hardware_capabilities: capabilities,
            pending_capabilities: None,
            last_input: Duration::ZERO,
            screen_off: false,
            volume: None,
            muted: false,
            touch_id: TouchIdOverlay::new(dimensions.width(), dimensions.height()),
            overlay_state: None,
            overlay_visual_state: None,
            overlay: Fade::default(),
            enrollment_progress: None,
            enrollment: Fade::default(),
            now: Duration::ZERO,
            input,
            frame,
            registered: false,
            dirty: true,
            in_flight: None,
            next_frame_id: FIRST_FRAME_ID,
            request_ids,
            pending: BTreeMap::new(),
            display_level: LevelSource::display(),
            keyboard_level: LevelSource::keyboard(),
            stepper: DisplayStepper::default(),
            last_level_poll: Duration::ZERO,
            keyboard: Throttle::default(),
            volume_writer: Throttle::default(),
            volume_set_at: None,
            do_not_disturb: DoNotDisturb::start(),
            shown_do_not_disturb: false,
            context,
            context_generation: 0,
            snapshot: Snapshot::default(),
            text: TextRenderer::load(),
            label: None,
            progress_pixel: None,
        })
    }

    fn register(&mut self, connection: &HardwareConnection) -> Result<(), RendererError> {
        let message = ClientMessage::RegisterBuffer(RegisterBuffer {
            buffer_id: BUFFER_ID,
            stride: self.stride,
            byte_length: self.byte_length,
        });
        let request_id = self.allocate_request(PendingRequest::Register)?;
        let envelope = Envelope {
            request_id,
            message,
        };
        if let Err(error) = connection.send_request(
            &envelope,
            Some(self.dimensions),
            Some(self.frame.descriptor()),
        ) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        Ok(())
    }

    fn handle(
        &mut self,
        envelope: Envelope<ServiceMessage>,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        match envelope.message {
            ServiceMessage::Ack => self.handle_ack(envelope.request_id, connection),
            ServiceMessage::Error(code) => self.handle_error(envelope.request_id, code),
            ServiceMessage::FrameReleased(release) => {
                if envelope.request_id != 0 || release.buffer_id != BUFFER_ID {
                    return Err(RendererError::Protocol);
                }
                let Some(in_flight) = self.in_flight else {
                    return Err(RendererError::Protocol);
                };
                if !in_flight.acknowledged || release.frame_id != in_flight.frame_id {
                    return Err(RendererError::Protocol);
                }
                self.in_flight = None;
                self.submit_if_ready(connection)
            }
            ServiceMessage::InputFrame(frame) => {
                if envelope.request_id != 0 {
                    return Err(RendererError::Protocol);
                }
                self.observe_input(frame, connection)?;
                self.apply_pending_capabilities()?;
                self.submit_if_ready(connection)
            }
            ServiceMessage::HelloAck(_) => Err(RendererError::Protocol),
        }
    }

    fn handle_ack(
        &mut self,
        request_id: u32,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        let pending = self
            .pending
            .remove(&request_id)
            .ok_or(RendererError::Protocol)?;
        match pending {
            PendingRequest::Register if !self.registered => {
                self.registered = true;
                self.submit_if_ready(connection)
            }
            PendingRequest::Submit { frame_id } => {
                let Some(in_flight) = self.in_flight.as_mut() else {
                    return Err(RendererError::Protocol);
                };
                if in_flight.frame_id != frame_id || in_flight.acknowledged {
                    return Err(RendererError::Protocol);
                }
                in_flight.acknowledged = true;
                Ok(())
            }
            PendingRequest::TapKeys
            | PendingRequest::CancelTouchId
            | PendingRequest::HardwareAction => Ok(()),
            PendingRequest::Register => Err(RendererError::Protocol),
        }
    }

    fn handle_error(&mut self, request_id: u32, _code: ErrorCode) -> Result<(), RendererError> {
        let pending = self
            .pending
            .remove(&request_id)
            .ok_or(RendererError::Protocol)?;
        match pending {
            PendingRequest::TapKeys
            | PendingRequest::CancelTouchId
            | PendingRequest::HardwareAction => {
                eprintln!("touchbar-renderer: typed action was rejected");
                Ok(())
            }
            PendingRequest::Register | PendingRequest::Submit { .. } => {
                Err(RendererError::ServiceRejected)
            }
        }
    }

    /// While the screen is off the panel is dark: contacts are dropped
    /// and must all lift before a new gesture can start.
    fn observe_input(
        &mut self,
        mut frame: WireInputFrame,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        self.last_input = self.now;
        if self.screen_off || self.input.wait_for_touch_release {
            self.input.wait_for_touch_release = frame.contacts.iter().any(|contact| contact.tip);
            frame.contacts.clear();
        }
        let volume = self.volume;
        let display = self.display_level.as_ref();
        let keyboard = self.keyboard_level.as_ref();
        let stepping = self.stepper.target();
        let outcome = self
            .input
            .ingest(&frame, self.overlay_state, self.now, |kind| match kind {
                SliderKind::Volume => volume.map_or(0.0, |value| f64::from(value) / 100.0),
                SliderKind::Display => stepping
                    .or_else(|| display.and_then(LevelSource::fraction))
                    .unwrap_or(0.5),
                SliderKind::Keyboard => keyboard.and_then(LevelSource::fraction).unwrap_or(0.5),
            })
            .map_err(|_| RendererError::Gesture)?;
        self.dirty |= outcome.redraw;
        for action in outcome.actions {
            self.dispatch(action, connection)?;
        }
        for (kind, fraction) in outcome.changes {
            self.set_level(kind, fraction);
        }
        if outcome.cancel_touch_id {
            self.send(ClientMessage::CancelTouchId, PendingRequest::CancelTouchId, connection)?;
        }
        Ok(())
    }

    fn set_level(&mut self, kind: SliderKind, fraction: f64) {
        match kind {
            SliderKind::Volume => {
                let percent = to_percent(fraction);
                self.volume = Some(percent);
                self.muted = false;
                self.volume_writer.set(percent);
                self.volume_set_at = Some(self.now);
            }
            SliderKind::Keyboard => self.keyboard.set(to_percent(fraction)),
            SliderKind::Display => self.stepper.set_target(fraction),
        }
    }

    fn dispatch(
        &mut self,
        action: Action,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        match action {
            Action::Escape => self.send_key(KeyCode::Escape, connection),
            Action::Function(key) => self.send_key(function_key(key), connection),
            Action::ToggleMute => {
                self.muted = !self.muted;
                self.volume_set_at = Some(self.now);
                self.dirty = true;
                self.context.toggle_output_mute();
                Ok(())
            }
            Action::MediaPrevious => {
                self.media("Previous");
                Ok(())
            }
            Action::MediaPlayPause | Action::NowPlayingToggle => {
                self.media("PlayPause");
                if let Some(player) = self.snapshot.now_playing.as_mut() {
                    player.playback = match player.playback {
                        Playback::Playing => Playback::Paused,
                        Playback::Paused => Playback::Playing,
                    };
                    self.dirty = true;
                }
                Ok(())
            }
            Action::MediaNext => {
                self.media("Next");
                Ok(())
            }
            Action::Screenshot => self.send_key(KeyCode::Print, connection),
            Action::LockScreen => {
                system::lock_screen();
                Ok(())
            }
            Action::ToggleDoNotDisturb => {
                self.do_not_disturb.toggle();
                Ok(())
            }
            Action::ToggleMicrophone => {
                if let Some(microphone) = self.snapshot.microphone.as_mut() {
                    microphone.muted = !microphone.muted;
                    self.dirty = true;
                }
                self.context.toggle_microphone();
                Ok(())
            }
            Action::Seek(permille) => {
                if let Some(player) = self.snapshot.now_playing.as_ref() {
                    self.context.seek(player, f64::from(permille) / 1_000.0);
                }
                Ok(())
            }
        }
    }

    fn media(&self, method: &'static str) {
        if let Some(player) = self.snapshot.now_playing.as_ref() {
            self.context.media(&player.bus_name, method);
        }
    }

    /// Whether to poll the socket quickly: recent input or animation.
    fn active(&self) -> bool {
        self.now.saturating_sub(self.last_input) < ACTIVE_AFTER_INPUT
            || self.input.slider().kind().is_some()
            || self.stepper.busy()
            || self.input.scrub().is_some()
            || self.overlay.value != self.overlay.target
            || self.enrollment.value != self.enrollment.target
    }

    fn send_key(&mut self, key: KeyCode, connection: &HardwareConnection) -> Result<(), RendererError> {
        self.send(
            ClientMessage::TapKeys(TapKeys { keys: vec![key] }),
            PendingRequest::TapKeys,
            connection,
        )
    }

    fn send(
        &mut self,
        message: ClientMessage,
        kind: PendingRequest,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        let request_id = self.allocate_request(kind)?;
        let envelope = Envelope {
            request_id,
            message,
        };
        if let Err(error) = connection.send_request(&envelope, Some(self.dimensions), None) {
            self.pending.remove(&request_id);
            return Err(error);
        }
        Ok(())
    }

    fn submit_if_ready(&mut self, connection: &HardwareConnection) -> Result<(), RendererError> {
        if !self.registered || !self.dirty || self.in_flight.is_some() {
            return Ok(());
        }
        self.render()?;
        let frame_id = self.take_frame_id();
        self.send(
            ClientMessage::SubmitFrame(SubmitFrame {
                buffer_id: BUFFER_ID,
                frame_id,
                damage: Vec::new(),
            }),
            PendingRequest::Submit { frame_id },
            connection,
        )?;
        self.in_flight = Some(InFlightFrame {
            frame_id,
            acknowledged: false,
        });
        self.dirty = false;
        Ok(())
    }

    fn render(&mut self) -> Result<(), RendererError> {
        if self.screen_off {
            self.frame.as_mut_slice().fill(0);
            return Ok(());
        }
        let width = self.dimensions.width();
        let height = self.dimensions.height();
        let mut frame = Xrgb8888Frame::new(width, height, self.stride, self.frame.as_mut_slice())
            .map_err(|_| RendererError::Frame)?;
        // The Touch ID prompt keeps the normal row under it, as built-in does.
        let surface = match self.input.surface() {
            Surface::Function if self.overlay_visual_state.is_some() => Surface::Normal,
            surface => surface,
        };
        let now_playing = match (&self.snapshot.now_playing, &self.label) {
            (Some(player), Some((_, label))) => Some(NowPlayingView {
                label,
                playing: player.playback == Playback::Playing,
                progress: self
                    .input
                    .scrub()
                    .or_else(|| player.progress(Instant::now())),
            }),
            _ => None,
        };
        let view = View {
            surface,
            pressed: self.input.pressed(),
            muted: self.muted,
            do_not_disturb: self.shown_do_not_disturb,
            microphone: self.snapshot.microphone.map(|microphone| microphone.muted),
            now_playing,
            fraction: self.input.slider().fraction(),
        };
        self.bar
            .render(&mut frame, &view)
            .map_err(|_| RendererError::Frame)?;
        let opacity = u8::try_from(self.overlay.value).unwrap_or(u8::MAX);
        if opacity > 0 {
            let left = self.bar.escape_right();
            if left < width {
                frame
                    .blend_rectangle(
                        Rectangle {
                            x: left,
                            y: 0,
                            width: width - left,
                            height,
                        },
                        Rgb {
                            red: 0,
                            green: 0,
                            blue: 0,
                        },
                        opacity,
                    )
                    .map_err(|_| RendererError::Frame)?;
            }
        }
        self.touch_id
            .render_with_progress(
                &mut frame,
                self.overlay_visual_state,
                (self.overlay_visual_state == Some(OverlayState::Enrollment))
                    .then_some(self.enrollment.value),
                opacity,
            )
            .map_err(|_| RendererError::Frame)
    }

    fn set_overlay(&mut self, state: Option<OverlayState>, progress: Option<u8>) {
        if self.overlay_state == state && self.enrollment_progress == progress {
            return;
        }
        let state_changed = self.overlay_state != state;
        self.overlay_state = state;
        self.enrollment_progress = progress;
        self.enrollment
            .retarget(u16::from(progress.unwrap_or(0)) * 10, self.now);
        if state_changed {
            match state {
                Some(state) => {
                    self.input.slider_mut().close();
                    self.overlay_visual_state = Some(state);
                    self.overlay.retarget(u16::from(u8::MAX), self.now);
                }
                None => self.overlay.retarget(0, self.now),
            }
        }
        self.dirty = true;
    }

    fn advance(
        &mut self,
        now: Duration,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        self.now = now;
        if self.overlay.advance(now, OVERLAY_FADE_DURATION) {
            if self.overlay.value == 0 {
                self.overlay_visual_state = None;
            }
            self.dirty = true;
        }
        self.dirty |= self.enrollment.advance(now, ENROLLMENT_PROGRESS_DURATION);
        self.dirty |= self.input.slider_mut().expire(now);
        self.advance_context();
        self.apply_pending_capabilities()?;
        let do_not_disturb = self.do_not_disturb.active().unwrap_or(false);
        if do_not_disturb != self.shown_do_not_disturb {
            self.shown_do_not_disturb = do_not_disturb;
            self.dirty = true;
        }

        self.context
            .set_fast_audio(self.input.slider().kind() == Some(SliderKind::Volume));
        if let Some(percent) = self.volume_writer.take(now, VOLUME_INTERVAL) {
            self.context.set_volume(percent);
        }
        if let Some(percent) = self.keyboard.take(now, KEYBOARD_INTERVAL) {
            let percentage = Percentage::new(percent).map_err(|_| RendererError::Protocol)?;
            self.send(
                ClientMessage::SetKeyboardBacklight(percentage),
                PendingRequest::HardwareAction,
                connection,
            )?;
        }
        self.advance_display(now, connection)?;
        self.submit_if_ready(connection)
    }

    /// Adopts a new contextual snapshot and keeps the progress bar moving.
    fn advance_context(&mut self) {
        if let Some((generation, snapshot)) = self.context.changed_since(self.context_generation) {
            self.context_generation = generation;
            self.snapshot = snapshot;
            if self.snapshot.screen_off != self.screen_off {
                self.screen_off = self.snapshot.screen_off;
                if self.screen_off {
                    self.input.cancel_touches();
                }
            }
            self.take_audio();
            self.set_capabilities(desktop_capabilities(&self.snapshot));
            self.refresh_label();
            let contextual = Contextual {
                now_playing: self.label.is_some() && self.snapshot.now_playing.is_some(),
                seekable: self
                    .snapshot
                    .now_playing
                    .as_ref()
                    .is_some_and(NowPlaying::seekable),
                microphone: self.snapshot.microphone.is_some(),
            };
            // Both copies: the session's draws, the interpreter's hit tests.
            self.bar = self.bar.with_contextual(contextual);
            self.input.set_contextual(contextual);
            self.dirty = true;
        }
        let width = self.bar.now_playing_label_width();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pixel = self
            .snapshot
            .now_playing
            .as_ref()
            .and_then(|player| player.progress(Instant::now()))
            .map(|progress| (progress * f64::from(width)) as u32);
        if pixel != self.progress_pixel && !self.screen_off {
            self.progress_pixel = pixel;
            self.dirty = true;
        }
    }

    fn refresh_label(&mut self) {
        let Some(player) = self.snapshot.now_playing.as_ref() else {
            self.label = None;
            return;
        };
        let Some(text) = self.text.as_ref() else {
            return;
        };
        let wanted = player.label();
        if self.label.as_ref().is_some_and(|(shown, _)| *shown == wanted) {
            return;
        }
        let mask = text.render(
            &wanted,
            self.bar.now_playing_font_size(),
            self.bar.now_playing_label_width(),
            self.bar.now_playing_label_height(),
        );
        self.label = Some((wanted, mask));
    }

    /// Steps the display toward the slider target and mirrors the measured
    /// level on an open display slider.
    fn advance_display(
        &mut self,
        now: Duration,
        connection: &HardwareConnection,
    ) -> Result<(), RendererError> {
        let showing = self.input.slider().kind() == Some(SliderKind::Display);
        if !(self.stepper.busy() || showing)
            || now.saturating_sub(self.last_level_poll) < LEVEL_POLL_INTERVAL
        {
            return Ok(());
        }
        self.last_level_poll = now;
        let Some(actual) = self.display_level.as_ref().and_then(LevelSource::fraction) else {
            return Ok(());
        };
        if let Some(direction) = self.stepper.poll(now, actual) {
            let direction = match direction {
                StepDirection::Down => WireStepDirection::Down,
                StepDirection::Up => WireStepDirection::Up,
            };
            self.send(
                ClientMessage::StepDisplayBrightness(direction),
                PendingRequest::HardwareAction,
                connection,
            )?;
        }
        if showing {
            let shown = self.stepper.target().unwrap_or(actual);
            self.dirty |= self
                .input
                .slider_mut()
                .show_external(SliderKind::Display, shown);
        }
        Ok(())
    }

    /// Adopts the polled volume unless it may predate our own change.
    fn take_audio(&mut self) {
        let Some(audio) = self.snapshot.audio else {
            return;
        };
        let recent = self
            .volume_set_at
            .is_some_and(|at| self.now.saturating_sub(at) < VOLUME_ECHO_GRACE);
        if recent || (self.volume == Some(audio.volume) && self.muted == audio.muted) {
            return;
        }
        self.volume = Some(audio.volume);
        self.muted = audio.muted;
        self.input
            .slider_mut()
            .show_external(SliderKind::Volume, f64::from(audio.volume) / 100.0);
        self.dirty = true;
    }

    /// Relayouts for new desktop capabilities once no finger is down.
    fn set_capabilities(&mut self, desktop: StockCapabilities) {
        let capabilities = self.hardware_capabilities | desktop;
        if capabilities == self.bar.capabilities() {
            self.pending_capabilities = None;
            return;
        }
        self.pending_capabilities = Some(capabilities);
    }

    fn apply_pending_capabilities(&mut self) -> Result<(), RendererError> {
        if !self.input.is_idle() {
            return Ok(());
        }
        let Some(capabilities) = self.pending_capabilities.take() else {
            return Ok(());
        };
        self.bar = Bar::new(
            self.dimensions.width(),
            self.dimensions.height(),
            capabilities,
        )
        .map_err(|_| RendererError::Negotiation)?
        .with_contextual(self.bar.contextual());
        self.input.set_bar(self.bar);
        self.dirty = true;
        Ok(())
    }

    fn allocate_request(&mut self, kind: PendingRequest) -> Result<u32, RendererError> {
        if self.pending.len() >= MAX_PENDING_REQUESTS {
            return Err(RendererError::Protocol);
        }
        let request_id = self.request_ids.next(&self.pending)?;
        self.pending.insert(request_id, kind);
        Ok(request_id)
    }

    fn take_frame_id(&mut self) -> u64 {
        let current = self.next_frame_id;
        self.next_frame_id = self.next_frame_id.wrapping_add(1);
        if self.next_frame_id == 0 {
            self.next_frame_id = 1;
        }
        current
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_percent(fraction: f64) -> u8 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u8
}

const fn function_key(key: FunctionKey) -> KeyCode {
    match key {
        FunctionKey::F1 => KeyCode::F1,
        FunctionKey::F2 => KeyCode::F2,
        FunctionKey::F3 => KeyCode::F3,
        FunctionKey::F4 => KeyCode::F4,
        FunctionKey::F5 => KeyCode::F5,
        FunctionKey::F6 => KeyCode::F6,
        FunctionKey::F7 => KeyCode::F7,
        FunctionKey::F8 => KeyCode::F8,
        FunctionKey::F9 => KeyCode::F9,
        FunctionKey::F10 => KeyCode::F10,
        FunctionKey::F11 => KeyCode::F11,
        FunctionKey::F12 => KeyCode::F12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_reaches_its_target_once() {
        let mut fade = Fade::default();
        fade.retarget(255, Duration::ZERO);
        assert!(fade.advance(Duration::from_millis(75), OVERLAY_FADE_DURATION));
        assert!(fade.value > 0 && fade.value < 255);
        assert!(fade.advance(Duration::from_millis(500), OVERLAY_FADE_DURATION));
        assert_eq!(fade.value, 255);
        assert!(!fade.advance(Duration::from_millis(600), OVERLAY_FADE_DURATION));
    }

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(to_percent(-1.0), 0);
        assert_eq!(to_percent(0.494), 49);
        assert_eq!(to_percent(0.495), 50);
        assert_eq!(to_percent(2.0), 100);
    }

    #[test]
    fn function_keys_map_to_their_wire_codes() {
        assert_eq!(function_key(FunctionKey::F1), KeyCode::F1);
        assert_eq!(function_key(FunctionKey::F12), KeyCode::F12);
    }
}
