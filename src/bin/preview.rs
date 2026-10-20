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

fn main() {
    let output = env::args_os().nth(1).expect("usage: preview <output-directory>");
    let output = Path::new(&output);
    fs::create_dir_all(output).expect("output directory");
    let bar = Bar::new(
        WIDTH,
        HEIGHT,
        StockCapabilities::DISPLAY_BRIGHTNESS
            | StockCapabilities::KEYBOARD_BACKLIGHT
            | StockCapabilities::AUDIO
            | StockCapabilities::MEDIA,
    )
    .expect("bar")
    .with_contextual(Contextual {
        now_playing: true,
        seekable: true,
        microphone: true,
    });
    let text = TextRenderer::load().expect("system font");
    let label = text.render(
        "A Fairly Long Synthetic Song Title · Example Artist",
        bar.now_playing_font_size(),
        bar.now_playing_label_width(),
        bar.now_playing_label_height(),
    );
    let views = [
        ("normal", Surface::Normal, None, false, 0.0),
        (
            "normal-muted-pressed",
            Surface::Normal,
            Some(Target::Action(Action::MediaPlayPause)),
            true,
            0.0,
        ),
        ("function", Surface::Function, None, false, 0.0),
        ("slider-volume", Surface::Slider(SliderKind::Volume), None, false, 0.62),
        (
            "slider-display",
            Surface::Slider(SliderKind::Display),
            Some(Target::Close),
            false,
            0.25,
        ),
        ("slider-keyboard", Surface::Slider(SliderKind::Keyboard), None, false, 1.0),
    ];
    for (name, surface, pressed, muted, fraction) in views {
        let stride = WIDTH * 4;
        let mut pixels = vec![0_u8; (stride * HEIGHT) as usize];
        let mut frame = Xrgb8888Frame::new(WIDTH, HEIGHT, stride, &mut pixels).expect("frame");
        bar.render(
            &mut frame,
            &View {
                surface,
                pressed,
                muted,
                do_not_disturb: muted,
                // The "muted" previews also show a recording, muted microphone
                // and a paused track; the others a playing track.
                microphone: Some(muted),
                now_playing: Some(NowPlayingView {
                    label: &label,
                    playing: !muted,
                    progress: Some(0.35),
                }),
                fraction,
            },
        )
        .expect("render");
        let path = output.join(format!("{name}.png"));
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
