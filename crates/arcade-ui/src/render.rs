use arcade_libretro::{FrameBuffer, GlTextureFrame, PixelFormat};
use eframe::egui;
use eframe::glow::{self, HasContext};
use egui::{ColorImage, Vec2};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::info;

use crate::app::NativeArcadeUiApp;
use crate::play_session::{frame_has_sampled_luma, is_dolphin_core};

static UI_FRAME_UPLOAD_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static PLAY_GL_PAINTER: OnceLock<Mutex<Option<PlayGlPainter>>> = OnceLock::new();

struct PlayGlPainter {
    program: glow::NativeProgram,
    vertex_array: glow::NativeVertexArray,
    vertex_buffer: glow::NativeBuffer,
    pos_attr: u32,
    texture_uniform: Option<glow::NativeUniformLocation>,
    bottom_left_origin_uniform: Option<glow::NativeUniformLocation>,
}

impl PlayGlPainter {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let program = gl.create_program()?;
        let vertex_shader = compile_shader(
            gl,
            glow::VERTEX_SHADER,
            r#"#version 150
in vec2 a_pos;
out vec2 v_uv;
uniform int u_bottom_left_origin;
void main() {
    gl_Position = vec4(a_pos, 0.0, 1.0);
    float y = (a_pos.y + 1.0) * 0.5;
    if (u_bottom_left_origin == 0) {
        y = 1.0 - y;
    }
    v_uv = vec2((a_pos.x + 1.0) * 0.5, y);
}
"#,
        )?;
        let fragment_shader = compile_shader(
            gl,
            glow::FRAGMENT_SHADER,
            r#"#version 150
uniform sampler2D u_texture;
in vec2 v_uv;
out vec4 out_color;
void main() {
    out_color = texture(u_texture, v_uv);
    out_color.a = 1.0;
}
"#,
        )?;

        gl.attach_shader(program, vertex_shader);
        gl.attach_shader(program, fragment_shader);
        gl.link_program(program);
        let linked = gl.get_program_link_status(program);
        let link_log = gl.get_program_info_log(program);
        gl.detach_shader(program, vertex_shader);
        gl.detach_shader(program, fragment_shader);
        gl.delete_shader(vertex_shader);
        gl.delete_shader(fragment_shader);
        if !linked {
            gl.delete_program(program);
            return Err(format!("failed to link Play GL painter shader: {link_log}"));
        }

        let vertex_array = gl.create_vertex_array()?;
        let vertex_buffer = gl.create_buffer()?;
        let pos_attr = gl.get_attrib_location(program, "a_pos").unwrap_or(0);
        let texture_uniform = gl.get_uniform_location(program, "u_texture");
        let bottom_left_origin_uniform = gl.get_uniform_location(program, "u_bottom_left_origin");

        Ok(Self {
            program,
            vertex_array,
            vertex_buffer,
            pos_attr,
            texture_uniform,
            bottom_left_origin_uniform,
        })
    }

    unsafe fn paint(&self, gl: &glow::Context, frame: GlTextureFrame) {
        const VERTICES: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

        gl.use_program(Some(self.program));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(frame.texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        if let Some(uniform) = self.texture_uniform.as_ref() {
            gl.uniform_1_i32(Some(uniform), 0);
        }
        if let Some(uniform) = self.bottom_left_origin_uniform.as_ref() {
            gl.uniform_1_i32(Some(uniform), if frame.bottom_left_origin { 1 } else { 0 });
        }

        gl.bind_vertex_array(Some(self.vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vertex_buffer));
        let vertex_bytes = std::slice::from_raw_parts(
            VERTICES.as_ptr().cast::<u8>(),
            VERTICES.len() * std::mem::size_of::<f32>(),
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::STREAM_DRAW);
        gl.enable_vertex_attrib_array(self.pos_attr);
        gl.vertex_attrib_pointer_f32(self.pos_attr, 2, glow::FLOAT, false, 8, 0);
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.disable_vertex_attrib_array(self.pos_attr);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_vertex_array(None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.use_program(None);
    }
}

unsafe fn compile_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::NativeShader, String> {
    let shader = gl.create_shader(shader_type)?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if gl.get_shader_compile_status(shader) {
        Ok(shader)
    } else {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        Err(format!("failed to compile Play GL painter shader: {log}"))
    }
}

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
    pub(crate) fn update_gl_texture_frame(&mut self, frame: GlTextureFrame) {
        self.assets.last_gl_texture_frame = Some(frame);
        self.assets.last_frame_texture = None;
        self.state.play.last_frame_size = Some((frame.width, frame.height));
    }

    pub(crate) fn draw_gl_texture_frame(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        frame: GlTextureFrame,
    ) {
        let callback = eframe::egui_glow::CallbackFn::new(move |_info, painter| {
            let gl = painter.gl();
            let state = PLAY_GL_PAINTER.get_or_init(|| Mutex::new(None));
            let mut guard = match state.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if guard.is_none() {
                match unsafe { PlayGlPainter::new(gl) } {
                    Ok(painter) => *guard = Some(painter),
                    Err(err) => {
                        tracing::warn!("failed to initialize Play GL painter: {err}");
                        return;
                    }
                }
            }
            if let Some(painter) = guard.as_ref() {
                unsafe {
                    painter.paint(gl, frame);
                }
            }
        });

        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(callback),
        });
    }

    pub(crate) fn update_frame_texture(&mut self, ctx: &egui::Context, mut frame: FrameBuffer) {
        self.assets.last_gl_texture_frame = None;
        if is_dolphin_core(self.state.play.active_core.as_deref())
            && self.assets.last_frame_texture.is_some()
            && !frame_has_sampled_luma(&frame)
        {
            return;
        }

        let size = [frame.width as usize, frame.height as usize];
        let required_len = size[0].saturating_mul(size[1]).saturating_mul(4);
        let direct_rgba = matches!(frame.pixel_format, PixelFormat::Rgba8888)
            && frame.pitch == size[0].saturating_mul(4)
            && frame.data.len() >= required_len;
        let force_opaque_alpha = matches!(frame.pixel_format, PixelFormat::Rgba8888)
            && self
                .state
                .play
                .active_core
                .as_deref()
                .is_some_and(|core| core.eq_ignore_ascii_case("play"));

        if direct_rgba && force_opaque_alpha {
            // Play's GL path can occasionally deliver non-opaque alpha in otherwise valid
            // scene frames; force opaque alpha before uploading to avoid translucent
            // multi-frame ghosting in UI compositing.
            for pixel in frame.data[..required_len].chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }

        if !direct_rgba {
            frame_to_rgba_into(
                &frame.data,
                frame.width,
                frame.height,
                frame.pitch,
                frame.pixel_format,
                &mut self.assets.play_frame_rgba,
            );
            if force_opaque_alpha {
                for pixel in self.assets.play_frame_rgba.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
            }
        }

        let reuse_texture = self.state.play.last_frame_size == Some((frame.width, frame.height))
            && self.assets.last_frame_texture.is_some();

        // Debug logging — scoped so the borrow of frame.data ends before we move it below.
        if std::env::var_os("ARCADE_VULKAN_DEBUG").is_some() {
            let upload_rgba: &[u8] = if direct_rgba {
                &frame.data[..required_len]
            } else {
                &self.assets.play_frame_rgba
            };
            let frame_index = UI_FRAME_UPLOAD_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
            if frame_index < 16 {
                let (checksum, non_black_pixels, first_rgba) =
                    summarize_rgba_debug_pixels(upload_rgba);
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

        // For direct RGBA frames we reinterpret the Vec<u8> as Vec<Color32> without copying.
        // Color32 is [u8;4] with identical RGBA layout, so this avoids allocation + conversion.
        let image = if direct_rgba {
            let rgba_bytes = frame.data;
            let pixel_count = size[0] * size[1];
            // SAFETY: Color32 is 4 bytes, align 1, same as [u8;4].  len == pixel_count * 4.
            // capacity % 4 == 0 because we allocate with_capacity(width*height*4).
            let pixels: Vec<egui::Color32> = unsafe {
                debug_assert_eq!(rgba_bytes.len(), pixel_count * 4);
                debug_assert_eq!(rgba_bytes.capacity() % 4, 0);
                let len = rgba_bytes.len() / 4;
                let cap = rgba_bytes.capacity() / 4;
                let ptr = rgba_bytes.as_ptr() as *mut egui::Color32;
                std::mem::forget(rgba_bytes);
                Vec::from_raw_parts(ptr, len, cap)
            };
            ColorImage { size, pixels }
        } else {
            ColorImage::from_rgba_unmultiplied(size, &self.assets.play_frame_rgba)
        };

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
