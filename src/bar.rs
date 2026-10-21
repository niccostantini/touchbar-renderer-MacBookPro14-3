//! Layout, hit testing and drawing of the three surfaces: the normal control
//! strip, the Fn row and the expanded slider. Geometry is authored in the
//! panel's native 2170x60 pixels and scaled to the negotiated dimensions.

use t1_touchbar::framebuffer::{FramebufferError, Rectangle, Rgb, Xrgb8888Frame};
use t1_touchbar::stock_slice::{FunctionKey, StockCapabilities};

use crate::art::shapes::{Icon, draw_close, draw_icon, draw_knob, draw_track};
use crate::art::stock_art::{self, Symbol};
use crate::art::stock_labels::{self, AlphaMask};
use crate::text::TextMask;

const NATIVE_WIDTH: u32 = 2_170;
const NATIVE_HEIGHT: u32 = 60;
const FUNCTION_SLOT_COUNT: u32 = 13;
const PLATE_RADIUS: u32 = 8;

const BACKGROUND: Rgb = rgb(0);
const PLATE: Rgb = rgb(72);
const PRESSED_PLATE: Rgb = rgb(104);
// Leo Camp accents (theme/build.py): violet toggles, magenta levels.
const ACTIVE_PLATE: Rgb = Rgb {
    red: 106,
    green: 43,
    blue: 232,
};
const LABEL: Rgb = rgb(255);
const TRACK_EMPTY: Rgb = rgb(92);
const KNOB: Rgb = rgb(255);
// Camp palette for icons, by group.
const MAGENTA: Rgb = Rgb { red: 255, green: 79, blue: 176 };
const ORANGE: Rgb = Rgb { red: 255, green: 138, blue: 61 };
const YELLOW: Rgb = Rgb { red: 255, green: 210, blue: 63 };
const VIOLET: Rgb = Rgb { red: 167, green: 123, blue: 255 };
const CYAN: Rgb = Rgb { red: 88, green: 214, blue: 255 };
const fn hex(value: u32) -> Rgb {
    #[allow(clippy::cast_possible_truncation)]
    Rgb {
        red: (value >> 16) as u8,
        green: (value >> 8) as u8,
        blue: value as u8,
    }
}

/// Three-stop slider gradients, each ending on its button's icon colour.
const DISPLAY_GRADIENT: [Rgb; 3] = [hex(0xff2d00), hex(0xff9a00), hex(0xffe14d)];
const KEYBOARD_GRADIENT: [Rgb; 3] = [hex(0xd4145a), hex(0xff5a1f), hex(0xffb347)];
const VOLUME_GRADIENT: [Rgb; 3] = [hex(0x2b12ff), hex(0x8b3dff), hex(0xff4fd8)];
/// Hold-to-lock bar: green to orange.
const LOCK_GRADIENT: [Rgb; 3] = [hex(0x2ee86f), hex(0xc6d93a), hex(0xff7b2e)];
/// The active output pill: the Leo Camp magenta to orange.
const OUTPUT_GRADIENT: [Rgb; 3] = [hex(0xf2369b), hex(0xff5a5a), hex(0xff7b2e)];
const PAUSED_LABEL: Rgb = rgb(150);
const PROGRESS_FILLED: Rgb = Rgb {
    red: 255,
    green: 123,
    blue: 46,
};
const PROGRESS_EMPTY: Rgb = rgb(60);
const MICROPHONE_MUTED_PLATE: Rgb = Rgb {
    red: 176,
    green: 42,
    blue: 42,
};
const BLUETOOTH_ON_PLATE: Rgb = Rgb {
    red: 37,
    green: 99,
    blue: 235,
};
/// A Bluetooth device being connected, disconnected or paired.
const BLUETOOTH_BUSY_PLATE: Rgb = Rgb {
    red: 30,
    green: 58,
    blue: 138,
};
/// Discovered, not yet paired devices: a dimmer pill and label.
const DISCOVERED_PLATE: Rgb = rgb(40);
const DISCOVERED_LABEL: Rgb = rgb(170);
const MICROPHONE_MUTED_PRESSED: Rgb = Rgb {
    red: 204,
    green: 64,
    blue: 64,
};

/// Native geometry of one horizontal span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Span {
    x: u32,
    width: u32,
}

// Custom buttons, left-aligned in the space after Escape. The microphone
// button appears only while an application records.
const SCREENSHOT_BUTTON: Span = Span { x: 194, width: 110 };
const LOCK_BUTTON: Span = Span { x: 312, width: 110 };
const DO_NOT_DISTURB_BUTTON: Span = Span { x: 430, width: 110 };
const BLUETOOTH_BUTTON: Span = Span { x: 548, width: 110 };
const MICROPHONE_BUTTON: Span = Span { x: 666, width: 110 };
const CUSTOM_ICON: u32 = 30;

// Now playing: title line and a thin progress bar, shown while a player has
// a track, centred in the free space between the custom buttons (the
// microphone one only while recording) and the brightness group.
const NOW_PLAYING_MARGIN: u32 = 64;
/// Previous/next zones at the ends of the now-playing block.
const SKIP_WIDTH: u32 = 92;
/// The small play glyph before a paused title.
const PAUSED_GLYPH: u32 = 30;
const NOW_PLAYING_TEXT_HEIGHT: u32 = 42;
const NOW_PLAYING_FONT: u32 = 23;
const PROGRESS_Y: u32 = 46;
const PROGRESS_HEIGHT: u32 = 4;
/// Thickness of the hold-to-lock bar.
const LOCK_BAR_HEIGHT: u32 = 16;
/// Blank space between the end of a scrolling title and its next start.
const MARQUEE_GAP: u32 = 90;

// Normal strip, right-aligned like the macOS Control Strip: brightness group,
// media group, audio group.
const DISPLAY_BUTTON: Span = Span { x: 1_474, width: 130 };
const KEYBOARD_BUTTON: Span = Span { x: 1_610, width: 130 };
const OUTPUTS_BUTTON: Span = Span { x: 1_768, width: 130 };
const MUTE_BUTTON: Span = Span { x: 1_904, width: 130 };
const VOLUME_BUTTON: Span = Span { x: 2_040, width: 130 };

// Expanded slider.
const CLOSE_BUTTON: Span = Span { x: 190, width: 130 };
const LOW_ICON: Span = Span { x: 340, width: 110 };
const TRACK: Span = Span { x: 470, width: 1_490 };
const HIGH_ICON: Span = Span { x: 1_980, width: 110 };
const TRACK_TOUCH_LEFT: u32 = 330;
const TRACK_HEIGHT: u32 = 12;
const KNOB_DIAMETER: u32 = 34;
const KNOB_RING: u32 = 6;
const CLOSE_GLYPH: u32 = 22;
const CLOSE_STROKE: f64 = 3.5;

// Audio output and Bluetooth device pickers: pills after the close button.
const OUTPUTS_LEFT: u32 = 340;
const OUTPUTS_RIGHT: u32 = 2_160;
const OUTPUT_GAP: u32 = 16;
const OUTPUT_MAX_WIDTH: u32 = 440;
const OUTPUT_LABEL_INSET: u32 = 20;
/// The picker shows at most this many outputs.
pub const MAX_OUTPUTS: usize = 4;
/// The Bluetooth picker shows at most this many devices.
pub const MAX_DEVICES: usize = 5;

const fn rgb(level: u8) -> Rgb {
    Rgb {
        red: level,
        green: level,
        blue: level,
    }
}

/// A control whose value is adjusted with the expanded slider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliderKind {
    Display,
    Keyboard,
    Volume,
}

/// A discrete action fired by a tap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Escape,
    Function(FunctionKey),
    ToggleMute,
    MediaPrevious,
    MediaPlayPause,
    MediaNext,
    Screenshot,
    LockScreen,
    ToggleDoNotDisturb,
    ToggleMicrophone,
    ToggleBluetooth,
    /// Play/pause the player shown in the now-playing area.
    NowPlayingToggle,
    /// Seek the shown track, in thousandths of its length.
    Seek(u16),
    /// Make the n-th listed audio output the default.
    SelectOutput(u8),
    /// Connect, disconnect or pair the n-th listed Bluetooth device.
    SelectDevice(u8),
}

/// What lies under a contact on the current surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Action(Action),
    /// Opens the slider for this control (normal surface).
    SliderButton(SliderKind),
    /// Closes the expanded slider.
    Close,
    /// The adjustable area of the expanded slider.
    Track,
    /// The now-playing title: tap to pause, drag to seek.
    NowPlaying,
    /// Opens the audio output picker (normal surface).
    OutputsButton,
}

/// Which surface is shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Surface {
    Normal,
    Function,
    Slider(SliderKind),
    /// The audio output picker.
    Outputs,
    /// The Bluetooth device picker (hold the Bluetooth key).
    Devices,
}

/// How a Bluetooth device pill is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceState {
    Connected,
    Paired,
    Discovered,
    /// A command for it is running.
    Busy,
    /// Not a device: the "searching" note shown while the list is empty.
    Placeholder,
}

/// One pill of the Bluetooth picker.
#[derive(Clone, Debug, PartialEq)]
pub struct DevicePill {
    pub label: TextMask,
    pub state: DeviceState,
}

/// The now-playing area's content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NowPlayingView<'a> {
    /// Pre-rendered "Title · Artist", at most the area's width.
    pub label: &'a TextMask,
    pub playing: bool,
    /// Track progress 0.0..=1.0, when the player reports it.
    pub progress: Option<f64>,
    /// Marquee offset in display pixels, for labels wider than the area.
    pub scroll: u32,
}

/// Everything besides geometry that affects one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View<'a> {
    pub surface: Surface,
    pub pressed: Option<Target>,
    pub muted: bool,
    pub do_not_disturb: bool,
    /// `Some(muted)` while an application records from a microphone.
    pub microphone: Option<bool>,
    /// `Some(on)` when the Mac has Bluetooth.
    pub bluetooth: Option<bool>,
    pub now_playing: Option<NowPlayingView<'a>>,
    /// Pre-rendered output names for the picker, and the active one.
    pub outputs: &'a [TextMask],
    pub active_output: Option<usize>,
    /// Bluetooth picker pills, paired devices first.
    pub devices: &'a [DevicePill],
    /// Hold-to-lock fill, shown in place of the now-playing block.
    pub lock_progress: Option<f64>,
    /// Slider position, 0.0..=1.0, used only on the slider surface.
    pub fraction: f64,
}

/// Which contextual elements are present, for hit testing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Contextual {
    pub now_playing: bool,
    pub seekable: bool,
    pub microphone: bool,
    /// Number of audio outputs listed in the picker.
    pub outputs: u8,
    /// The Mac has a Bluetooth adapter.
    pub bluetooth: bool,
    /// Number of Bluetooth devices listed in the picker.
    pub devices: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarError {
    InvalidGeometry,
    FrameGeometryMismatch,
    Drawing,
}

impl From<FramebufferError> for BarError {
    fn from(_: FramebufferError) -> Self {
        Self::Drawing
    }
}

/// Pure geometry for one negotiated panel size and capability set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bar {
    width: u32,
    height: u32,
    capabilities: StockCapabilities,
    contextual: Contextual,
}

impl Bar {
    /// # Errors
    ///
    /// Rejects zero dimensions.
    pub const fn new(
        width: u32,
        height: u32,
        capabilities: StockCapabilities,
    ) -> Result<Self, BarError> {
        if width == 0 || height == 0 {
            Err(BarError::InvalidGeometry)
        } else {
            Ok(Self {
                width,
                height,
                capabilities,
                contextual: Contextual {
                    now_playing: false,
                    seekable: false,
                    microphone: false,
                    outputs: 0,
                    bluetooth: false,
                    devices: 0,
                },
            })
        }
    }

    #[must_use]
    pub const fn capabilities(self) -> StockCapabilities {
        self.capabilities
    }

    #[must_use]
    pub const fn contextual(self) -> Contextual {
        self.contextual
    }

    #[must_use]
    pub const fn with_contextual(mut self, contextual: Contextual) -> Self {
        self.contextual = contextual;
        self
    }

    /// Display pixels per native pixel, horizontally.
    #[must_use]
    pub fn horizontal_scale(self) -> f64 {
        f64::from(self.width) / f64::from(NATIVE_WIDTH)
    }

    /// The free space left of the brightness group, after the last visible
    /// custom button.
    fn now_playing_span(self) -> Span {
        let last = if self.contextual.microphone {
            MICROPHONE_BUTTON
        } else if self.contextual.bluetooth {
            BLUETOOTH_BUTTON
        } else {
            DO_NOT_DISTURB_BUTTON
        };
        let left = last.x + last.width + NOW_PLAYING_MARGIN;
        let right = DISPLAY_BUTTON.x - NOW_PLAYING_MARGIN;
        Span {
            x: left,
            width: right.saturating_sub(left).max(1),
        }
    }

    /// The title and progress, between the previous and next zones.
    fn now_playing_title_span(self) -> Span {
        let span = self.now_playing_span();
        Span {
            x: span.x + SKIP_WIDTH,
            width: span.width.saturating_sub(2 * SKIP_WIDTH).max(1),
        }
    }

    fn skip_span(self, next: bool) -> Span {
        let span = self.now_playing_span();
        Span {
            x: if next { span.x + span.width - SKIP_WIDTH } else { span.x },
            width: SKIP_WIDTH,
        }
    }

    /// Width available to the now-playing label, in display pixels.
    #[must_use]
    pub fn now_playing_label_width(self) -> u32 {
        self.scaled_x(self.now_playing_title_span().width).max(1)
    }

    #[must_use]
    pub fn now_playing_label_height(self) -> u32 {
        self.scaled_y(NOW_PLAYING_TEXT_HEIGHT).max(1)
    }

    /// Font size for the now-playing label, in display pixels.
    #[must_use]
    pub fn now_playing_font_size(self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let size = self.scaled_y(NOW_PLAYING_FONT).max(6) as f32;
        size
    }

    /// Whether a rendered label must scroll to be read in full.
    #[must_use]
    pub fn now_playing_scrolls(self, label_width: u32) -> bool {
        label_width > self.now_playing_label_width()
    }

    /// One marquee cycle: the label plus the gap, in display pixels.
    #[must_use]
    pub fn marquee_period(self, label_width: u32) -> u32 {
        label_width + self.scaled_x(MARQUEE_GAP).max(1)
    }

    /// Redraws only the now-playing area (marquee and progress steps), on a
    /// frame that already holds the rest of the normal surface.
    ///
    /// # Errors
    ///
    /// Rejects a changed frame geometry or an internally invalid draw.
    pub fn repaint_now_playing(
        self,
        frame: &mut Xrgb8888Frame<'_>,
        view: &View<'_>,
    ) -> Result<(), BarError> {
        if frame.width() != self.width || frame.height() != self.height {
            return Err(BarError::FrameGeometryMismatch);
        }
        if view.surface != Surface::Normal || !self.contextual.now_playing {
            return Ok(());
        }
        let Some(now_playing) = view.now_playing else {
            return Ok(());
        };
        frame.fill_rectangle(self.span_bounds(self.now_playing_span()), BACKGROUND)?;
        self.render_now_playing(frame, &now_playing, view.pressed)
    }

    /// Maps a display x coordinate to a position in the shown track.
    #[must_use]
    pub fn now_playing_fraction(self, x: f64) -> f64 {
        let left = f64::from(self.scaled_x(self.now_playing_title_span().x));
        let width = f64::from(self.now_playing_label_width());
        ((x - left) / width).clamp(0.0, 1.0)
    }

    /// Bounds of the n-th of `count` picker pills, in display pixels.
    fn output_pill(self, index: usize, count: usize) -> Rectangle {
        let count = u32::try_from(count.clamp(1, MAX_OUTPUTS.max(MAX_DEVICES))).unwrap_or(1);
        let index = u32::try_from(index).unwrap_or(0);
        let available = OUTPUTS_RIGHT - OUTPUTS_LEFT - (count - 1) * OUTPUT_GAP;
        let width = (available / count).min(OUTPUT_MAX_WIDTH);
        self.span_bounds(Span {
            x: OUTPUTS_LEFT + index * (width + OUTPUT_GAP),
            width,
        })
    }

    /// Room for an output or device name inside its pill, in display pixels.
    #[must_use]
    pub fn output_label_width(self, count: usize) -> u32 {
        self.output_pill(0, count)
            .width
            .saturating_sub(2 * self.scaled_x(OUTPUT_LABEL_INSET))
            .max(1)
    }

    /// Right edge of the Escape key, where the Touch ID overlay dimming starts.
    #[must_use]
    pub fn escape_right(self) -> u32 {
        let bounds = self.function_slot(0);
        bounds.x + bounds.width
    }

    #[must_use]
    pub fn slider_available(self, kind: SliderKind) -> bool {
        self.capabilities.contains(match kind {
            SliderKind::Display => StockCapabilities::DISPLAY_BRIGHTNESS,
            SliderKind::Keyboard => StockCapabilities::KEYBOARD_BACKLIGHT,
            SliderKind::Volume => StockCapabilities::AUDIO,
        })
    }

    /// Returns the target under a display-coordinate contact.
    #[must_use]
    pub fn target_at(self, x: f64, y: f64, surface: Surface) -> Option<Target> {
        if !(0.0..f64::from(self.width)).contains(&x) || !(0.0..f64::from(self.height)).contains(&y)
        {
            return None;
        }
        let x = display_coordinate(x);
        match surface {
            Surface::Function => self.function_target(x),
            Surface::Normal => {
                if self.within(self.function_slot(0), x) {
                    return Some(Target::Action(Action::Escape));
                }
                if self.contextual.now_playing && self.within_span(self.now_playing_span(), x) {
                    return Some(if self.within_span(self.skip_span(false), x) {
                        Target::Action(Action::MediaPrevious)
                    } else if self.within_span(self.skip_span(true), x) {
                        Target::Action(Action::MediaNext)
                    } else {
                        Target::NowPlaying
                    });
                }
                self.normal_items()
                    .into_iter()
                    .find(|(target, span)| self.target_enabled(*target) && self.within_span(*span, x))
                    .map(|(target, _)| target)
            }
            Surface::Outputs => {
                if self.within(self.function_slot(0), x) {
                    return Some(Target::Action(Action::Escape));
                }
                if self.within_span(CLOSE_BUTTON, x) {
                    return Some(Target::Close);
                }
                let count = usize::from(self.contextual.outputs).min(MAX_OUTPUTS);
                (0..count)
                    .find(|index| self.within(self.output_pill(*index, count), x))
                    .and_then(|index| u8::try_from(index).ok())
                    .map(|index| Target::Action(Action::SelectOutput(index)))
            }
            Surface::Devices => {
                if self.within(self.function_slot(0), x) {
                    return Some(Target::Action(Action::Escape));
                }
                if self.within_span(CLOSE_BUTTON, x) {
                    return Some(Target::Close);
                }
                let count = usize::from(self.contextual.devices).min(MAX_DEVICES);
                (0..count)
                    .find(|index| self.within(self.output_pill(*index, count), x))
                    .and_then(|index| u8::try_from(index).ok())
                    .map(|index| Target::Action(Action::SelectDevice(index)))
            }
            Surface::Slider(_) => {
                if self.within(self.function_slot(0), x) {
                    Some(Target::Action(Action::Escape))
                } else if self.within_span(CLOSE_BUTTON, x) {
                    Some(Target::Close)
                } else if x >= self.scaled_x(TRACK_TOUCH_LEFT) {
                    Some(Target::Track)
                } else {
                    None
                }
            }
        }
    }

    /// Maps a display x coordinate to an absolute slider position.
    #[must_use]
    pub fn track_fraction(self, x: f64) -> f64 {
        let left = f64::from(self.scaled_x(TRACK.x));
        let width = self.track_width();
        ((x - left) / width).clamp(0.0, 1.0)
    }

    /// Track length in display pixels, used for relative drags.
    #[must_use]
    pub fn track_width(self) -> f64 {
        f64::from(self.scaled_x(TRACK.width).max(1))
    }

    /// # Errors
    ///
    /// Rejects a changed frame geometry or an internally invalid draw.
    pub fn render(self, frame: &mut Xrgb8888Frame<'_>, view: &View<'_>) -> Result<(), BarError> {
        if frame.width() != self.width || frame.height() != self.height {
            return Err(BarError::FrameGeometryMismatch);
        }
        frame.clear(BACKGROUND);
        match view.surface {
            Surface::Function => self.render_function_row(frame, view.pressed),
            Surface::Normal => self.render_normal(frame, view),
            Surface::Slider(kind) => self.render_slider(frame, kind, view),
            Surface::Outputs => self.render_outputs(frame, view),
            Surface::Devices => self.render_devices(frame, view),
        }
    }

    fn render_picker_close(self, frame: &mut Xrgb8888Frame<'_>, view: &View<'_>) -> Result<(), BarError> {
        let escape = Target::Action(Action::Escape);
        self.draw_plate(frame, self.function_slot(0), plate_color(escape, view))?;
        stock_art::draw_label(frame, self.function_slot(0), stock_labels::ESC, LABEL)?;
        let close = self.span_bounds(CLOSE_BUTTON);
        self.draw_plate(frame, close, plate_color(Target::Close, view))?;
        draw_close(
            frame,
            close,
            self.scaled_y(CLOSE_GLYPH),
            f64::from(self.height) * CLOSE_STROKE / f64::from(NATIVE_HEIGHT),
            LABEL,
        )?;
        Ok(())
    }

    fn render_devices(self, frame: &mut Xrgb8888Frame<'_>, view: &View<'_>) -> Result<(), BarError> {
        self.render_picker_close(frame, view)?;
        let count = view.devices.len().min(MAX_DEVICES);
        for (index, device) in view.devices.iter().take(count).enumerate() {
            let pill = self.output_pill(index, count);
            let pressed = u8::try_from(index)
                .is_ok_and(|index| view.pressed == Some(Target::Action(Action::SelectDevice(index))));
            let (plate, label) = match device.state {
                DeviceState::Placeholder => (None, DISCOVERED_LABEL),
                _ if pressed => (Some(PRESSED_PLATE), LABEL),
                DeviceState::Connected => (Some(BLUETOOTH_ON_PLATE), LABEL),
                DeviceState::Busy => (Some(BLUETOOTH_BUSY_PLATE), LABEL),
                DeviceState::Paired => (Some(PLATE), LABEL),
                DeviceState::Discovered => (Some(DISCOVERED_PLATE), DISCOVERED_LABEL),
            };
            if let Some(plate) = plate {
                self.draw_plate(frame, pill, plate)?;
            }
            self.draw_pill_label(frame, pill, &device.label, label)?;
        }
        Ok(())
    }

    fn draw_pill_label(
        self,
        frame: &mut Xrgb8888Frame<'_>,
        pill: Rectangle,
        label: &TextMask,
        color: Rgb,
    ) -> Result<(), BarError> {
        if label.width > 0
            && label.width <= pill.width
            && label.height <= self.height
            && label.pixels.len() == (label.width * label.height) as usize
        {
            frame.blend_mask(
                Rectangle {
                    x: pill.x + (pill.width - label.width) / 2,
                    y: (self.height - label.height) / 2,
                    width: label.width,
                    height: label.height,
                },
                &label.pixels,
                color,
            )?;
        }
        Ok(())
    }

    fn render_outputs(self, frame: &mut Xrgb8888Frame<'_>, view: &View<'_>) -> Result<(), BarError> {
        self.render_picker_close(frame, view)?;
        let count = view.outputs.len().min(MAX_OUTPUTS);
        for (index, label) in view.outputs.iter().take(count).enumerate() {
            let pill = self.output_pill(index, count);
            let target = u8::try_from(index)
                .map(|index| Target::Action(Action::SelectOutput(index)))
                .ok();
            if view.active_output == Some(index) {
                draw_gradient_fill(frame, pill, 1.0, OUTPUT_GRADIENT)?;
            } else {
                let color = if view.pressed.is_some() && view.pressed == target {
                    PRESSED_PLATE
                } else {
                    PLATE
                };
                self.draw_plate(frame, pill, color)?;
            }
            self.draw_pill_label(frame, pill, label, LABEL)?;
        }
        Ok(())
    }

    fn render_normal(self, frame: &mut Xrgb8888Frame<'_>, view: &View<'_>) -> Result<(), BarError> {
        let escape = Target::Action(Action::Escape);
        self.draw_plate(frame, self.function_slot(0), plate_color(escape, view))?;
        stock_art::draw_label(frame, self.function_slot(0), stock_labels::ESC, LABEL)?;
        for (target, span) in self.normal_items() {
            if !self.target_visible(target) {
                continue;
            }
            let bounds = self.span_bounds(span);
            let toggled = match target {
                Target::Action(Action::ToggleMute) => view.muted,
                Target::Action(Action::ToggleDoNotDisturb) => view.do_not_disturb,
                Target::Action(Action::ToggleMicrophone) => view.microphone == Some(true),
                Target::Action(Action::ToggleBluetooth) => view.bluetooth == Some(true),
                _ => false,
            };
            let pressed = view.pressed == Some(target);
            let color = match (target, toggled, pressed) {
                (Target::Action(Action::ToggleMicrophone), true, false) => MICROPHONE_MUTED_PLATE,
                (Target::Action(Action::ToggleMicrophone), true, true) => MICROPHONE_MUTED_PRESSED,
                (Target::Action(Action::ToggleBluetooth), true, false) => BLUETOOTH_ON_PLATE,
                (_, true, false) => ACTIVE_PLATE,
                (_, _, true) => PRESSED_PLATE,
                (_, false, false) => PLATE,
            };
            self.draw_plate(frame, bounds, color)?;
            let icon = match target {
                Target::Action(Action::ToggleMicrophone) if view.microphone == Some(true) => {
                    Some(Icon::MicrophoneOff)
                }
                _ => custom_icon(target),
            };
            // Toggled plates are coloured, so their icon goes white.
            let tint = if toggled { LABEL } else { icon_color(target) };
            match icon {
                Some(icon) => draw_icon(frame, bounds, self.scaled_y(CUSTOM_ICON), icon, tint)?,
                None => stock_art::draw_symbol(frame, bounds, normal_symbol(target), tint)?,
            }
        }
        if let Some(progress) = view.lock_progress {
            self.render_lock_hold(frame, progress)?;
        } else if let Some(now_playing) = view.now_playing {
            self.render_now_playing(frame, &now_playing, view.pressed)?;
        }
        Ok(())
    }

    /// The hold-to-lock bar across the whole now-playing block.
    fn render_lock_hold(self, frame: &mut Xrgb8888Frame<'_>, progress: f64) -> Result<(), BarError> {
        let span = self.span_bounds(self.now_playing_span());
        let thickness = self.scaled_y(LOCK_BAR_HEIGHT).max(2);
        let track = Rectangle {
            x: span.x,
            y: (self.height - thickness) / 2,
            width: span.width,
            height: thickness,
        };
        draw_track(frame, track, 1.0, TRACK_EMPTY, TRACK_EMPTY)?;
        draw_gradient_fill(frame, track, progress, LOCK_GRADIENT)
    }

    fn render_now_playing(
        self,
        frame: &mut Xrgb8888Frame<'_>,
        now_playing: &NowPlayingView<'_>,
        pressed: Option<Target>,
    ) -> Result<(), BarError> {
        // Previous and next sit at the ends of the block, icon only.
        for (next, action, symbol) in [
            (false, Action::MediaPrevious, Symbol::Previous),
            (true, Action::MediaNext, Symbol::Next),
        ] {
            let zone = self.span_bounds(self.skip_span(next));
            if pressed == Some(Target::Action(action)) {
                self.draw_plate(frame, zone, PRESSED_PLATE)?;
            }
            stock_art::draw_symbol(frame, zone, symbol, MAGENTA)?;
        }
        let full = self.span_bounds(self.now_playing_title_span());
        if pressed == Some(Target::NowPlaying) {
            self.draw_plate(frame, full, PLATE)?;
        }
        let label = now_playing.label;
        let text_height = if now_playing.progress.is_some() {
            self.now_playing_label_height()
        } else {
            self.height
        };
        let color = if now_playing.playing {
            LABEL
        } else {
            PAUSED_LABEL
        };
        // Paused: a small play glyph right before the grey title. When the
        // title scrolls, the glyph keeps the block's left edge.
        let glyph = if now_playing.playing {
            0
        } else {
            self.scaled_x(PAUSED_GLYPH).min(full.width / 4)
        };
        let valid = label.width > 0
            && label.height <= self.height
            && label.pixels.len() == (label.width * label.height) as usize;
        let top = text_height.saturating_sub(label.height) / 2;
        let glyph_box = |x: u32| Rectangle {
            x,
            y: 0,
            width: glyph,
            height: text_height.min(self.height),
        };
        if valid && label.width + glyph <= full.width {
            let x = full.x + (full.width - label.width - glyph) / 2;
            if glyph > 0 {
                draw_play_glyph(frame, glyph_box(x), MAGENTA)?;
            }
            frame.blend_mask(
                Rectangle {
                    x: x + glyph,
                    y: top,
                    width: label.width,
                    height: label.height,
                },
                &label.pixels,
                color,
            )?;
        } else if valid {
            if glyph > 0 {
                draw_play_glyph(frame, glyph_box(full.x), MAGENTA)?;
            }
            let width = full.width - glyph;
            let window = marquee_window(label, width, now_playing.scroll, self.marquee_period(label.width));
            frame.blend_mask(
                Rectangle {
                    x: full.x + glyph,
                    y: top,
                    width,
                    height: label.height,
                },
                &window,
                color,
            )?;
        }
        let area = full;
        if let Some(progress) = now_playing.progress {
            let inset = self.scaled_x(16);
            let track = Rectangle {
                x: area.x + inset,
                y: self.scaled_y(PROGRESS_Y).min(self.height.saturating_sub(1)),
                width: area.width.saturating_sub(2 * inset).max(1),
                height: self
                    .scaled_y(PROGRESS_HEIGHT)
                    .max(1)
                    .min(self.height - self.scaled_y(PROGRESS_Y).min(self.height.saturating_sub(1))),
            };
            draw_track(frame, track, progress, PROGRESS_FILLED, PROGRESS_EMPTY)?;
        }
        Ok(())
    }

    fn render_slider(
        self,
        frame: &mut Xrgb8888Frame<'_>,
        kind: SliderKind,
        view: &View<'_>,
    ) -> Result<(), BarError> {
        let escape = Target::Action(Action::Escape);
        self.draw_plate(frame, self.function_slot(0), plate_color(escape, view))?;
        stock_art::draw_label(frame, self.function_slot(0), stock_labels::ESC, LABEL)?;

        let close = self.span_bounds(CLOSE_BUTTON);
        self.draw_plate(frame, close, plate_color(Target::Close, view))?;
        draw_close(
            frame,
            close,
            self.scaled_y(CLOSE_GLYPH),
            f64::from(self.height) * CLOSE_STROKE / f64::from(NATIVE_HEIGHT),
            LABEL,
        )?;

        let (low, high) = slider_symbols(kind, view.muted);
        let tint = icon_color(Target::SliderButton(kind));
        stock_art::draw_symbol(frame, self.span_bounds(LOW_ICON), low, tint)?;
        stock_art::draw_symbol(frame, self.span_bounds(HIGH_ICON), high, tint)?;

        let track_height = self.scaled_y(TRACK_HEIGHT).max(2);
        let track = Rectangle {
            x: self.scaled_x(TRACK.x),
            y: (self.height - track_height) / 2,
            width: self.scaled_x(TRACK.width).max(1),
            height: track_height,
        };
        let fraction = view.fraction.clamp(0.0, 1.0);
        draw_track(frame, track, 1.0, TRACK_EMPTY, TRACK_EMPTY)?;
        draw_gradient_fill(frame, track, fraction, slider_gradient(kind))?;
        let knob_x = track.x + fraction_of(track.width, fraction);
        let center_y = self.height / 2;
        let diameter = self.scaled_y(KNOB_DIAMETER).max(4);
        draw_knob(
            frame,
            knob_x,
            center_y,
            diameter + self.scaled_y(KNOB_RING),
            BACKGROUND,
        )?;
        draw_knob(frame, knob_x, center_y, diameter, KNOB)?;
        Ok(())
    }

    fn render_function_row(
        self,
        frame: &mut Xrgb8888Frame<'_>,
        pressed: Option<Target>,
    ) -> Result<(), BarError> {
        for slot in 0..FUNCTION_SLOT_COUNT {
            let bounds = self.function_slot(slot);
            if bounds.width == 0 {
                continue;
            }
            let target = function_target(slot).ok_or(BarError::InvalidGeometry)?;
            let color = if pressed == Some(target) {
                PRESSED_PLATE
            } else {
                PLATE
            };
            self.draw_plate(frame, bounds, color)?;
            stock_art::draw_label(frame, bounds, function_label(slot), LABEL)?;
        }
        Ok(())
    }

    fn draw_plate(
        self,
        frame: &mut Xrgb8888Frame<'_>,
        bounds: Rectangle,
        color: Rgb,
    ) -> Result<(), BarError> {
        stock_art::draw_rounded_plate(frame, bounds, self.scaled_y(PLATE_RADIUS).max(1), color)?;
        Ok(())
    }

    fn function_target(self, x: u32) -> Option<Target> {
        let slot = u32::try_from(
            u64::from(x) * u64::from(FUNCTION_SLOT_COUNT) / u64::from(self.width),
        )
        .ok()?;
        if self.within(self.function_slot(slot), x) {
            function_target(slot)
        } else {
            None
        }
    }

    /// Same partition as the built-in renderer so Esc and F-keys line up.
    fn function_slot(self, slot: u32) -> Rectangle {
        let left = ceiling_partition(slot, self.width, FUNCTION_SLOT_COUNT);
        let right = ceiling_partition(slot + 1, self.width, FUNCTION_SLOT_COUNT);
        let inset = self.scaled_x(5).min((right - left) / 2);
        let left_inset = if slot == 0 { 0 } else { inset };
        Rectangle {
            x: left + left_inset,
            y: 0,
            width: (right - left).saturating_sub(left_inset + inset),
            height: self.height,
        }
    }

    fn normal_items(self) -> [(Target, Span); 10] {
        [
            (Target::Action(Action::Screenshot), SCREENSHOT_BUTTON),
            (Target::Action(Action::LockScreen), LOCK_BUTTON),
            (Target::Action(Action::ToggleDoNotDisturb), DO_NOT_DISTURB_BUTTON),
            (Target::Action(Action::ToggleBluetooth), BLUETOOTH_BUTTON),
            (Target::Action(Action::ToggleMicrophone), MICROPHONE_BUTTON),
            (Target::SliderButton(SliderKind::Display), DISPLAY_BUTTON),
            (Target::SliderButton(SliderKind::Keyboard), KEYBOARD_BUTTON),
            (Target::OutputsButton, OUTPUTS_BUTTON),
            (Target::Action(Action::ToggleMute), MUTE_BUTTON),
            (Target::SliderButton(SliderKind::Volume), VOLUME_BUTTON),
        ]
    }

    fn target_enabled(self, target: Target) -> bool {
        match target {
            Target::SliderButton(kind) => self.slider_available(kind),
            Target::Action(Action::ToggleMute) => {
                self.capabilities.contains(StockCapabilities::AUDIO)
            }
            Target::Action(
                Action::MediaPrevious | Action::MediaPlayPause | Action::MediaNext,
            ) => self.capabilities.contains(StockCapabilities::MEDIA),
            Target::Action(Action::ToggleMicrophone) => self.contextual.microphone,
            Target::Action(Action::ToggleBluetooth) => self.contextual.bluetooth,
            Target::OutputsButton => {
                self.capabilities.contains(StockCapabilities::AUDIO) && self.contextual.outputs > 0
            }
            Target::Action(Action::SelectOutput(_)) => self.contextual.outputs > 0,
            Target::Action(Action::SelectDevice(_)) => self.contextual.devices > 0,
            Target::Action(Action::NowPlayingToggle | Action::Seek(_)) | Target::NowPlaying => {
                self.contextual.now_playing
            }
            Target::Action(
                Action::Escape
                | Action::Function(_)
                | Action::Screenshot
                | Action::LockScreen
                | Action::ToggleDoNotDisturb,
            )
            | Target::Close
            | Target::Track => true,
        }
    }

    /// Media keys stay drawn without a player so the strip does not jump.
    fn target_visible(self, target: Target) -> bool {
        matches!(
            target,
            Target::Action(Action::MediaPrevious | Action::MediaPlayPause | Action::MediaNext)
        ) || self.target_enabled(target)
    }

    fn span_bounds(self, span: Span) -> Rectangle {
        Rectangle {
            x: self.scaled_x(span.x),
            y: 0,
            width: self.scaled_x(span.width).max(1),
            height: self.height,
        }
    }

    fn within_span(self, span: Span, x: u32) -> bool {
        self.within(self.span_bounds(span), x)
    }

    #[allow(clippy::unused_self)]
    fn within(self, bounds: Rectangle, x: u32) -> bool {
        x >= bounds.x && x < bounds.x.saturating_add(bounds.width)
    }

    fn scaled_x(self, value: u32) -> u32 {
        scale(value, self.width, NATIVE_WIDTH)
    }

    fn scaled_y(self, value: u32) -> u32 {
        scale(value, self.height, NATIVE_HEIGHT)
    }
}

fn plate_color(target: Target, view: &View<'_>) -> Rgb {
    if view.pressed == Some(target) {
        PRESSED_PLATE
    } else {
        PLATE
    }
}

const fn normal_symbol(target: Target) -> Symbol {
    match target {
        Target::SliderButton(SliderKind::Display) => Symbol::DisplayUp,
        Target::SliderButton(SliderKind::Keyboard) => Symbol::KeyboardUp,
        Target::Action(Action::MediaPrevious) => Symbol::Previous,
        Target::Action(Action::MediaPlayPause) => Symbol::PlayPause,
        Target::Action(Action::MediaNext) => Symbol::Next,
        Target::Action(Action::ToggleMute) => Symbol::Mute,
        Target::SliderButton(SliderKind::Volume)
        | Target::Action(
            Action::Escape
            | Action::Function(_)
            | Action::Screenshot
            | Action::LockScreen
            | Action::ToggleDoNotDisturb
            | Action::ToggleMicrophone
            | Action::ToggleBluetooth
            | Action::NowPlayingToggle
            | Action::Seek(_)
            | Action::SelectOutput(_)
            | Action::SelectDevice(_),
        )
        | Target::Close
        | Target::Track
        | Target::NowPlaying
        | Target::OutputsButton => Symbol::VolumeUp,
    }
}

/// A small right-pointing triangle centred in `bounds`.
fn draw_play_glyph(frame: &mut Xrgb8888Frame<'_>, bounds: Rectangle, color: Rgb) -> Result<(), BarError> {
    let size = (bounds.height * 2 / 5).min(bounds.width).max(2);
    let area = Rectangle {
        x: bounds.x + (bounds.width - size) / 2,
        y: bounds.y + (bounds.height - size) / 2,
        width: size,
        height: size,
    };
    let extent = f64::from(size);
    let mask = crate::art::stock_art::rasterize(size, size, |x, y| {
        let (u, v) = (x / extent, y / extent);
        u >= 0.15 && v >= 0.1 + (u - 0.15) * 0.47 && v <= 0.9 - (u - 0.15) * 0.47
    });
    frame.blend_mask(area, &mask, color)?;
    Ok(())
}

/// The `width`-pixel slice of a label scrolled by `scroll`, with the label
/// repeating every `period` pixels (label, gap, label...).
fn marquee_window(label: &TextMask, width: u32, scroll: u32, period: u32) -> Vec<u8> {
    let period = period.max(label.width).max(1);
    let mut window = vec![0_u8; (width * label.height) as usize];
    for x in 0..width {
        let source = (x + scroll) % period;
        if source >= label.width {
            continue;
        }
        for y in 0..label.height {
            window[(y * width + x) as usize] = label.pixels[(y * label.width + source) as usize];
        }
    }
    window
}

/// Icon colour by group: system keys warm-to-cool, brightness yellow and
/// orange, media magenta, audio cyan and violet.
const fn icon_color(target: Target) -> Rgb {
    match target {
        Target::Action(Action::Screenshot) => ORANGE,
        Target::Action(Action::LockScreen) => YELLOW,
        Target::Action(Action::ToggleDoNotDisturb) => VIOLET,
        Target::Action(Action::ToggleMicrophone) => CYAN,
        Target::Action(Action::ToggleBluetooth) => rgb(150),
        Target::SliderButton(SliderKind::Display) => DISPLAY_GRADIENT[2],
        Target::SliderButton(SliderKind::Keyboard) => KEYBOARD_GRADIENT[2],
        Target::Action(Action::MediaPrevious | Action::MediaPlayPause | Action::MediaNext) => {
            MAGENTA
        }
        Target::Action(Action::ToggleMute) => CYAN,
        Target::OutputsButton => ORANGE,
        Target::SliderButton(SliderKind::Volume) => VOLUME_GRADIENT[1],
        _ => LABEL,
    }
}

const fn slider_gradient(kind: SliderKind) -> [Rgb; 3] {
    match kind {
        SliderKind::Display => DISPLAY_GRADIENT,
        SliderKind::Keyboard => KEYBOARD_GRADIENT,
        SliderKind::Volume => VOLUME_GRADIENT,
    }
}

/// Colour at `t` (0..=1) along a three-stop gradient.
fn gradient_at(stops: [Rgb; 3], t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0) * 2.0;
    let (a, b, local) = if t <= 1.0 {
        (stops[0], stops[1], t)
    } else {
        (stops[1], stops[2], t - 1.0)
    };
    let mix = |a: u8, b: u8| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let value = (f64::from(a) + (f64::from(b) - f64::from(a)) * local).round() as u8;
        value
    };
    Rgb {
        red: mix(a.red, b.red),
        green: mix(a.green, b.green),
        blue: mix(a.blue, b.blue),
    }
}

/// Fills the left `fraction` of a pill track with a gradient spanning the
/// whole track, in narrow strips of interpolated colour.
fn draw_gradient_fill(
    frame: &mut Xrgb8888Frame<'_>,
    track: Rectangle,
    fraction: f64,
    stops: [Rgb; 3],
) -> Result<(), BarError> {
    const STRIPS: u32 = 48;
    let filled = fraction_of(track.width, fraction);
    if filled == 0 {
        return Ok(());
    }
    let strip = track.width.div_ceil(STRIPS).max(1);
    let mut x = 0;
    while x < filled {
        let width = strip.min(filled - x);
        let middle = f64::from(x) + f64::from(width) / 2.0;
        let color = gradient_at(stops, middle / f64::from(track.width.max(1)));
        // Same pill mask as the track, clipped to this strip.
        let piece = Rectangle {
            x: track.x + x,
            y: track.y,
            width,
            height: track.height,
        };
        let radius = f64::from(track.height) / 2.0;
        let full = f64::from(track.width);
        let mask = crate::art::stock_art::rasterize(width, track.height, |px, py| {
            let gx = f64::from(x) + px;
            let cx = gx.max(radius).min(full - radius);
            (gx - cx).hypot(py - radius) <= radius
        });
        frame.blend_mask(piece, &mask, color)?;
        x += width;
    }
    Ok(())
}

const fn custom_icon(target: Target) -> Option<Icon> {
    match target {
        Target::Action(Action::Screenshot) => Some(Icon::Crop),
        Target::Action(Action::LockScreen) => Some(Icon::Padlock),
        Target::Action(Action::ToggleDoNotDisturb) => Some(Icon::Moon),
        Target::Action(Action::ToggleMicrophone) => Some(Icon::Microphone),
        Target::Action(Action::ToggleBluetooth) => Some(Icon::Bluetooth),
        Target::OutputsButton => Some(Icon::Speaker),
        _ => None,
    }
}

const fn slider_symbols(kind: SliderKind, muted: bool) -> (Symbol, Symbol) {
    match kind {
        SliderKind::Display => (Symbol::DisplayDown, Symbol::DisplayUp),
        SliderKind::Keyboard => (Symbol::KeyboardDown, Symbol::KeyboardUp),
        SliderKind::Volume if muted => (Symbol::Mute, Symbol::VolumeUp),
        SliderKind::Volume => (Symbol::VolumeDown, Symbol::VolumeUp),
    }
}

const fn function_target(slot: u32) -> Option<Target> {
    Some(Target::Action(match slot {
        0 => Action::Escape,
        1 => Action::Function(FunctionKey::F1),
        2 => Action::Function(FunctionKey::F2),
        3 => Action::Function(FunctionKey::F3),
        4 => Action::Function(FunctionKey::F4),
        5 => Action::Function(FunctionKey::F5),
        6 => Action::Function(FunctionKey::F6),
        7 => Action::Function(FunctionKey::F7),
        8 => Action::Function(FunctionKey::F8),
        9 => Action::Function(FunctionKey::F9),
        10 => Action::Function(FunctionKey::F10),
        11 => Action::Function(FunctionKey::F11),
        12 => Action::Function(FunctionKey::F12),
        _ => return None,
    }))
}

const fn function_label(slot: u32) -> AlphaMask {
    match slot {
        1 => stock_labels::F1,
        2 => stock_labels::F2,
        3 => stock_labels::F3,
        4 => stock_labels::F4,
        5 => stock_labels::F5,
        6 => stock_labels::F6,
        7 => stock_labels::F7,
        8 => stock_labels::F8,
        9 => stock_labels::F9,
        10 => stock_labels::F10,
        11 => stock_labels::F11,
        12 => stock_labels::F12,
        _ => stock_labels::ESC,
    }
}

fn scale(value: u32, extent: u32, native_extent: u32) -> u32 {
    u32::try_from(
        (u64::from(value) * u64::from(extent) + u64::from(native_extent) / 2)
            / u64::from(native_extent),
    )
    .unwrap_or(extent)
}

fn ceiling_partition(index: u32, extent: u32, count: u32) -> u32 {
    let numerator = u64::from(index) * u64::from(extent);
    u32::try_from(numerator.div_ceil(u64::from(count))).unwrap_or(extent)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn display_coordinate(value: f64) -> u32 {
    value as u32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fraction_of(extent: u32, fraction: f64) -> u32 {
    (f64::from(extent) * fraction).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Bar {
        Bar::new(
            NATIVE_WIDTH,
            NATIVE_HEIGHT,
            StockCapabilities::DISPLAY_BRIGHTNESS
                | StockCapabilities::KEYBOARD_BACKLIGHT
                | StockCapabilities::AUDIO
                | StockCapabilities::MEDIA,
        )
        .unwrap()
    }

    #[test]
    fn normal_strip_hit_tests_every_control() {
        let bar = full();
        let at = |x: u32| bar.target_at(f64::from(x), 30.0, Surface::Normal);
        assert_eq!(at(10), Some(Target::Action(Action::Escape)));
        assert_eq!(at(250), Some(Target::Action(Action::Screenshot)));
        assert_eq!(at(360), Some(Target::Action(Action::LockScreen)));
        assert_eq!(at(480), Some(Target::Action(Action::ToggleDoNotDisturb)));
        // Contextual elements are inert until present.
        assert_eq!(at(600), None);
        assert_eq!(at(800), None);
        assert_eq!(at(1_100), None, "now playing is absent");
        assert_eq!(at(1_500), Some(Target::SliderButton(SliderKind::Display)));
        assert_eq!(at(1_650), Some(Target::SliderButton(SliderKind::Keyboard)));
        // The outputs button needs at least one listed output.
        assert_eq!(at(1_830), None);
        assert_eq!(at(1_960), Some(Target::Action(Action::ToggleMute)));
        assert_eq!(at(2_169), Some(Target::SliderButton(SliderKind::Volume)));
        // Gaps between plates are inert.
        assert_eq!(at(1_606), None);

        let bar = bar.with_contextual(Contextual {
            now_playing: true,
            seekable: true,
            microphone: true,
            outputs: 2,
            bluetooth: true,
            devices: 0,
        });
        let at = |x: u32| bar.target_at(f64::from(x), 30.0, Surface::Normal);
        assert_eq!(at(1_830), Some(Target::OutputsButton));
        let pick = |x: u32| bar.target_at(f64::from(x), 30.0, Surface::Outputs);
        assert_eq!(pick(250), Some(Target::Close));
        assert_eq!(pick(400), Some(Target::Action(Action::SelectOutput(0))));
        assert_eq!(pick(900), Some(Target::Action(Action::SelectOutput(1))));
        assert_eq!(pick(2_000), None);
        assert_eq!(at(600), Some(Target::Action(Action::ToggleBluetooth)));
        assert_eq!(at(700), Some(Target::Action(Action::ToggleMicrophone)));
        // Block 840..1410: previous and next at the ends, title between.
        assert_eq!(at(880), Some(Target::Action(Action::MediaPrevious)));
        assert_eq!(at(1_100), Some(Target::NowPlaying));
        assert_eq!(at(1_380), Some(Target::Action(Action::MediaNext)));
        assert_eq!(at(1_440), None, "padding before the brightness group");
        assert!((bar.now_playing_fraction(1_125.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn unavailable_controls_are_not_hit() {
        let bar = Bar::new(NATIVE_WIDTH, NATIVE_HEIGHT, StockCapabilities::default()).unwrap();
        assert_eq!(bar.target_at(1_240.0, 30.0, Surface::Normal), None);
        assert_eq!(bar.target_at(2_100.0, 30.0, Surface::Normal), None);
        assert_eq!(bar.target_at(1_600.0, 30.0, Surface::Normal), None);
    }

    #[test]
    fn slider_surface_maps_track_and_close() {
        let bar = full();
        let surface = Surface::Slider(SliderKind::Volume);
        assert_eq!(bar.target_at(250.0, 30.0, surface), Some(Target::Close));
        assert_eq!(bar.target_at(400.0, 30.0, surface), Some(Target::Track));
        assert_eq!(bar.target_at(2_150.0, 30.0, surface), Some(Target::Track));
        assert!(bar.track_fraction(0.0).abs() < f64::EPSILON);
        assert!((bar.track_fraction(2_169.0) - 1.0).abs() < f64::EPSILON);
        assert!((bar.track_fraction(470.0 + 745.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn marquee_window_wraps_with_a_gap() {
        let label = TextMask {
            width: 3,
            height: 1,
            pixels: vec![1, 2, 3],
        };
        // Period 5: label (3 px) then a 2 px gap.
        assert_eq!(marquee_window(&label, 4, 0, 5), vec![1, 2, 3, 0]);
        assert_eq!(marquee_window(&label, 4, 2, 5), vec![3, 0, 0, 1]);
        assert_eq!(marquee_window(&label, 4, 5, 5), vec![1, 2, 3, 0]);
    }

    #[test]
    fn function_row_matches_stock_partition() {
        let bar = full();
        let at = |x: u32| bar.target_at(f64::from(x), 30.0, Surface::Function);
        assert_eq!(at(20), Some(Target::Action(Action::Escape)));
        assert_eq!(at(250), Some(Target::Action(Action::Function(FunctionKey::F1))));
        assert_eq!(
            at(2_100),
            Some(Target::Action(Action::Function(FunctionKey::F12)))
        );
    }

    #[test]
    fn every_surface_renders_at_native_and_scaled_sizes() {
        for (width, height) in [(NATIVE_WIDTH, NATIVE_HEIGHT), (1_085, 30), (130, 20)] {
            let bar = Bar::new(width, height, full().capabilities()).unwrap();
            let label = TextMask {
                width: 3,
                height: 2,
                pixels: vec![255; 6],
            };
            let stride = width * 4;
            let mut pixels = vec![0_u8; (stride * height) as usize];
            for surface in [
                Surface::Normal,
                Surface::Function,
                Surface::Slider(SliderKind::Display),
                Surface::Slider(SliderKind::Volume),
                Surface::Outputs,
            ] {
                for fraction in [0.0, 0.5, 1.0] {
                    let outputs = [label.clone(), label.clone(), label.clone()];
                    let mut frame =
                        Xrgb8888Frame::new(width, height, stride, &mut pixels).unwrap();
                    let view = View {
                        surface,
                        pressed: Some(Target::Close),
                        muted: true,
                        do_not_disturb: true,
                        microphone: Some(true),
                        bluetooth: Some(true),
                        now_playing: Some(NowPlayingView {
                            label: &label,
                            playing: false,
                            progress: Some(fraction),
                            scroll: 0,
                        }),
                        outputs: &outputs,
                        devices: &[],
                        active_output: Some(1),
                        lock_progress: (surface == Surface::Normal).then_some(fraction),
                        fraction,
                    };
                    bar.render(&mut frame, &view)
                        .unwrap_or_else(|error| panic!("{width}x{height} {surface:?}: {error:?}"));
                }
            }
        }
    }
}
