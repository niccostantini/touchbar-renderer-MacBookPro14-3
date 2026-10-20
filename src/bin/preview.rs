//! Renders every surface to PNG files so the layout can be checked without
//! the hardware: `preview <output-directory>`.

use std::env;
use std::fs;
use std::path::Path;

use t1_touchbar::framebuffer::Xrgb8888Frame;
use t1_touchbar::stock_slice::StockCapabilities;
use touchbar_renderer::bar::{Action, Bar, Contextual, NowPlayingView, SliderKind, Surface, Target, View};
use touchbar_renderer::text::TextRenderer;

const WIDTH: u32 = 2_170;
const HEIGHT: u32 = 60;

/// One preview: file name, surface, pressed target, toggles, slider value
/// and which contextual elements are present.
struct Scene {
    name: &'static str,
    surface: Surface,
    pressed: Option<Target>,
    muted: bool,
    do_not_disturb: bool,
    microphone: Option<bool>,
    playing: Option<bool>,
    long_title: bool,
    scroll: u32,
    fraction: f64,
}

const fn scene(name: &'static str, surface: Surface) -> Scene {
    Scene {
        name,
        surface,
        pressed: None,
        muted: false,
        do_not_disturb: false,
        microphone: None,
        playing: Some(true),
        long_title: false,
        scroll: 0,
        fraction: 0.0,
    }
}

fn main() {
    let output = env::args_os().nth(1).expect("usage: preview <output-directory>");
    let output = Path::new(&output);
    fs::create_dir_all(output).expect("output directory");
    let base = Bar::new(
        WIDTH,
        HEIGHT,
        StockCapabilities::DISPLAY_BRIGHTNESS
            | StockCapabilities::KEYBOARD_BACKLIGHT
            | StockCapabilities::AUDIO
            | StockCapabilities::MEDIA,
    )
    .expect("bar");
    let text = TextRenderer::load().expect("system font");
    let outputs = ["Audio interno", "Kensington Dock", "HDMI / DisplayPort"];
    let probe = base.with_contextual(Contextual {
        now_playing: true,
        seekable: true,
        microphone: true,
        outputs: 3,
    });
    let label = text.render(
        "Vogue · Madonna",
        probe.now_playing_font_size(),
        probe.now_playing_label_width(),
        probe.now_playing_label_height(),
    );
    let long_label = text.render(
        "Express Yourself (Non-Stop Dance Remix) · Madonna",
        probe.now_playing_font_size(),
        6_000,
        probe.now_playing_label_height(),
    );
    let output_labels: Vec<_> = outputs
        .iter()
        .map(|name| {
            text.render(
                name,
                probe.now_playing_font_size(),
                probe.output_label_width(outputs.len()),
                probe.now_playing_label_height(),
            )
        })
        .collect();
    let scenes = [
        Scene { playing: None, ..scene("1-normale", Surface::Normal) },
        Scene { microphone: Some(false), ..scene("2-musica-e-microfono", Surface::Normal) },
        Scene {
            long_title: true,
            scroll: 150,
            ..scene("2b-titolo-lungo-che-scorre", Surface::Normal)
        },
        Scene {
            muted: true,
            do_not_disturb: true,
            microphone: Some(true),
            playing: Some(false),
            pressed: Some(Target::Action(Action::MediaPlayPause)),
            ..scene("3-muto-non-disturbare-pausa", Surface::Normal)
        },
        Scene { playing: None, ..scene("4-tasti-fn", Surface::Function) },
        Scene { fraction: 0.35, ..scene("5-slider-luminosita", Surface::Slider(SliderKind::Display)) },
        Scene { fraction: 0.8, ..scene("6-slider-tastiera", Surface::Slider(SliderKind::Keyboard)) },
        Scene { fraction: 0.62, ..scene("7-slider-volume", Surface::Slider(SliderKind::Volume)) },
        Scene { playing: None, ..scene("8-uscite-audio", Surface::Outputs) },
    ];
    for scene in scenes {
        let bar = base.with_contextual(Contextual {
            now_playing: scene.playing.is_some(),
            seekable: true,
            microphone: scene.microphone.is_some(),
            outputs: 3,
        });
        let stride = WIDTH * 4;
        let mut pixels = vec![0_u8; (stride * HEIGHT) as usize];
        let mut frame = Xrgb8888Frame::new(WIDTH, HEIGHT, stride, &mut pixels).expect("frame");
        bar.render(
            &mut frame,
            &View {
                surface: scene.surface,
                pressed: scene.pressed,
                muted: scene.muted,
                do_not_disturb: scene.do_not_disturb,
                microphone: scene.microphone,
                now_playing: scene.playing.map(|playing| NowPlayingView {
                    label: if scene.long_title { &long_label } else { &label },
                    playing,
                    progress: Some(0.35),
                    scroll: scene.scroll,
                }),
                outputs: &output_labels,
                active_output: Some(0),
                fraction: scene.fraction,
            },
        )
        .expect("render");
        let path = output.join(format!("{}.png", scene.name));
        fs::write(&path, png(WIDTH, HEIGHT, &pixels)).expect("write png");
        println!("{}", path.display());
    }
}

/// Minimal PNG: 8-bit RGB, one zlib stream of stored (uncompressed) blocks.
fn png(width: u32, height: u32, xrgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((width * height * 3 + height) as usize);
    for row in xrgb.chunks_exact((width * 4) as usize) {
        raw.push(0);
        for pixel in row.chunks_exact(4) {
            raw.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        let length = u16::try_from(block.len()).expect("block length");
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).expect("chunk").to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}
