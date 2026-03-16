use arcade_libretro::{FrameBuffer, PixelFormat};
use eframe::egui;
use egui::{ColorImage, Vec2};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

use crate::app::NativeArcadeUiApp;

static UI_FRAME_UPLOAD_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);

fn summarize_rgba_debug_pixels(pixels: &[u8]) -> (u64, usize, [u8; 4]) {
    let mut checksum = 0_u64;
    let mut non_black_pixels = 0_usize;
    let mut first_rgba = [0_u8; 4];

    for (index, pixel) in pixels.chunks_exact(4).enumerate().take(64) {
        if index == 0 {
            first_rgba.copy_from_slice(pixel);
        }
        if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
            non_black_pixels += 1;
        }
        checksum = checksum
            .wrapping_mul(16_777_619)
            .wrapping_add(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]) as u64);
    }

    (checksum, non_black_pixels, first_rgba)
}

impl NativeArcadeUiApp {
    pub(crate) fn update_frame_texture(&mut self, ctx: &egui::Context, frame: FrameBuffer) {
        let size = [frame.width as usize, frame.height as usize];
        let required_len = size[0].saturating_mul(size[1]).saturating_mul(4);
        let direct_rgba = matches!(frame.pixel_format, PixelFormat::Rgba8888)
            && frame.pitch == size[0].saturating_mul(4)
            && frame.data.len() >= required_len;

        if !direct_rgba {
            frame_to_rgba_into(
                &frame.data,
                frame.width,
                frame.height,
                frame.pitch,
                frame.pixel_format,
                &mut self.assets.play_frame_rgba,
            );
        }

        let upload_rgba: &[u8] = if direct_rgba {
            &frame.data[..required_len]
        } else {
            &self.assets.play_frame_rgba
        };
        let reuse_texture = self.state.play.last_frame_size == Some((frame.width, frame.height))
            && self.assets.last_frame_texture.is_some();
        if std::env::var_os("ARCADE_VULKAN_DEBUG").is_some() {
            let frame_index = UI_FRAME_UPLOAD_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
            if frame_index < 16 {
                let (checksum, non_black_pixels, first_rgba) = summarize_rgba_debug_pixels(upload_rgba);
                info!(
                    target: "arcade_ui::video_debug",
                    "upload frame={} action={} size={}x{} src_pitch={} pixel_format={:?} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x}",
                    frame_index,
                    if reuse_texture { "update" } else { "create" },
                    frame.width,
                    frame.height,
                    frame.pitch,
                    frame.pixel_format,
                    non_black_pixels,
                    first_rgba[0],
                    first_rgba[1],
                    first_rgba[2],
                    first_rgba[3],
                );
            }
        }
        let image = ColorImage::from_rgba_unmultiplied(size, upload_rgba);

        match &mut self.assets.last_frame_texture {
            Some(texture)
                if self.state.play.last_frame_size == Some((frame.width, frame.height)) =>
            {
                texture.set(image, egui::TextureOptions::NEAREST);
            }
            _ => {
                self.assets.last_frame_texture =
                    Some(ctx.load_texture("emulator-frame", image, egui::TextureOptions::NEAREST));
                self.state.play.last_frame_size = Some((frame.width, frame.height));
            }
        }
    }
}

pub(crate) fn fit_size(original: Vec2, max: Vec2) -> Vec2 {
    if original.x <= 0.0 || original.y <= 0.0 {
        return max;
    }
    let scale_x = max.x / original.x;
    let scale_y = max.y / original.y;
    let scale = scale_x.min(scale_y).min(1.0);
    egui::vec2(original.x * scale, original.y * scale)
}

pub(crate) fn fit_size_to_aspect(max: Vec2, aspect_ratio: f32) -> Vec2 {
    if max.x <= 0.0 || max.y <= 0.0 || !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
        return max;
    }

    let max_aspect = max.x / max.y;
    if max_aspect > aspect_ratio {
        egui::vec2(max.y * aspect_ratio, max.y)
    } else {
        egui::vec2(max.x, max.x / aspect_ratio)
    }
}

fn frame_to_rgba_into(
    input: &[u8],
    width: u32,
    height: u32,
    pitch: usize,
    pixel_format: PixelFormat,
    out: &mut Vec<u8>,
) {
    let width = width as usize;
    let height = height as usize;
    let required_len = width.saturating_mul(height).saturating_mul(4);
    if out.len() != required_len {
        out.resize(required_len, 0);
    }

    if matches!(pixel_format, PixelFormat::Rgba8888) {
        let row_len = width.saturating_mul(4);
        if pitch == row_len {
            let bytes = required_len.min(input.len());
            out[..bytes].copy_from_slice(&input[..bytes]);
            if bytes < required_len {
                out[bytes..].fill(0);
            }
            return;
        }

        for y in 0..height {
            let src_row_start = y.saturating_mul(pitch);
            let dst_row_start = y.saturating_mul(row_len);
            if src_row_start >= input.len() || dst_row_start >= out.len() {
                break;
            }
            let src_row_end = (src_row_start + row_len).min(input.len());
            let dst_row_end = (dst_row_start + row_len).min(out.len());
            let copy_len = (src_row_end - src_row_start).min(dst_row_end - dst_row_start);
            out[dst_row_start..dst_row_start + copy_len]
                .copy_from_slice(&input[src_row_start..src_row_start + copy_len]);
            if copy_len < row_len && dst_row_start + copy_len < out.len() {
                let fill_end = (dst_row_start + row_len).min(out.len());
                out[dst_row_start + copy_len..fill_end].fill(0);
            }
        }
        return;
    }

    for y in 0..height {
        let src_row_start = y.saturating_mul(pitch);
        let dst_row_start = y.saturating_mul(width * 4);
        if src_row_start >= input.len() || dst_row_start >= out.len() {
            break;
        }

        for x in 0..width {
            let bytes_per_pixel = match pixel_format {
                PixelFormat::Xrgb8888 | PixelFormat::Rgba8888 => 4,
                PixelFormat::Rgb565 | PixelFormat::Argb1555 => 2,
            };
            let src = src_row_start + x * bytes_per_pixel;
            let dst = dst_row_start + x * 4;
            if src + bytes_per_pixel > input.len() || dst + 3 >= out.len() {
                break;
            }

            let (r, g, b) = match pixel_format {
                PixelFormat::Xrgb8888 => (input[src + 2], input[src + 1], input[src]),
                PixelFormat::Rgba8888 => {
                    out[dst] = input[src];
                    out[dst + 1] = input[src + 1];
                    out[dst + 2] = input[src + 2];
                    out[dst + 3] = input[src + 3];
                    continue;
                }
                PixelFormat::Rgb565 => {
                    let value = u16::from_le_bytes([input[src], input[src + 1]]);
                    let r = ((value >> 11) & 0x1f) as u8;
                    let g = ((value >> 5) & 0x3f) as u8;
                    let b = (value & 0x1f) as u8;
                    (
                        (r << 3) | (r >> 2),
                        (g << 2) | (g >> 4),
                        (b << 3) | (b >> 2),
                    )
                }
                PixelFormat::Argb1555 => {
                    let value = u16::from_le_bytes([input[src], input[src + 1]]);
                    let r = ((value >> 10) & 0x1f) as u8;
                    let g = ((value >> 5) & 0x1f) as u8;
                    let b = (value & 0x1f) as u8;
                    (
                        (r << 3) | (r >> 2),
                        (g << 3) | (g >> 2),
                        (b << 3) | (b >> 2),
                    )
                }
            };

            out[dst] = r;
            out[dst + 1] = g;
            out[dst + 2] = b;
            out[dst + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::frame_to_rgba_into;
    use arcade_libretro::PixelFormat;

    #[test]
    fn converts_xrgb8888_little_endian_to_rgba() {
        let input = [0x33, 0x22, 0x11, 0x00];
        let mut rgba = Vec::new();
        frame_to_rgba_into(&input, 1, 1, 4, PixelFormat::Xrgb8888, &mut rgba);
        assert_eq!(rgba, vec![0x11, 0x22, 0x33, 0xff]);
    }

    #[test]
    fn keeps_rgba8888_rows_without_conversion_when_tightly_packed() {
        let input = [0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14];
        let mut rgba = Vec::new();
        frame_to_rgba_into(&input, 2, 1, 8, PixelFormat::Rgba8888, &mut rgba);
        assert_eq!(rgba, input);
    }

    #[test]
    fn keeps_rgba8888_rows_without_conversion_when_padded() {
        let input = [
            0x01, 0x02, 0x03, 0x04, 0xaa, 0xbb, 0xcc, 0xdd, // row 0 + padding
            0x11, 0x12, 0x13, 0x14, 0xee, 0xff, 0x00, 0x99, // row 1 + padding
        ];
        let mut rgba = Vec::new();
        frame_to_rgba_into(&input, 1, 2, 8, PixelFormat::Rgba8888, &mut rgba);
        assert_eq!(rgba, vec![0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14]);
    }

    #[test]
    fn converts_rgb565_to_rgba() {
        let red_565 = 0xf800_u16.to_le_bytes();
        let mut rgba = Vec::new();
        frame_to_rgba_into(&red_565, 1, 1, 2, PixelFormat::Rgb565, &mut rgba);
        assert_eq!(rgba, vec![255, 0, 0, 255]);
    }

    #[test]
    fn converts_0rgb1555_to_rgba() {
        let green_1555 = 0b0_00000_11111_00000u16.to_le_bytes();
        let mut rgba = Vec::new();
        frame_to_rgba_into(&green_1555, 1, 1, 2, PixelFormat::Argb1555, &mut rgba);
        assert_eq!(rgba, vec![0, 255, 0, 255]);
    }

    #[test]
    fn respects_pitch_padding_for_16_bit_frames() {
        let red_565 = 0xf800_u16.to_le_bytes();
        let green_565 = 0x07e0_u16.to_le_bytes();
        let input = [
            red_565[0],
            red_565[1],
            green_565[0],
            green_565[1],
            0xaa,
            0xbb,
        ];
        let mut rgba = Vec::new();
        frame_to_rgba_into(&input, 2, 1, 6, PixelFormat::Rgb565, &mut rgba);
        assert_eq!(rgba, vec![255, 0, 0, 255, 0, 255, 0, 255]);
    }
}
