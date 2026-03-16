mod content;
mod core_variables;
mod video;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CStr, CString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use anyhow::{anyhow, Context, Result};
use arcade_domain::{resolve_core, EmulationConfig};
use ash::vk;
#[cfg(feature = "audio")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glow::HasContext;
use libloading::{Library, Symbol};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tempfile::TempDir;
use thiserror::Error;
use tracing::{info, warn};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DestroyWindow, DispatchMessageA, GetClientRect,
    GetSystemMetrics, MoveWindow, PeekMessageA, RegisterClassExA, SetWindowPos, SetWindowTextA,
    ShowWindow, TranslateMessage, HWND_TOPMOST, MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN,
    SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, SW_SHOWMAXIMIZED, WM_CLOSE, WNDCLASSEXA, WS_CHILD,
    WS_EX_TOPMOST, WS_POPUP,
};
#[cfg(target_os = "linux")]
use x11_dl::xlib;

use self::content::{
    build_game_info, inspect_core_requirements, prepare_game_content, prepare_launch_session,
    strategy_uses_data, LaunchSession, LoadGameStrategy,
};
use self::core_variables::{
    apply_core_runtime_env_defaults, default_core_variables_for, store_default_variable,
};
use self::video::{FrameDelivery, VideoBackendKind, VideoCoordinator, VideoSessionInfo};

pub use self::video::FrontendCapabilities;

unsafe extern "C" {
    fn arcade_libretro_log_printf(level: i32, fmt: *const c_char, ...);
}

const VULKAN_PRESENT_VERT_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_present.vert.spv"));
const VULKAN_PRESENT_FRAG_SPV: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_present.frag.spv"));

const RETRO_ENVIRONMENT_EXPERIMENTAL: u32 = 0x10000;
const RETRO_ENVIRONMENT_GET_HW_RENDER_INTERFACE: u32 = 41 | RETRO_ENVIRONMENT_EXPERIMENTAL;
const RETRO_HW_CONTEXT_VULKAN: u32 = 6;
const RETRO_HW_RENDER_INTERFACE_VULKAN: u32 = 0;

#[derive(Debug, Clone)]
pub struct FrameBuffer {
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
    pub data: Vec<u8>,
    pub pixel_format: PixelFormat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PixelFormat {
    #[default]
    Argb1555,
    Xrgb8888,
    Rgb565,
    Rgba8888,
}

#[derive(Debug, Clone, Copy)]
struct PendingHardwareFrame {
    width: u32,
    height: u32,
    bottom_left_origin: bool,
}

#[derive(Debug, Default)]
struct CallbackState {
    latest_frame: Option<FrameBuffer>,
    latest_hw_frame: Option<PendingHardwareFrame>,
    input_state: HashMap<(u32, u32, u32, u32), i16>,
    pixel_format: PixelFormat,
}

#[derive(Debug, Default)]
struct AudioState {
    samples: VecDeque<i16>,
    source_sample_rate: f64,
    output_sample_rate: f64,
    resample_phase: f64,
    current_frame: Option<(i16, i16)>,
    next_frame: Option<(i16, i16)>,
}
const MAX_AUDIO_SAMPLES: usize = 49_152;
#[cfg(feature = "audio")]
const AUDIO_TARGET_LATENCY_SECS: f64 = 0.060;
#[cfg(feature = "audio")]
const AUDIO_MAX_LATENCY_SECS: f64 = 0.180;
#[cfg(feature = "audio")]
const AUDIO_RESAMPLE_QUEUE_CORRECTION: f64 = 0.005;
#[cfg(feature = "audio")]
const AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT: f64 = 0.005;
const VULKAN_FALLBACK_SYNC_FRAMES: u32 = 3;
const DISPLAY_FPS_TOLERANCE: f64 = 2.0;
const COMMON_DISPLAY_FPS: &[f64] = &[60.0, 50.0, 30.0];
const PERF_LOG_SAMPLE_FRAMES: u32 = 180;

#[derive(Debug, Default)]
struct PerformanceLogState {
    sample_started_at: Option<std::time::Instant>,
    sample_frames: u32,
    accumulated_run_ns: u64,
    accumulated_present_ns: u64,
    external_present_frames: u32,
    cpu_frame_count: u32,
    empty_frame_count: u32,
    error_count: u32,
    last_frame_size: Option<(u32, u32)>,
}

#[derive(Default)]
struct EnvironmentContext {
    system_dir: Option<StableCStringBuffer>,
    save_dir: Option<StableCStringBuffer>,
    variables: HashMap<String, CString>,
    variables_updated: bool,
    allow_vfs: bool,
    controller_info: Vec<Vec<u32>>,
    requested_hw_render: bool,
    requested_hw_context_type: Option<u32>,
    last_load_error: Option<String>,
    last_negotiation_interface: Option<(u32, u32)>,
}

#[derive(Clone, Copy)]
struct HardwareRenderCallbacks {
    context_reset: Option<RetroHwContextResetFn>,
    context_destroy: Option<RetroHwContextResetFn>,
    bottom_left_origin: bool,
}

struct HardwareRenderTarget {
    framebuffer: glow::Framebuffer,
    color_texture: glow::Texture,
    depth_stencil: glow::Renderbuffer,
    width: u32,
    height: u32,
}

#[derive(Clone)]
struct PendingVulkanImage {
    image: vk::Image,
    image_view: vk::ImageView,
    image_layout: vk::ImageLayout,
    format: vk::Format,
    view_type: vk::ImageViewType,
    components: vk::ComponentMapping,
    subresource_range: vk::ImageSubresourceRange,
    subresource_layers: vk::ImageSubresourceLayers,
    semaphores: Vec<vk::Semaphore>,
    src_queue_family: u32,
    signal_semaphore: Option<vk::Semaphore>,
}

impl PendingVulkanImage {
    fn color_subresource_range(
        create_info: &vk::ImageViewCreateInfo<'static>,
    ) -> vk::ImageSubresourceRange {
        let range = create_info.subresource_range;
        vk::ImageSubresourceRange::default()
            .aspect_mask(if range.aspect_mask.is_empty() {
                vk::ImageAspectFlags::COLOR
            } else {
                range.aspect_mask
            })
            .base_mip_level(range.base_mip_level)
            .level_count(range.level_count.max(1))
            .base_array_layer(range.base_array_layer)
            .layer_count(range.layer_count.max(1))
    }

    fn color_subresource_layers(
        create_info: &vk::ImageViewCreateInfo<'static>,
    ) -> vk::ImageSubresourceLayers {
        let range = Self::color_subresource_range(create_info);
        vk::ImageSubresourceLayers::default()
            .aspect_mask(range.aspect_mask)
            .mip_level(range.base_mip_level)
            .base_array_layer(range.base_array_layer)
            .layer_count(range.layer_count)
    }
}

fn pending_vulkan_image_delay(vulkan: &VulkanInterfaceState) -> usize {
    vulkan.sync_frames.saturating_sub(1).min(2) as usize
}

fn current_pending_vulkan_image(vulkan: &VulkanInterfaceState) -> Option<&PendingVulkanImage> {
    let delay = pending_vulkan_image_delay(vulkan);
    (vulkan.pending_images.len() > delay)
        .then(|| vulkan.pending_images.front())
        .flatten()
}

fn take_current_pending_vulkan_image(
    vulkan: &mut VulkanInterfaceState,
) -> Option<PendingVulkanImage> {
    let delay = pending_vulkan_image_delay(vulkan);
    if vulkan.pending_images.len() > delay {
        vulkan.pending_images.pop_front()
    } else {
        None
    }
}

fn should_sample_pending_vulkan_image_directly(image: &PendingVulkanImage) -> bool {
    #[cfg(target_os = "windows")]
    {
        image.semaphores.is_empty()
            && image.signal_semaphore.is_none()
            && image.src_queue_family == u32::MAX
            && matches!(
                image.image_layout,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL | vk::ImageLayout::GENERAL
            )
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = image;
        false
    }
}

fn create_sampling_image_view_for_pending_image(
    device: &ash::Device,
    image: &PendingVulkanImage,
) -> Result<vk::ImageView> {
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image.image)
        .view_type(image.view_type)
        .format(image.format)
        .components(image.components)
        .subresource_range(image.subresource_range);
    unsafe {
        device
            .create_image_view(&create_info, None)
            .map_err(|err| anyhow!("failed to create Vulkan sampling image view: {err:?}"))
    }
}

struct VulkanReadbackState {
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    staging_capacity: usize,
}

#[derive(Clone, Copy)]
enum ExternalVulkanWindowDescriptor {
    #[cfg(target_os = "linux")]
    Xlib {
        display: *mut xlib::Display,
        window: xlib::Window,
        width: u32,
        height: u32,
    },
    #[cfg(target_os = "windows")]
    Win32 {
        hwnd: vk::HWND,
        hinstance: vk::HINSTANCE,
        width: u32,
        height: u32,
    },
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    Unsupported { width: u32, height: u32 },
}

impl ExternalVulkanWindowDescriptor {
    fn size(self) -> (u32, u32) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Xlib { width, height, .. } => (width, height),
            #[cfg(target_os = "windows")]
            Self::Win32 { width, height, .. } => (width, height),
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            Self::Unsupported { width, height } => (width, height),
        }
    }
}

#[cfg(target_os = "linux")]
struct ExternalVulkanWindow {
    xlib: xlib::Xlib,
    display: *mut xlib::Display,
    window: xlib::Window,
    overlay_window: xlib::Window,
    overlay_gc: xlib::GC,
    wm_delete: xlib::Atom,
    net_wm_state: xlib::Atom,
    net_wm_state_fullscreen: xlib::Atom,
    net_wm_state_above: xlib::Atom,
    net_wm_bypass_compositor: xlib::Atom,
    width: u32,
    height: u32,
    visible: bool,
}

#[cfg(target_os = "windows")]
struct ExternalVulkanWindow {
    hinstance: HINSTANCE,
    hwnd: HWND,
    overlay_hwnd: HWND,
    width: u32,
    height: u32,
    visible: bool,
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
struct ExternalVulkanWindow;

#[cfg(target_os = "linux")]
unsafe impl Send for ExternalVulkanWindow {}

#[cfg(target_os = "windows")]
unsafe impl Send for ExternalVulkanWindow {}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
unsafe impl Send for ExternalVulkanWindow {}

struct VulkanPresentState {
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    extent: vk::Extent2D,
    format: vk::Format,
    images: Vec<vk::Image>,
    image_initialized: Vec<bool>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    present_queue: vk::Queue,
    acquire_semaphore: vk::Semaphore,
    present_queue_family_index: u32,
    acquired_image_index: Option<u32>,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    debug_readback: Option<VulkanPresentDebugReadback>,
    frames: Vec<VulkanPresentFrameResources>,
}

struct VulkanPresentDebugReadback {
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    staging_capacity: usize,
}

struct VulkanPresentFrameResources {
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    render_semaphore: vk::Semaphore,
    fence: vk::Fence,
    transient_image_view: Option<vk::ImageView>,
}

struct VulkanInterfaceState {
    _entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    _queue: vk::Queue,
    queue_family_index: u32,
    presentation_queue: vk::Queue,
    sync_index: u32,
    sync_frames: u32,
    waiting_for_core_wait_sync: bool,
    wait_sync_generation: u64,
    queue_locked: bool,
    destroy_device_callback: Option<RetroVulkanDestroyDeviceFn>,
    interface: RetroHwRenderInterfaceVulkan,
    pending_images: VecDeque<PendingVulkanImage>,
    present: Option<VulkanPresentState>,
    readback: Option<VulkanReadbackState>,
}

unsafe impl Send for VulkanInterfaceState {}

#[derive(Default)]
struct HardwareRenderState {
    external_vulkan_probe_logged: bool,
    frontend_gl_context: Option<Arc<glow::Context>>,
    context_type: Option<u32>,
    callbacks: Option<HardwareRenderCallbacks>,
    context_ready: bool,
    target: Option<HardwareRenderTarget>,
    external_vulkan_window: Option<ExternalVulkanWindow>,
    external_vulkan_present_active: bool,
    vulkan_fallback_frame_size: Option<(u32, u32)>,
    vulkan_negotiation: Option<VulkanNegotiationCallbacks>,
    vulkan: Option<VulkanInterfaceState>,
}

#[derive(Default)]
struct HostRuntime {
    callback_state: Mutex<CallbackState>,
    audio_state: Mutex<AudioState>,
    environment_context: Mutex<EnvironmentContext>,
    video_coordinator: Mutex<VideoCoordinator>,
    hw_render_state: Mutex<HardwareRenderState>,
    performance_log_state: Mutex<PerformanceLogState>,
}

#[cfg(all(unix, not(target_os = "macos")))]
type GlxGetProcAddress = unsafe extern "C" fn(*const u8) -> *const c_void;
#[cfg(all(unix, not(target_os = "macos")))]
type EglGetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;
#[cfg(target_os = "windows")]
type WglGetProcAddress = unsafe extern "system" fn(*const c_char) -> *const c_void;

struct GlProcLoader {
    #[cfg(all(unix, not(target_os = "macos")))]
    libgl: Option<Library>,
    #[cfg(all(unix, not(target_os = "macos")))]
    libegl: Option<Library>,
    #[cfg(all(unix, not(target_os = "macos")))]
    glx_get_proc_address: Option<GlxGetProcAddress>,
    #[cfg(all(unix, not(target_os = "macos")))]
    egl_get_proc_address: Option<EglGetProcAddress>,
    #[cfg(target_os = "macos")]
    opengl_framework: Option<Library>,
    #[cfg(target_os = "windows")]
    opengl32: Option<Library>,
    #[cfg(target_os = "windows")]
    wgl_get_proc_address: Option<WglGetProcAddress>,
}

static GL_PROC_LOADER: Lazy<Mutex<GlProcLoader>> = Lazy::new(|| Mutex::new(GlProcLoader::new()));
static ACTIVE_RUNTIME: Lazy<Mutex<Option<Weak<HostRuntime>>>> = Lazy::new(|| Mutex::new(None));
static VULKAN_DEBUG_STEP_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_READBACK_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_PRESENT_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_SOURCE_IMAGE_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_RUN_FRAME_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static GL_CONTEXT_DEBUG_LOGGED: AtomicU64 = AtomicU64::new(0);

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

fn summarize_rgba_debug_pixels_grid(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> (u64, usize, [u8; 4], [u8; 4]) {
    let mut checksum = 0_u64;
    let mut non_black_samples = 0_usize;
    let mut first_rgba = [0_u8; 4];
    let mut center_rgba = [0_u8; 4];

    if width == 0 || height == 0 {
        return (checksum, non_black_samples, first_rgba, center_rgba);
    }

    let sample_width = width.min(8);
    let sample_height = height.min(8);
    for sample_y in 0..sample_height {
        let y = ((sample_y as usize * height as usize) / sample_height as usize)
            .min(height.saturating_sub(1) as usize);
        for sample_x in 0..sample_width {
            let x = ((sample_x as usize * width as usize) / sample_width as usize)
                .min(width.saturating_sub(1) as usize);
            let offset = (y * width as usize + x) * 4;
            if offset + 4 > pixels.len() {
                continue;
            }
            let pixel = &pixels[offset..offset + 4];
            if sample_x == 0 && sample_y == 0 {
                first_rgba.copy_from_slice(pixel);
            }
            if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                non_black_samples += 1;
            }
            checksum = checksum
                .wrapping_mul(16_777_619)
                .wrapping_add(u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]) as u64);
        }
    }

    let center_x = (width / 2) as usize;
    let center_y = (height / 2) as usize;
    let center_offset = (center_y * width as usize + center_x) * 4;
    if center_offset + 4 <= pixels.len() {
        center_rgba.copy_from_slice(&pixels[center_offset..center_offset + 4]);
    }

    (checksum, non_black_samples, first_rgba, center_rgba)
}

fn log_frontend_gl_context(gl: &glow::Context) {
    if std::env::var_os("LIBRETRO_TRACE_GL_CONTEXT").is_none() {
        return;
    }
    if GL_CONTEXT_DEBUG_LOGGED.fetch_add(1, Ordering::Relaxed) != 0 {
        return;
    }

    unsafe {
        let version = gl.get_parameter_string(glow::VERSION);
        let shading_language_version = gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION);
        let vendor = gl.get_parameter_string(glow::VENDOR);
        let renderer = gl.get_parameter_string(glow::RENDERER);
        let major_version = gl.get_parameter_i32(glow::MAJOR_VERSION);
        let minor_version = gl.get_parameter_i32(glow::MINOR_VERSION);
        let context_profile_mask = gl.get_parameter_i32(glow::CONTEXT_PROFILE_MASK);
        let depth_bits = gl.get_parameter_i32(glow::DEPTH_BITS);
        let stencil_bits = gl.get_parameter_i32(glow::STENCIL_BITS);
        let samples = gl.get_parameter_i32(glow::SAMPLES);
        let draw_framebuffer_binding = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let read_framebuffer_binding = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let framebuffer_binding = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);

        eprintln!(
            "frontend GL context version={version:?} glsl={shading_language_version:?} vendor={vendor:?} renderer={renderer:?} major={major_version} minor={minor_version} profile_mask=0x{context_profile_mask:x} depth_bits={depth_bits} stencil_bits={stencil_bits} samples={samples} framebuffer={framebuffer_binding} draw_framebuffer={draw_framebuffer_binding} read_framebuffer={read_framebuffer_binding}"
        );
    }
}

fn trace_frontend_gl_framebuffer(gl: &glow::Context, stage: &str) {
    if std::env::var_os("LIBRETRO_TRACE_GL_FRAMEBUFFER").is_none() {
        return;
    }

    unsafe {
        let framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let draw_framebuffer = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let read_framebuffer = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let draw_buffer = gl.get_parameter_i32(glow::DRAW_BUFFER);
        let read_buffer = gl.get_parameter_i32(glow::READ_BUFFER);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        eprintln!(
            "frontend GL framebuffer stage={stage} framebuffer={framebuffer} draw_framebuffer={draw_framebuffer} read_framebuffer={read_framebuffer} draw_buffer=0x{draw_buffer:x} read_buffer=0x{read_buffer:x} status=0x{status:x}"
        );
    }
}

fn active_runtime() -> Option<Arc<HostRuntime>> {
    ACTIVE_RUNTIME.lock().as_ref()?.upgrade()
}

fn with_active_runtime<T>(f: impl FnOnce(&HostRuntime) -> T) -> Option<T> {
    let runtime = active_runtime()?;
    Some(f(&runtime))
}

fn register_active_runtime(runtime: &Arc<HostRuntime>) {
    *ACTIVE_RUNTIME.lock() = Some(Arc::downgrade(runtime));
}

fn clear_active_runtime(runtime: &Arc<HostRuntime>) {
    let mut active = ACTIVE_RUNTIME.lock();
    let Some(current) = active.as_ref().and_then(Weak::upgrade) else {
        return;
    };
    if Arc::ptr_eq(&current, runtime) {
        *active = None;
    }
}

#[derive(Debug, Error)]
pub enum LibretroError {
    #[error("core file not found: {0}")]
    CoreNotFound(String),
    #[error("failed to load game into core")]
    LoadGameFailed,
    #[error("unsupported hardware-render request: {0}")]
    UnsupportedHardwareRender(String),
}

type RetroEnvironmentFn = unsafe extern "C" fn(cmd: u32, data: *mut c_void) -> bool;
type RetroVideoRefreshFn =
    unsafe extern "C" fn(data: *const c_void, width: u32, height: u32, pitch: usize);
type RetroAudioSampleFn = unsafe extern "C" fn(left: i16, right: i16);
type RetroAudioSampleBatchFn = unsafe extern "C" fn(data: *const i16, frames: usize) -> usize;
type RetroInputPollFn = unsafe extern "C" fn();
type RetroInputStateFn = unsafe extern "C" fn(port: u32, device: u32, index: u32, id: u32) -> i16;

type RetroSetEnvironment = unsafe extern "C" fn(cb: RetroEnvironmentFn);
type RetroSetVideoRefresh = unsafe extern "C" fn(cb: RetroVideoRefreshFn);
type RetroSetAudioSample = unsafe extern "C" fn(cb: RetroAudioSampleFn);
type RetroSetAudioSampleBatch = unsafe extern "C" fn(cb: RetroAudioSampleBatchFn);
type RetroSetInputPoll = unsafe extern "C" fn(cb: RetroInputPollFn);
type RetroSetInputState = unsafe extern "C" fn(cb: RetroInputStateFn);
type RetroSetControllerPortDevice = unsafe extern "C" fn(port: u32, device: u32);
type RetroInit = unsafe extern "C" fn();
type RetroDeinit = unsafe extern "C" fn();
type RetroApiVersion = unsafe extern "C" fn() -> u32;
type RetroGetSystemInfo = unsafe extern "C" fn(info: *mut RetroSystemInfo);
type RetroGetSystemAvInfo = unsafe extern "C" fn(info: *mut RetroSystemAvInfo);
type RetroLoadGame = unsafe extern "C" fn(game: *const RetroGameInfo) -> bool;
type RetroUnloadGame = unsafe extern "C" fn();
type RetroReset = unsafe extern "C" fn();
type RetroRun = unsafe extern "C" fn();
type RetroSerializeSize = unsafe extern "C" fn() -> usize;
type RetroSerialize = unsafe extern "C" fn(data: *mut c_void, size: usize) -> bool;
type RetroUnserialize = unsafe extern "C" fn(data: *const c_void, size: usize) -> bool;
type RetroHwContextResetFn = unsafe extern "C" fn();
type RetroHwGetCurrentFramebufferFn = unsafe extern "C" fn() -> usize;
type RetroHwGetProcAddressFn = unsafe extern "C" fn(sym: *const c_char) -> *const c_void;

#[repr(C)]
struct RetroHwRenderCallback {
    context_type: u32,
    context_reset: Option<RetroHwContextResetFn>,
    get_current_framebuffer: Option<RetroHwGetCurrentFramebufferFn>,
    get_proc_address: Option<RetroHwGetProcAddressFn>,
    depth: bool,
    stencil: bool,
    bottom_left_origin: bool,
    version_major: u32,
    version_minor: u32,
    cache_context: bool,
    context_destroy: Option<RetroHwContextResetFn>,
    debug_context: bool,
}

#[repr(C)]
struct RetroHwRenderContextNegotiationInterface {
    interface_type: u32,
    interface_version: u32,
}

type RetroVulkanGetApplicationInfoFn = unsafe extern "C" fn() -> *const c_void;

#[repr(C)]
struct RetroVulkanContext {
    gpu: vk::PhysicalDevice,
    device: vk::Device,
    queue: vk::Queue,
    queue_family_index: u32,
    presentation_queue: vk::Queue,
    presentation_queue_family_index: u32,
}

type RetroVulkanCreateDeviceFn = unsafe extern "C" fn(
    context: *mut RetroVulkanContext,
    instance: vk::Instance,
    gpu: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    required_device_extensions: *const *const c_char,
    num_required_device_extensions: u32,
    required_device_layers: *const *const c_char,
    num_required_device_layers: u32,
    required_features: *const vk::PhysicalDeviceFeatures,
) -> bool;
type RetroVulkanDestroyDeviceFn = unsafe extern "C" fn();

#[repr(C)]
struct RetroHwRenderContextNegotiationInterfaceVulkan {
    interface_type: u32,
    interface_version: u32,
    get_application_info: Option<RetroVulkanGetApplicationInfoFn>,
    create_device: Option<RetroVulkanCreateDeviceFn>,
    destroy_device: Option<RetroVulkanDestroyDeviceFn>,
}

#[derive(Clone, Copy, Default)]
struct VulkanNegotiationCallbacks {
    get_application_info: Option<RetroVulkanGetApplicationInfoFn>,
    create_device: Option<RetroVulkanCreateDeviceFn>,
    destroy_device: Option<RetroVulkanDestroyDeviceFn>,
}

#[repr(C)]
struct RetroHwRenderInterface {
    interface_type: u32,
    interface_version: u32,
}

#[repr(C)]
struct RetroVulkanImage {
    image_view: vk::ImageView,
    image_layout: vk::ImageLayout,
    create_info: vk::ImageViewCreateInfo<'static>,
}

type RetroVulkanSetImageFn = unsafe extern "C" fn(
    handle: *mut c_void,
    image: *const RetroVulkanImage,
    num_semaphores: u32,
    semaphores: *const vk::Semaphore,
    src_queue_family: u32,
);
type RetroVulkanGetSyncIndexFn = unsafe extern "C" fn(handle: *mut c_void) -> u32;
type RetroVulkanSetCommandBuffersFn =
    unsafe extern "C" fn(handle: *mut c_void, num_cmd: u32, cmd: *const vk::CommandBuffer);
type RetroVulkanWaitSyncIndexFn = unsafe extern "C" fn(handle: *mut c_void);
type RetroVulkanLockQueueFn = unsafe extern "C" fn(handle: *mut c_void);
type RetroVulkanUnlockQueueFn = unsafe extern "C" fn(handle: *mut c_void);
type RetroVulkanSetSignalSemaphoreFn =
    unsafe extern "C" fn(handle: *mut c_void, semaphore: vk::Semaphore);

#[repr(C)]
struct RetroHwRenderInterfaceVulkan {
    interface_type: u32,
    interface_version: u32,
    handle: *mut c_void,
    instance: vk::Instance,
    gpu: vk::PhysicalDevice,
    device: vk::Device,
    get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    queue: vk::Queue,
    queue_index: u32,
    set_image: Option<RetroVulkanSetImageFn>,
    get_sync_index: Option<RetroVulkanGetSyncIndexFn>,
    get_sync_index_mask: Option<RetroVulkanGetSyncIndexFn>,
    set_command_buffers: Option<RetroVulkanSetCommandBuffersFn>,
    wait_sync_index: Option<RetroVulkanWaitSyncIndexFn>,
    lock_queue: Option<RetroVulkanLockQueueFn>,
    unlock_queue: Option<RetroVulkanUnlockQueueFn>,
    set_signal_semaphore: Option<RetroVulkanSetSignalSemaphoreFn>,
}

#[repr(C)]
struct RetroGameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

#[repr(C)]
struct RetroSystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct RetroGameGeometry {
    base_width: u32,
    base_height: u32,
    max_width: u32,
    max_height: u32,
    aspect_ratio: f32,
}

#[repr(C)]
struct RetroSystemTiming {
    fps: f64,
    sample_rate: f64,
}

#[repr(C)]
struct RetroSystemAvInfo {
    geometry: RetroGameGeometry,
    timing: RetroSystemTiming,
}

#[repr(C)]
struct RetroVariable {
    key: *const c_char,
    value: *const c_char,
}

#[repr(C)]
struct RetroControllerDescription {
    desc: *const c_char,
    id: u32,
}

#[repr(C)]
struct RetroControllerInfo {
    types: *const RetroControllerDescription,
    num_types: u32,
}

#[repr(C)]
struct RetroLogCallback {
    log: *const c_void,
}

#[repr(C)]
struct RetroVfsInterfaceInfo {
    required_interface_version: u32,
    iface: *mut RetroVfsInterface,
}

#[repr(C)]
struct RetroVfsInterface {
    get_path: unsafe extern "C" fn(*mut RetroVfsFileHandle) -> *const c_char,
    open: unsafe extern "C" fn(*const c_char, u32, u32) -> *mut RetroVfsFileHandle,
    close: unsafe extern "C" fn(*mut RetroVfsFileHandle) -> i32,
    size: unsafe extern "C" fn(*mut RetroVfsFileHandle) -> i64,
    tell: unsafe extern "C" fn(*mut RetroVfsFileHandle) -> i64,
    seek: unsafe extern "C" fn(*mut RetroVfsFileHandle, i64, i32) -> i64,
    read: unsafe extern "C" fn(*mut RetroVfsFileHandle, *mut c_void, u64) -> i64,
    write: unsafe extern "C" fn(*mut RetroVfsFileHandle, *const c_void, u64) -> i64,
    flush: unsafe extern "C" fn(*mut RetroVfsFileHandle) -> i32,
    remove: unsafe extern "C" fn(*const c_char) -> i32,
    rename: unsafe extern "C" fn(*const c_char, *const c_char) -> i32,
    truncate: unsafe extern "C" fn(*mut RetroVfsFileHandle, i64) -> i64,
}

struct RetroVfsFileHandle {
    id: u64,
    file: File,
    path: CString,
}

const RETRO_VFS_FILE_ACCESS_READ: u32 = 1 << 0;
const RETRO_VFS_FILE_ACCESS_WRITE: u32 = 1 << 1;
const RETRO_VFS_FILE_ACCESS_UPDATE_EXISTING: u32 = 1 << 2;
const RETRO_VFS_SEEK_POSITION_START: i32 = 0;
const RETRO_VFS_SEEK_POSITION_CURRENT: i32 = 1;
const RETRO_VFS_SEEK_POSITION_END: i32 = 2;

static VFS_INTERFACE: RetroVfsInterface = RetroVfsInterface {
    get_path: retro_vfs_get_path,
    open: retro_vfs_open,
    close: retro_vfs_close,
    size: retro_vfs_size,
    tell: retro_vfs_tell,
    seek: retro_vfs_seek,
    read: retro_vfs_read,
    write: retro_vfs_write,
    flush: retro_vfs_flush,
    remove: retro_vfs_remove,
    rename: retro_vfs_rename,
    truncate: retro_vfs_truncate,
};

static NEXT_VFS_HANDLE_ID: AtomicU64 = AtomicU64::new(1);
static VFS_TRACE_ENABLED: Lazy<bool> =
    Lazy::new(|| std::env::var_os("LIBRETRO_TRACE_VFS").is_some());
static VFS_SEEK_COMPAT_ENABLED: Lazy<bool> =
    Lazy::new(|| std::env::var_os("LIBRETRO_VFS_SEEK_COMPAT").is_some());
static VFS_DISABLED: Lazy<bool> = Lazy::new(|| std::env::var_os("LIBRETRO_DISABLE_VFS").is_some());

struct CoreApi {
    set_environment: RetroSetEnvironment,
    set_video_refresh: RetroSetVideoRefresh,
    set_audio_sample: RetroSetAudioSample,
    set_audio_sample_batch: RetroSetAudioSampleBatch,
    set_input_poll: RetroSetInputPoll,
    set_input_state: RetroSetInputState,
    set_controller_port_device: Option<RetroSetControllerPortDevice>,
    init: RetroInit,
    deinit: RetroDeinit,
    api_version: RetroApiVersion,
    get_system_info: RetroGetSystemInfo,
    get_system_av_info: RetroGetSystemAvInfo,
    load_game: RetroLoadGame,
    unload_game: RetroUnloadGame,
    reset: RetroReset,
    run: RetroRun,
    serialize_size: RetroSerializeSize,
    serialize: RetroSerialize,
    unserialize: RetroUnserialize,
}

struct LoadedCore {
    _library: Library,
    api: CoreApi,
    core_path: PathBuf,
    rom_path: PathBuf,
    _content_data: Option<Vec<u8>>,
    _launch_session: Option<LaunchSession>,
    video_fps: f64,
    video_aspect_ratio: f32,
    video_base_size: (u32, u32),
    video_max_size: (u32, u32),
    uses_hw_render: bool,
}

fn default_core_library_filename(core_name: &str) -> String {
    core_library_filename_candidates(core_name)
        .into_iter()
        .next()
        .unwrap_or_else(|| format!("{core_name}_libretro.so"))
}

fn core_library_filename_candidates(core_name: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        return vec![
            format!("{core_name}_libretro.dll"),
            format!("{core_name}.dll"),
        ];
    }

    #[cfg(target_os = "macos")]
    {
        return vec![format!("{core_name}_libretro.dylib")];
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        vec![format!("{core_name}_libretro.so")]
    }
}

impl GlProcLoader {
    fn new() -> Self {
        #[cfg(all(unix, not(target_os = "macos")))]
        unsafe {
            let libgl = Library::new("libGL.so.1").ok();
            let libegl = Library::new("libEGL.so.1").ok();

            let glx_get_proc_address = libgl.as_ref().and_then(|lib| {
                let symbol: std::result::Result<Symbol<GlxGetProcAddress>, _> =
                    lib.get(b"glXGetProcAddressARB");
                symbol.ok().map(|symbol| *symbol)
            });
            let egl_get_proc_address = libegl.as_ref().and_then(|lib| {
                let symbol: std::result::Result<Symbol<EglGetProcAddress>, _> =
                    lib.get(b"eglGetProcAddress");
                symbol.ok().map(|symbol| *symbol)
            });

            Self {
                libgl,
                libegl,
                glx_get_proc_address,
                egl_get_proc_address,
            }
        }

        #[cfg(target_os = "macos")]
        unsafe {
            let opengl_framework =
                Library::new("/System/Library/Frameworks/OpenGL.framework/OpenGL").ok();

            Self { opengl_framework }
        }

        #[cfg(target_os = "windows")]
        unsafe {
            let opengl32 = Library::new("opengl32.dll").ok();
            let wgl_get_proc_address = opengl32.as_ref().and_then(|lib| {
                let symbol: std::result::Result<Symbol<WglGetProcAddress>, _> =
                    lib.get(b"wglGetProcAddress");
                symbol.ok().map(|symbol| *symbol)
            });

            Self {
                opengl32,
                wgl_get_proc_address,
            }
        }
    }

    fn get(&self, sym: &CStr) -> *const c_void {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if let Some(glx_get_proc_address) = self.glx_get_proc_address {
                let ptr = unsafe { glx_get_proc_address(sym.as_ptr() as *const u8) };
                if !ptr.is_null() {
                    return ptr;
                }
            }
            if let Some(egl_get_proc_address) = self.egl_get_proc_address {
                let ptr = unsafe { egl_get_proc_address(sym.as_ptr()) };
                if !ptr.is_null() {
                    return ptr;
                }
            }
            if let Some(lib) = self.libgl.as_ref() {
                if let Ok(symbol) = unsafe { lib.get::<*const c_void>(sym.to_bytes_with_nul()) } {
                    if !(*symbol).is_null() {
                        return *symbol;
                    }
                }
            }
            if let Some(lib) = self.libegl.as_ref() {
                if let Ok(symbol) = unsafe { lib.get::<*const c_void>(sym.to_bytes_with_nul()) } {
                    if !(*symbol).is_null() {
                        return *symbol;
                    }
                }
            }
            std::ptr::null()
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(lib) = self.opengl_framework.as_ref() {
                if let Ok(symbol) = unsafe { lib.get::<*const c_void>(sym.to_bytes_with_nul()) } {
                    if !(*symbol).is_null() {
                        return *symbol;
                    }
                }
            }

            std::ptr::null()
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(wgl_get_proc_address) = self.wgl_get_proc_address {
                let ptr = unsafe { wgl_get_proc_address(sym.as_ptr()) };
                if is_valid_wgl_proc_address(ptr) {
                    return ptr;
                }
            }

            if let Some(lib) = self.opengl32.as_ref() {
                if let Ok(symbol) = unsafe { lib.get::<*const c_void>(sym.to_bytes_with_nul()) } {
                    if !(*symbol).is_null() {
                        return *symbol;
                    }
                }
            }

            std::ptr::null()
        }
    }
}

#[cfg(target_os = "windows")]
fn is_valid_wgl_proc_address(ptr: *const c_void) -> bool {
    let value = ptr as isize;
    !ptr.is_null() && !matches!(value, -1 | 1 | 2 | 3)
}

#[cfg(feature = "audio")]
struct AudioOutput {
    _stream: cpal::Stream,
    sample_rate_hz: u32,
}

#[cfg(feature = "audio")]
impl AudioOutput {
    fn new(preferred_sample_rate_hz: Option<u32>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output device found"))?;
        let supported_config = select_output_config(&device, preferred_sample_rate_hz)?;
        let channels = supported_config.channels() as usize;
        let mut stream_config: cpal::StreamConfig = supported_config.config();
        let sample_format = supported_config.sample_format();
        let sample_rate_hz = stream_config.sample_rate.0;
        let custom_buffer_size = pick_buffer_size(&supported_config, sample_rate_hz);
        if let Some(buffer_size) = custom_buffer_size {
            stream_config.buffer_size = buffer_size;
        }
        set_audio_output_sample_rate(sample_rate_hz as f64);
        let stream = match build_output_stream(&device, sample_format, &stream_config, channels) {
            Ok(stream) => stream,
            Err(err) if custom_buffer_size.is_some() => {
                eprintln!(
                    "Custom audio buffer size rejected ({err}); retrying with backend default."
                );
                stream_config.buffer_size = cpal::BufferSize::Default;
                build_output_stream(&device, sample_format, &stream_config, channels)?
            }
            Err(err) => return Err(err),
        };

        stream.play()?;
        Ok(Self {
            _stream: stream,
            sample_rate_hz,
        })
    }
}

#[cfg(feature = "audio")]
fn pick_buffer_size(
    supported_config: &cpal::SupportedStreamConfig,
    sample_rate_hz: u32,
) -> Option<cpal::BufferSize> {
    match supported_config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            // Prefer ~16ms callback buffers for smoother pacing on bursty cores.
            let target = (sample_rate_hz / 60).clamp(256, 2048);
            Some(cpal::BufferSize::Fixed(target.clamp(*min, *max)))
        }
        _ => None,
    }
}

#[cfg(feature = "audio")]
fn build_output_stream(
    device: &cpal::Device,
    sample_format: cpal::SampleFormat,
    stream_config: &cpal::StreamConfig,
    channels: usize,
) -> Result<cpal::Stream> {
    fn err_fn(err: cpal::StreamError) {
        eprintln!("cpal output stream error: {err}");
    }

    let stream = match sample_format {
        cpal::SampleFormat::I16 => device.build_output_stream(
            stream_config,
            move |data: &mut [i16], _| write_output_i16(data, channels),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            stream_config,
            move |data: &mut [u16], _| write_output_u16(data, channels),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_output_stream(
            stream_config,
            move |data: &mut [f32], _| write_output_f32(data, channels),
            err_fn,
            None,
        )?,
        other => {
            return Err(anyhow!("unsupported cpal sample format: {other:?}"));
        }
    };

    Ok(stream)
}

#[cfg(feature = "audio")]
fn select_output_config(
    device: &cpal::Device,
    preferred_sample_rate_hz: Option<u32>,
) -> Result<cpal::SupportedStreamConfig> {
    let default_config = device.default_output_config()?;
    let Some(target_rate) = preferred_sample_rate_hz.filter(|rate| *rate > 0) else {
        return Ok(default_config);
    };

    let default_channels = default_config.channels();
    let mut best: Option<(u64, cpal::SupportedStreamConfig)> = None;

    for range in device.supported_output_configs()? {
        let min_rate = range.min_sample_rate().0;
        let max_rate = range.max_sample_rate().0;
        let selected_rate = target_rate.clamp(min_rate, max_rate);
        let config = range.with_sample_rate(cpal::SampleRate(selected_rate));

        let rate_penalty = u32::abs_diff(selected_rate, target_rate) as u64 * 1000;
        let channel_penalty = u16::abs_diff(config.channels(), default_channels) as u64 * 50;
        let format_penalty = if config.sample_format() == cpal::SampleFormat::I16 {
            0
        } else if config.sample_format() == default_config.sample_format() {
            500_000
        } else {
            10_000_000
        };
        let score = rate_penalty + channel_penalty + format_penalty;

        if best
            .as_ref()
            .map(|(best_score, _)| score < *best_score)
            .unwrap_or(true)
        {
            best = Some((score, config));
            if score == 0 {
                break;
            }
        }
    }

    Ok(best.map(|(_, config)| config).unwrap_or(default_config))
}

#[cfg(not(feature = "audio"))]
struct AudioOutput {
    sample_rate_hz: u32,
}

#[cfg(not(feature = "audio"))]
impl AudioOutput {
    fn new(_preferred_sample_rate_hz: Option<u32>) -> Result<Self> {
        Err(anyhow!("audio feature is disabled at compile time"))
    }
}

#[derive(Clone)]
pub struct LibretroHost {
    core_root: PathBuf,
    system_root: PathBuf,
    save_root: PathBuf,
    emulation: EmulationConfig,
    runtime: Arc<HostRuntime>,
    loaded: Arc<Mutex<Option<LoadedCore>>>,
    audio_output: Rc<RefCell<Option<AudioOutput>>>,
}

impl LibretroHost {
    pub fn new(
        core_root: PathBuf,
        system_root: PathBuf,
        save_root: PathBuf,
        emulation: EmulationConfig,
    ) -> Self {
        Self {
            core_root,
            system_root,
            save_root,
            emulation,
            runtime: Arc::new(HostRuntime::default()),
            loaded: Arc::new(Mutex::new(None)),
            audio_output: Rc::new(RefCell::new(None)),
        }
    }

    pub fn set_frontend_capabilities(&self, capabilities: FrontendCapabilities) {
        if let Some(gl) = capabilities.gl_context.as_ref() {
            log_frontend_gl_context(gl);
        }
        let changed = self
            .runtime
            .video_coordinator
            .lock()
            .apply_frontend_capabilities(&self.runtime, capabilities.clone());
        self.runtime.hw_render_state.lock().frontend_gl_context = capabilities.gl_context.clone();
        if changed {
            self.runtime
                .hw_render_state
                .lock()
                .external_vulkan_probe_logged = false;
        }
        if changed && vulkan_debug_enabled() {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "frontend windowing renderer={:?} window_handle={:?} display_handle={:?} env_win32_external_present={}",
                capabilities.renderer_name,
                capabilities.window_handle_kind,
                capabilities.display_handle_kind,
                std::env::var_os("ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT").is_some()
            );
        }
    }

    pub fn using_external_vulkan_present_window(&self) -> bool {
        self.runtime
            .video_coordinator
            .lock()
            .using_external_present(&self.runtime)
    }

    pub fn has_external_vulkan_present_window(&self) -> bool {
        self.runtime
            .video_coordinator
            .lock()
            .has_external_present_window(&self.runtime)
    }

    pub fn set_external_overlay_message(&self, message: Option<&str>) {
        self.runtime
            .video_coordinator
            .lock()
            .set_overlay_message(&self.runtime, message);
    }

    pub fn core_root(&self) -> &Path {
        &self.core_root
    }

    pub fn resolve_core_candidates(&self, core_name: &str) -> Vec<PathBuf> {
        core_library_filename_candidates(core_name)
            .into_iter()
            .map(|file_name| self.core_root.join(file_name))
            .collect()
    }

    pub fn resolve_core_path(&self, core_name: &str) -> PathBuf {
        self.resolve_core_candidates(core_name)
            .into_iter()
            .find(|path| path.exists())
            .unwrap_or_else(|| {
                self.core_root
                    .join(default_core_library_filename(core_name))
            })
    }

    pub fn load_for_rom(
        &self,
        system: &str,
        core_override: Option<&str>,
        rom_path: &Path,
    ) -> Result<String> {
        let core_name = resolve_core(system, core_override);
        let candidates = self.resolve_core_candidates(&core_name);
        let core_path = candidates
            .iter()
            .find(|path| path.exists())
            .cloned()
            .ok_or_else(|| {
                let checked = candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                LibretroError::CoreNotFound(format!("{} (checked: {checked})", core_name))
            })?;
        let launch_session =
            prepare_launch_session(system, &core_name, rom_path, &self.system_root)?;
        let launch_rom_path = launch_session
            .as_ref()
            .map(|session| session.launch_rom_path.clone())
            .unwrap_or_else(|| rom_path.to_path_buf());
        self.load_core(
            &core_path,
            &core_name,
            rom_path,
            &launch_rom_path,
            launch_session,
        )?;
        Ok(core_name)
    }

    fn load_core(
        &self,
        core_path: &Path,
        core_name: &str,
        rom_path: &Path,
        launch_rom_path: &Path,
        launch_session: Option<LaunchSession>,
    ) -> Result<()> {
        if !core_path.exists() {
            return Err(LibretroError::CoreNotFound(core_path.display().to_string()).into());
        }

        let _ = self.unload();
        let requirements = inspect_core_requirements(core_path)?;
        let selection = {
            let mut coordinator = self.runtime.video_coordinator.lock();
            coordinator.plan_session(VideoSessionInfo {
                core_name: core_name.to_string(),
                requires_hw_render: requirements.requires_hw_render,
                requested_hw_context_type: None,
            })
        };
        if std::env::var_os("LIBRETRO_TRACE_BACKEND").is_some() {
            eprintln!(
                "libretro backend core={} chosen={:?} fallbacks={:?} requires_hw_render={}",
                core_name, selection.chosen, selection.fallbacks, requirements.requires_hw_render
            );
        }
        if vulkan_debug_enabled() {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "backend selection core={} chosen={:?} fallbacks={:?} requires_hw_render={} macos_experimental_vulkan={}",
                core_name,
                selection.chosen,
                selection.fallbacks,
                requirements.requires_hw_render,
                video::macos_parallel_n64_vulkan_enabled()
            );
        }
        apply_core_runtime_env_defaults(core_name, selection.chosen);
        configure_environment_context(
            &self.runtime,
            &self.system_root,
            &self.save_root,
            core_name,
            selection.chosen,
            &self.emulation,
        );
        self.runtime
            .video_coordinator
            .lock()
            .begin_session(&self.runtime)?;
        if requirements.requires_hw_render
            && !hardware_render_preflight_available_for_core(&self.runtime, core_name)
            && !core_has_embedded_software_video_fallback(core_name)
        {
            return Err(anyhow!(
                "core requires hardware-render integration that this embedded host does not implement yet: {}",
                core_path.display()
            ));
        }

        let mut prepared = prepare_game_content(launch_rom_path, &requirements, core_name)?;
        let mut launch_session = launch_session;
        let mut failures = Vec::new();

        for strategy in &prepared.strategies {
            reset_callback_video_state(&self.runtime);
            {
                let mut context = self.runtime.environment_context.lock();
                context.requested_hw_render = false;
                context.requested_hw_context_type = None;
                context.last_load_error = None;
                context.last_negotiation_interface = None;
            }

            if matches!(
                *strategy,
                LoadGameStrategy::PathAndData | LoadGameStrategy::DataOnly
            ) && prepared.data.is_none()
            {
                prepared.data = Some(fs::read(launch_rom_path).with_context(|| {
                    format!("failed to read ROM bytes {}", launch_rom_path.display())
                })?);
            }

            let library = unsafe { Library::new(core_path) }
                .with_context(|| format!("failed to load core {}", core_path.display()))?;
            let api = unsafe { load_api(&library)? };
            register_active_runtime(&self.runtime);

            unsafe {
                (api.set_environment)(retro_environment);
                (api.set_video_refresh)(retro_video_refresh);
                (api.set_audio_sample)(retro_audio_sample);
                (api.set_audio_sample_batch)(retro_audio_sample_batch);
                (api.set_input_poll)(retro_input_poll);
                (api.set_input_state)(retro_input_state);
                (api.init)();
            }
            if hardware_render_requested() {
                initialize_hw_render_context((640, 480))?;
            }

            let game_info = build_game_info(*strategy, &prepared.path, prepared.data.as_deref());
            let loaded = unsafe { (api.load_game)(&game_info as *const RetroGameInfo) };
            if loaded {
                let last_load_error = self
                    .runtime
                    .environment_context
                    .lock()
                    .last_load_error
                    .clone();
                if let Some(reason) = last_load_error {
                    unsafe {
                        (api.unload_game)();
                    }
                    destroy_hw_render_session();
                    clear_active_runtime(&self.runtime);
                    unsafe {
                        (api.deinit)();
                    }
                    return Err(LibretroError::UnsupportedHardwareRender(reason).into());
                }
                if hardware_render_requested() && !hardware_render_context_ready() {
                    initialize_hw_render_context((640, 480))?;
                }
                let mut av_info = RetroSystemAvInfo {
                    geometry: RetroGameGeometry {
                        base_width: 0,
                        base_height: 0,
                        max_width: 0,
                        max_height: 0,
                        aspect_ratio: 0.0,
                    },
                    timing: RetroSystemTiming {
                        fps: 60.0,
                        sample_rate: 48_000.0,
                    },
                };
                unsafe {
                    (api.get_system_av_info)(&mut av_info as *mut RetroSystemAvInfo);
                }
                let _version = unsafe { (api.api_version)() };
                let preferred_sample_rate_hz =
                    if av_info.timing.sample_rate.is_finite() && av_info.timing.sample_rate > 0.0 {
                        Some(av_info.timing.sample_rate.round() as u32)
                    } else {
                        None
                    };
                set_audio_source_sample_rate(av_info.timing.sample_rate);
                if let Err(err) = self.ensure_audio_output_started(preferred_sample_rate_hz) {
                    eprintln!("Audio output unavailable, continuing without sound: {err}");
                }
                let mut video_aspect_ratio = av_info.geometry.aspect_ratio;
                if !video_aspect_ratio.is_finite() || video_aspect_ratio <= 0.0 {
                    if av_info.geometry.base_width > 0 && av_info.geometry.base_height > 0 {
                        video_aspect_ratio = av_info.geometry.base_width as f32
                            / av_info.geometry.base_height as f32;
                    } else {
                        video_aspect_ratio = 0.0;
                    }
                }
                let loaded_core = LoadedCore {
                    _library: library,
                    api,
                    core_path: core_path.to_path_buf(),
                    rom_path: rom_path.to_path_buf(),
                    _content_data: if strategy_uses_data(*strategy) {
                        prepared.data.take()
                    } else {
                        None
                    },
                    _launch_session: launch_session.take(),
                    video_fps: av_info.timing.fps,
                    video_aspect_ratio,
                    video_base_size: (av_info.geometry.base_width, av_info.geometry.base_height),
                    video_max_size: (
                        av_info.geometry.max_width.max(av_info.geometry.base_width),
                        av_info
                            .geometry
                            .max_height
                            .max(av_info.geometry.base_height),
                    ),
                    uses_hw_render: hardware_render_requested(),
                };

                apply_runtime_geometry_update(&self.runtime, av_info.geometry);
                *self.loaded.lock() = Some(loaded_core);
                let _ = self.configure_default_controller_ports(4);
                return Ok(());
            }

            destroy_hw_render_session();
            clear_active_runtime(&self.runtime);
            unsafe {
                (api.deinit)();
            }
            let context = self.runtime.environment_context.lock();
            let requested_hw_render = context.requested_hw_render;
            let requested_hw_context_type = context.requested_hw_context_type;
            let last_load_error = context.last_load_error.clone();
            let last_negotiation_interface = context.last_negotiation_interface;
            drop(context);
            if let Some(reason) = last_load_error {
                return Err(LibretroError::UnsupportedHardwareRender(reason).into());
            }
            if let Some((interface_type, interface_version)) = last_negotiation_interface {
                failures.push(format!(
                    "{strategy:?} (core requested render negotiation interface {} ({}) v{})",
                    interface_type,
                    hw_render_interface_type_name(interface_type),
                    interface_version
                ));
                continue;
            }
            if requested_hw_render {
                failures.push(format!(
                    "{strategy:?} (core requested unsupported hardware render{}{})",
                    requested_hw_context_type
                        .map(|value| format!(" context={value}"))
                        .unwrap_or_default(),
                    requested_hw_context_type
                        .map(|value| format!(" ({})", hw_context_type_name(value)))
                        .unwrap_or_default()
                ));
            } else {
                failures.push(format!("{strategy:?}"));
            }
        }

        if let Some(reason) = self
            .runtime
            .environment_context
            .lock()
            .last_load_error
            .clone()
        {
            return Err(LibretroError::UnsupportedHardwareRender(reason).into());
        }

        Err(anyhow!(
            "{}: core={} rom={} need_fullpath={} attempts={}",
            LibretroError::LoadGameFailed,
            core_path.display(),
            launch_rom_path.display(),
            requirements.need_fullpath,
            failures.join(", ")
        ))
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.lock().is_some()
    }

    pub fn loaded_paths(&self) -> Option<(PathBuf, PathBuf)> {
        self.loaded
            .lock()
            .as_ref()
            .map(|c| (c.core_path.clone(), c.rom_path.clone()))
    }

    pub fn frame_interval(&self) -> Option<std::time::Duration> {
        let mut fps = self.loaded.lock().as_ref().map(|c| c.video_fps)?;
        if !fps.is_finite() || fps <= 0.0 {
            return None;
        }

        fps = normalize_display_fps(fps);

        Some(std::time::Duration::from_secs_f64(1.0 / fps))
    }

    pub fn video_aspect_ratio(&self) -> Option<f32> {
        let aspect = self.loaded.lock().as_ref().map(|c| c.video_aspect_ratio)?;
        if aspect.is_finite() && aspect > 0.0 {
            Some(aspect)
        } else {
            None
        }
    }

    pub fn run_frame(&self) -> Result<Option<FrameBuffer>> {
        let loaded_guard = self.loaded.lock();
        let Some(loaded) = loaded_guard.as_ref() else {
            return Ok(None);
        };
        let uses_hw_render = loaded.uses_hw_render;
        let core_label = loaded
            .core_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown_core")
            .to_owned();
        let is_parallel_n64 = loaded
            .core_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains("parallel_n64"));
        let target_size = hw_render_target_size(loaded.video_max_size, loaded.video_base_size);
        drop(loaded_guard);

        if uses_hw_render && hardware_render_requested() && !hardware_render_context_ready() {
            invoke_hw_context_reset()?;
        }

        let loaded_guard = self.loaded.lock();
        let Some(loaded) = loaded_guard.as_ref() else {
            return Ok(None);
        };

        let using_vulkan_hw_render = uses_hw_render
            && with_active_runtime(|runtime| {
                runtime.hw_render_state.lock().context_type == Some(RETRO_HW_CONTEXT_VULKAN)
            })
            .unwrap_or(false);
        let run_frame_debug_step = if vulkan_debug_enabled() && using_vulkan_hw_render {
            Some(VULKAN_RUN_FRAME_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed))
        } else {
            None
        };

        pump_external_vulkan_window_events(&self.runtime);
        if using_vulkan_hw_render {
            if let Some(debug_step) = run_frame_debug_step.filter(|step| *step < 8) {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "run_frame step={} preparing Vulkan sync",
                    debug_step
                );
            }
        }
        self.runtime
            .video_coordinator
            .lock()
            .prepare_frame(&self.runtime, uses_hw_render.then_some(target_size))?;
        if let Some(debug_step) = run_frame_debug_step.filter(|step| *step < 8) {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "run_frame step={} calling retro_run",
                debug_step
            );
        }
        let run_started_at = std::time::Instant::now();
        unsafe {
            (loaded.api.run)();
        }
        let run_duration = run_started_at.elapsed();
        if uses_hw_render {
            drain_frontend_gl_errors(&self.runtime, "retro_run");
        }
        if let Some(debug_step) = run_frame_debug_step.filter(|step| *step < 8) {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "run_frame step={} retro_run returned in {:.3} ms",
                debug_step,
                run_duration.as_secs_f64() * 1000.0
            );
        }

        let present_started_at = std::time::Instant::now();
        let frame = self
            .runtime
            .video_coordinator
            .lock()
            .consume_frame(&self.runtime);
        let present_duration = present_started_at.elapsed();
        if uses_hw_render {
            drain_frontend_gl_errors(&self.runtime, "frame delivery");
        }
        let delivery = match &frame {
            Ok(FrameDelivery::CpuFrame(frame)) => FrameDelivery::CpuFrame(frame.clone()),
            Ok(FrameDelivery::ExternalPresent) => FrameDelivery::ExternalPresent,
            Ok(FrameDelivery::NoFrame) => FrameDelivery::NoFrame,
            Ok(FrameDelivery::Error(err)) => FrameDelivery::Error(err.clone()),
            Err(err) => FrameDelivery::Error(err.to_string()),
        };

        if uses_hw_render {
            if let Some(debug_step) = run_frame_debug_step.filter(|step| *step < 8) {
                match &frame {
                    Ok(FrameDelivery::CpuFrame(frame)) => info!(
                        target: "arcade_libretro::vulkan_debug",
                        "run_frame step={} hw frame result=cpu {}x{} present_ms={:.3}",
                        debug_step,
                        frame.width,
                        frame.height,
                        present_duration.as_secs_f64() * 1000.0
                    ),
                    Ok(FrameDelivery::ExternalPresent) | Ok(FrameDelivery::NoFrame) => info!(
                        target: "arcade_libretro::vulkan_debug",
                        "run_frame step={} hw frame result=none present_ms={:.3}",
                        debug_step,
                        present_duration.as_secs_f64() * 1000.0
                    ),
                    Ok(FrameDelivery::Error(err)) => info!(
                        target: "arcade_libretro::vulkan_debug",
                        "run_frame step={} hw frame result=error err={} present_ms={:.3}",
                        debug_step,
                        err,
                        present_duration.as_secs_f64() * 1000.0
                    ),
                    Err(err) => info!(
                        target: "arcade_libretro::vulkan_debug",
                        "run_frame step={} hw frame result=error err={} present_ms={:.3}",
                        debug_step,
                        err,
                        present_duration.as_secs_f64() * 1000.0
                    ),
                }
            }
            update_external_vulkan_present_state(
                &self.runtime,
                matches!(delivery, FrameDelivery::ExternalPresent),
            );
        }

        if is_parallel_n64 {
            record_frame_performance(
                &self.runtime,
                &core_label,
                run_duration,
                present_duration,
                delivery,
            );
        }

        match frame {
            Ok(FrameDelivery::CpuFrame(frame)) => Ok(Some(frame)),
            Ok(FrameDelivery::ExternalPresent | FrameDelivery::NoFrame) => Ok(None),
            Ok(FrameDelivery::Error(err)) => Err(anyhow!(err)),
            Err(err) => Err(err),
        }
    }

    pub fn reset(&self) -> Result<()> {
        let loaded_guard = self.loaded.lock();
        let Some(loaded) = loaded_guard.as_ref() else {
            return Ok(());
        };

        unsafe {
            (loaded.api.reset)();
        }
        drop(loaded_guard);
        let _ = self.configure_default_controller_ports(4);

        Ok(())
    }

    pub fn serialize_state(&self) -> Result<Option<Vec<u8>>> {
        let loaded_guard = self.loaded.lock();
        let Some(loaded) = loaded_guard.as_ref() else {
            return Ok(None);
        };

        let size = unsafe { (loaded.api.serialize_size)() };
        if size == 0 {
            return Ok(None);
        }

        let mut data = vec![0_u8; size];
        let success = unsafe { (loaded.api.serialize)(data.as_mut_ptr() as *mut c_void, size) };
        if !success {
            return Err(anyhow!("libretro serialize failed"));
        }

        Ok(Some(data))
    }

    pub fn unserialize_state(&self, data: &[u8]) -> Result<bool> {
        let loaded_guard = self.loaded.lock();
        let Some(loaded) = loaded_guard.as_ref() else {
            return Ok(false);
        };

        let success =
            unsafe { (loaded.api.unserialize)(data.as_ptr() as *const c_void, data.len()) };
        Ok(success)
    }

    pub fn set_input_state(&self, port: u32, device: u32, index: u32, id: u32, value: i16) {
        self.runtime
            .callback_state
            .lock()
            .input_state
            .insert((port, device, index, id), value);
    }

    pub fn clear_input_state(&self) {
        self.runtime.callback_state.lock().input_state.clear();
    }

    pub fn configure_default_controller_ports(&self, max_ports: u32) -> Result<()> {
        let loaded_guard = self.loaded.lock();
        let Some(loaded) = loaded_guard.as_ref() else {
            return Ok(());
        };
        let Some(set_controller_port_device) = loaded.api.set_controller_port_device else {
            return Ok(());
        };

        let context = self.runtime.environment_context.lock();
        for port in 0..max_ports as usize {
            let device =
                preferred_controller_device(context.controller_info.get(port).map(Vec::as_slice));
            unsafe {
                set_controller_port_device(port as u32, device);
            }
        }

        Ok(())
    }

    pub fn unload(&self) -> Result<()> {
        let mut loaded_guard = self.loaded.lock();
        if let Some(loaded) = loaded_guard.take() {
            unsafe {
                (loaded.api.unload_game)();
            }
            destroy_hw_render_session();
            unsafe {
                (loaded.api.deinit)();
            }
        } else {
            destroy_hw_render_session();
        }
        reset_callback_video_state(&self.runtime);
        self.runtime.callback_state.lock().input_state.clear();
        self.runtime
            .environment_context
            .lock()
            .controller_info
            .clear();
        clear_active_runtime(&self.runtime);
        Ok(())
    }

    fn ensure_audio_output_started(&self, preferred_sample_rate_hz: Option<u32>) -> Result<()> {
        let mut guard = self.audio_output.borrow_mut();
        if let Some(existing) = guard.as_ref() {
            let requested = preferred_sample_rate_hz.unwrap_or(existing.sample_rate_hz);
            if requested == existing.sample_rate_hz {
                return Ok(());
            }
        }

        let output = AudioOutput::new(preferred_sample_rate_hz)?;
        *guard = Some(output);
        Ok(())
    }
}

fn normalize_display_fps(fps: f64) -> f64 {
    if !fps.is_finite() || fps <= 0.0 {
        return fps;
    }

    // Normalize only near the most common stable presentation rates. This keeps console-style
    // systems from feeling slightly fast while preserving unusual native arcade timings.
    for target in COMMON_DISPLAY_FPS {
        if (fps - target).abs() <= DISPLAY_FPS_TOLERANCE {
            return *target;
        }
    }

    fps
}

fn duration_to_ns(duration: std::time::Duration) -> u64 {
    duration
        .as_nanos()
        .min(u64::MAX as u128)
        .try_into()
        .unwrap_or(u64::MAX)
}

fn record_frame_performance(
    runtime: &HostRuntime,
    core_label: &str,
    run_duration: std::time::Duration,
    present_duration: std::time::Duration,
    delivery: FrameDelivery,
) {
    let now = std::time::Instant::now();
    let mut state = runtime.performance_log_state.lock();
    let sample_started_at = state.sample_started_at.unwrap_or(now);
    if state.sample_started_at.is_none() {
        state.sample_started_at = Some(now);
    }
    state.sample_frames = state.sample_frames.saturating_add(1);
    state.accumulated_run_ns = state
        .accumulated_run_ns
        .saturating_add(duration_to_ns(run_duration));
    state.accumulated_present_ns = state
        .accumulated_present_ns
        .saturating_add(duration_to_ns(present_duration));

    match delivery {
        FrameDelivery::ExternalPresent => {
            state.external_present_frames = state.external_present_frames.saturating_add(1);
        }
        FrameDelivery::CpuFrame(frame) => {
            state.cpu_frame_count = state.cpu_frame_count.saturating_add(1);
            state.last_frame_size = Some((frame.width, frame.height));
        }
        FrameDelivery::NoFrame => {
            state.empty_frame_count = state.empty_frame_count.saturating_add(1);
        }
        FrameDelivery::Error(_) => {
            state.error_count = state.error_count.saturating_add(1);
        }
    }

    if state.sample_frames < PERF_LOG_SAMPLE_FRAMES {
        return;
    }

    let elapsed = now
        .duration_since(sample_started_at)
        .as_secs_f64()
        .max(f64::EPSILON);
    let frames = state.sample_frames.max(1) as f64;
    let avg_run_ms = state.accumulated_run_ns as f64 / frames / 1_000_000.0;
    let avg_present_ms = state.accumulated_present_ns as f64 / frames / 1_000_000.0;
    let avg_total_ms = avg_run_ms + avg_present_ms;
    let fps = frames / elapsed;
    let last_size = state
        .last_frame_size
        .map(|(width, height)| format!("{width}x{height}"))
        .unwrap_or_else(|| String::from("n/a"));

    info!(
        target: "arcade_libretro::perf",
        "perf core={core_label} frames={} fps={:.1} avg_run_ms={:.2} avg_present_ms={:.2} avg_total_ms={:.2} external_present={} cpu_frames={} empty_frames={} errors={} last_frame={}",
        state.sample_frames,
        fps,
        avg_run_ms,
        avg_present_ms,
        avg_total_ms,
        state.external_present_frames,
        state.cpu_frame_count,
        state.empty_frame_count,
        state.error_count,
        last_size,
    );

    *state = PerformanceLogState {
        sample_started_at: Some(now),
        ..PerformanceLogState::default()
    };
}

impl Drop for LibretroHost {
    fn drop(&mut self) {
        let _ = self.unload();
    }
}

fn reset_callback_video_state(runtime: &HostRuntime) {
    let mut callback_state = runtime.callback_state.lock();
    callback_state.latest_frame = None;
    callback_state.latest_hw_frame = None;
    callback_state.pixel_format = PixelFormat::Argb1555;
    let mut audio_state = runtime.audio_state.lock();
    audio_state.samples.clear();
    audio_state.source_sample_rate = 0.0;
    audio_state.output_sample_rate = 0.0;
    audio_state.resample_phase = 0.0;
    audio_state.current_frame = None;
    audio_state.next_frame = None;
}

fn hardware_render_frontend_available() -> bool {
    with_active_runtime(hardware_render_frontend_available_for).unwrap_or(false)
}

fn hardware_render_frontend_available_for(runtime: &HostRuntime) -> bool {
    runtime
        .video_coordinator
        .lock()
        .frontend_capabilities()
        .gl_context
        .is_some()
}

fn vulkan_frontend_probe_available_for(runtime: &HostRuntime) -> bool {
    runtime
        .video_coordinator
        .lock()
        .frontend_capabilities()
        .has_windowing_probe()
}

fn hardware_render_preflight_available_for_core(runtime: &HostRuntime, core_name: &str) -> bool {
    let selection = runtime
        .video_coordinator
        .lock()
        .current_selection()
        .cloned();
    if let Some(selection) = selection {
        return selection.chosen != VideoBackendKind::Software
            || core_has_embedded_software_video_fallback(core_name);
    }

    if hardware_render_frontend_available_for(runtime) {
        return true;
    }

    core_name == "parallel_n64" && vulkan_frontend_probe_available_for(runtime)
}

fn hardware_render_requested() -> bool {
    with_active_runtime(|runtime| runtime.hw_render_state.lock().callbacks.is_some())
        .unwrap_or(false)
}

fn hardware_render_context_ready() -> bool {
    with_active_runtime(|runtime| runtime.hw_render_state.lock().context_ready).unwrap_or(false)
}

fn hw_render_target_size(max_size: (u32, u32), base_size: (u32, u32)) -> (u32, u32) {
    let width = max_size.0.max(base_size.0).max(1);
    let height = max_size.1.max(base_size.1).max(1);
    (width, height)
}

fn apply_runtime_geometry_update(runtime: &HostRuntime, geometry: RetroGameGeometry) {
    let base_size = (geometry.base_width.max(1), geometry.base_height.max(1));
    let max_size = (
        geometry.max_width.max(base_size.0),
        geometry.max_height.max(base_size.1),
    );
    let target_size = hw_render_target_size(max_size, base_size);
    runtime.hw_render_state.lock().vulkan_fallback_frame_size = Some(target_size);

    if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "updated runtime geometry base={}x{} max={}x{} fallback={}x{} aspect={:.4}",
            base_size.0,
            base_size.1,
            max_size.0,
            max_size.1,
            target_size.0,
            target_size.1,
            geometry.aspect_ratio
        );
    }
}

fn resolve_vulkan_frame_size(runtime: &HostRuntime, pending: PendingHardwareFrame) -> (u32, u32) {
    if pending.width > 1 && pending.height > 1 {
        return (pending.width, pending.height);
    }

    runtime
        .hw_render_state
        .lock()
        .vulkan_fallback_frame_size
        .filter(|(width, height)| *width > 1 && *height > 1)
        .unwrap_or((pending.width.max(1), pending.height.max(1)))
}

fn hw_context_type_supported(context_type: u32) -> bool {
    const RETRO_HW_CONTEXT_OPENGL: u32 = 1;
    const RETRO_HW_CONTEXT_OPENGL_CORE: u32 = 3;

    matches!(
        context_type,
        RETRO_HW_CONTEXT_OPENGL | RETRO_HW_CONTEXT_OPENGL_CORE
    )
}

fn hw_context_type_name(context_type: u32) -> &'static str {
    match context_type {
        1 => "OpenGL",
        2 => "OpenGLES2",
        3 => "OpenGLCore",
        4 => "OpenGLES3",
        5 => "OpenGLESVersion",
        6 => "Vulkan",
        _ => "Unknown",
    }
}

fn hw_render_interface_type_name(interface_type: u32) -> &'static str {
    match interface_type {
        0 => "Vulkan",
        _ => "Unknown",
    }
}

#[cfg(test)]
fn frontend_windowing_summary_for(runtime: &HostRuntime) -> Option<String> {
    let coordinator = runtime.video_coordinator.lock();
    let capabilities = coordinator.frontend_capabilities();
    let renderer = capabilities.renderer_name.as_deref();
    let window = capabilities.window_handle_kind.as_deref();
    let display = capabilities.display_handle_kind.as_deref();

    if renderer.is_none() && window.is_none() && display.is_none() {
        return None;
    }

    let mut parts = Vec::new();
    if let Some(renderer) = renderer {
        parts.push(format!("renderer={renderer}"));
    }
    if let Some(window) = window {
        parts.push(format!("window_handle={window}"));
    }
    if let Some(display) = display {
        parts.push(format!("display_handle={display}"));
    }

    Some(parts.join(", "))
}

#[cfg(target_os = "linux")]
impl ExternalVulkanWindow {
    fn create() -> std::result::Result<Self, String> {
        let xlib = xlib::Xlib::open().map_err(|err| format!("failed to load Xlib: {err}"))?;
        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return Err(String::from(
                "failed to open X11 display for external Vulkan window",
            ));
        }

        let screen = unsafe { (xlib.XDefaultScreen)(display) };
        let root = unsafe { (xlib.XRootWindow)(display, screen) };
        let black = unsafe { (xlib.XBlackPixel)(display, screen) };
        let white = unsafe { (xlib.XWhitePixel)(display, screen) };
        let width = unsafe { (xlib.XDisplayWidth)(display, screen).max(1) as u32 };
        let height = unsafe { (xlib.XDisplayHeight)(display, screen).max(1) as u32 };
        let window = unsafe {
            (xlib.XCreateSimpleWindow)(display, root, 0, 0, width, height, 0, black, white)
        };
        if window == 0 {
            unsafe {
                (xlib.XCloseDisplay)(display);
            }
            return Err(String::from(
                "failed to create X11 window for Vulkan presentation",
            ));
        }

        unsafe {
            (xlib.XSelectInput)(
                display,
                window,
                xlib::ExposureMask | xlib::StructureNotifyMask,
            );
        }
        let title = CString::new("Personal Arcade N64 (Vulkan)")
            .map_err(|_| String::from("failed to build X11 window title"))?;
        unsafe {
            (xlib.XStoreName)(display, window, title.as_ptr());
        }
        let overlay_window =
            unsafe { (xlib.XCreateSimpleWindow)(display, window, 0, 0, 320, 36, 0, white, black) };
        if overlay_window == 0 {
            unsafe {
                (xlib.XDestroyWindow)(display, window);
                (xlib.XCloseDisplay)(display);
            }
            return Err(String::from(
                "failed to create X11 overlay window for Vulkan presentation",
            ));
        }
        let overlay_gc =
            unsafe { (xlib.XCreateGC)(display, overlay_window, 0, std::ptr::null_mut()) };
        if overlay_gc.is_null() {
            unsafe {
                (xlib.XDestroyWindow)(display, overlay_window);
                (xlib.XDestroyWindow)(display, window);
                (xlib.XCloseDisplay)(display);
            }
            return Err(String::from(
                "failed to create X11 overlay graphics context",
            ));
        }
        unsafe {
            (xlib.XSetForeground)(display, overlay_gc, white);
            (xlib.XSetBackground)(display, overlay_gc, black);
            (xlib.XUnmapWindow)(display, overlay_window);
        }
        let wm_delete_name =
            CString::new("WM_DELETE_WINDOW").map_err(|_| String::from("invalid WM_DELETE atom"))?;
        let wm_delete = unsafe { (xlib.XInternAtom)(display, wm_delete_name.as_ptr(), 0) };
        let net_wm_state_name = CString::new("_NET_WM_STATE")
            .map_err(|_| String::from("invalid _NET_WM_STATE atom"))?;
        let net_wm_state = unsafe { (xlib.XInternAtom)(display, net_wm_state_name.as_ptr(), 0) };
        let net_wm_state_fullscreen_name = CString::new("_NET_WM_STATE_FULLSCREEN")
            .map_err(|_| String::from("invalid _NET_WM_STATE_FULLSCREEN atom"))?;
        let net_wm_state_fullscreen =
            unsafe { (xlib.XInternAtom)(display, net_wm_state_fullscreen_name.as_ptr(), 0) };
        let net_wm_state_above_name = CString::new("_NET_WM_STATE_ABOVE")
            .map_err(|_| String::from("invalid _NET_WM_STATE_ABOVE atom"))?;
        let net_wm_state_above =
            unsafe { (xlib.XInternAtom)(display, net_wm_state_above_name.as_ptr(), 0) };
        let net_wm_bypass_compositor_name = CString::new("_NET_WM_BYPASS_COMPOSITOR")
            .map_err(|_| String::from("invalid _NET_WM_BYPASS_COMPOSITOR atom"))?;
        let net_wm_bypass_compositor =
            unsafe { (xlib.XInternAtom)(display, net_wm_bypass_compositor_name.as_ptr(), 0) };
        if wm_delete != 0 {
            let mut atom = wm_delete;
            unsafe {
                (xlib.XSetWMProtocols)(display, window, &mut atom, 1);
            }
        }
        let mut external_window = Self {
            xlib,
            display,
            window,
            overlay_window,
            overlay_gc,
            wm_delete,
            net_wm_state,
            net_wm_state_fullscreen,
            net_wm_state_above,
            net_wm_bypass_compositor,
            width,
            height,
            visible: false,
        };
        external_window.configure_fullscreen();

        Ok(external_window)
    }

    fn descriptor(&self) -> ExternalVulkanWindowDescriptor {
        ExternalVulkanWindowDescriptor::Xlib {
            display: self.display,
            window: self.window,
            width: self.width,
            height: self.height,
        }
    }

    fn set_overlay_message(&mut self, message: Option<&str>) {
        if self.overlay_window == 0 {
            return;
        }

        let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) else {
            unsafe {
                (self.xlib.XUnmapWindow)(self.display, self.overlay_window);
                (self.xlib.XFlush)(self.display);
            }
            return;
        };

        let max_chars = 72usize;
        let sanitized = message
            .chars()
            .filter(|ch| *ch != '\0')
            .take(max_chars)
            .collect::<String>();
        let Ok(text) = CString::new(sanitized.clone()) else {
            return;
        };
        let overlay_width = ((sanitized.chars().count() as u32).saturating_mul(9) + 32)
            .clamp(220, self.width.saturating_sub(24).max(220));
        let overlay_height = 36u32;
        let overlay_x = ((self.width.saturating_sub(overlay_width)) / 2) as i32;
        let overlay_y = self
            .height
            .saturating_sub(overlay_height.saturating_add(28)) as i32;

        unsafe {
            (self.xlib.XMoveResizeWindow)(
                self.display,
                self.overlay_window,
                overlay_x,
                overlay_y,
                overlay_width,
                overlay_height,
            );
            (self.xlib.XMapRaised)(self.display, self.overlay_window);
            (self.xlib.XClearWindow)(self.display, self.overlay_window);
            (self.xlib.XDrawString)(
                self.display,
                self.overlay_window,
                self.overlay_gc,
                14,
                23,
                text.as_ptr(),
                sanitized.len() as i32,
            );
            (self.xlib.XFlush)(self.display);
        }
    }

    fn configure_fullscreen(&mut self) {
        if self.net_wm_state != 0 {
            let mut state_atoms = [0 as xlib::Atom; 2];
            let mut state_count = 0;
            if self.net_wm_state_fullscreen != 0 {
                state_atoms[state_count] = self.net_wm_state_fullscreen;
                state_count += 1;
            }
            if self.net_wm_state_above != 0 {
                state_atoms[state_count] = self.net_wm_state_above;
                state_count += 1;
            }
            if state_count > 0 {
                unsafe {
                    (self.xlib.XChangeProperty)(
                        self.display,
                        self.window,
                        self.net_wm_state,
                        xlib::XA_ATOM,
                        32,
                        xlib::PropModeReplace,
                        state_atoms.as_ptr().cast(),
                        state_count as i32,
                    );
                }
            }
        }
        if self.net_wm_bypass_compositor != 0 {
            let bypass = [1_u64];
            unsafe {
                (self.xlib.XChangeProperty)(
                    self.display,
                    self.window,
                    self.net_wm_bypass_compositor,
                    xlib::XA_CARDINAL,
                    32,
                    xlib::PropModeReplace,
                    bypass.as_ptr().cast(),
                    1,
                );
            }
        }

        unsafe {
            (self.xlib.XMoveResizeWindow)(self.display, self.window, 0, 0, self.width, self.height);
            (self.xlib.XFlush)(self.display);
        }
    }

    fn show_fullscreen(&mut self) {
        self.configure_fullscreen();
        unsafe {
            (self.xlib.XMapRaised)(self.display, self.window);
            (self.xlib.XRaiseWindow)(self.display, self.window);
            (self.xlib.XFlush)(self.display);
        }
    }

    fn pump_events(&mut self) {
        loop {
            let pending = unsafe { (self.xlib.XPending)(self.display) };
            if pending <= 0 {
                break;
            }
            let mut event = std::mem::MaybeUninit::<xlib::XEvent>::uninit();
            unsafe {
                (self.xlib.XNextEvent)(self.display, event.as_mut_ptr());
            }
            let event = unsafe { event.assume_init() };
            match event.get_type() {
                xlib::ConfigureNotify => {
                    let configure = unsafe { event.configure };
                    self.width = configure.width.max(1) as u32;
                    self.height = configure.height.max(1) as u32;
                }
                xlib::ClientMessage => {
                    let client = unsafe { event.client_message };
                    let data = client.data.as_longs();
                    if self.wm_delete != 0 && data.first().copied() == Some(self.wm_delete as i64) {
                        self.show_fullscreen();
                    }
                }
                _ => {}
            }
        }
    }

    fn set_visible(&mut self, visible: bool) {
        if visible {
            if self.visible {
                return;
            }
            self.show_fullscreen();
            self.visible = true;
        } else {
            if !self.visible {
                return;
            }
            unsafe {
                (self.xlib.XUnmapWindow)(self.display, self.window);
                (self.xlib.XUnmapWindow)(self.display, self.overlay_window);
                (self.xlib.XFlush)(self.display);
            }
            self.visible = false;
        }
    }
}

#[cfg(target_os = "windows")]
const EXTERNAL_VULKAN_WINDOW_CLASS_NAME: &[u8] = b"PersonalArcadeExternalVulkanWindow\0";

#[cfg(target_os = "windows")]
const EXTERNAL_VULKAN_OVERLAY_CLASS_NAME: &[u8] = b"STATIC\0";

#[cfg(target_os = "windows")]
fn external_vulkan_window_size(hwnd: HWND) -> (u32, u32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetClientRect(hwnd, &mut rect) } != 0 {
        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;
        (width, height)
    } else {
        (1, 1)
    }
}

#[cfg(target_os = "windows")]
fn pump_external_vulkan_window_messages(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }

    let mut message = unsafe { std::mem::zeroed::<MSG>() };
    while unsafe { PeekMessageA(&mut message, hwnd, 0, 0, PM_REMOVE) } != 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageA(&message);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn external_vulkan_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLOSE => {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
            0
        }
        _ => unsafe { DefWindowProcA(hwnd, message, wparam, lparam) },
    }
}

#[cfg(target_os = "windows")]
fn register_external_vulkan_window_class(hinstance: HINSTANCE) -> std::result::Result<(), String> {
    let class = WNDCLASSEXA {
        cbSize: std::mem::size_of::<WNDCLASSEXA>() as u32,
        style: 0,
        lpfnWndProc: Some(external_vulkan_window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: EXTERNAL_VULKAN_WINDOW_CLASS_NAME.as_ptr(),
        hIconSm: std::ptr::null_mut(),
    };
    let atom = unsafe { RegisterClassExA(&class) };
    if atom == 0 {
        let error = unsafe { GetLastError() };
        if error != ERROR_CLASS_ALREADY_EXISTS {
            return Err(format!(
                "failed to register Win32 window class for Vulkan presentation (GetLastError={error})"
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
impl ExternalVulkanWindow {
    fn create() -> std::result::Result<Self, String> {
        let hinstance = unsafe { GetModuleHandleA(std::ptr::null()) };
        if hinstance.is_null() {
            let error = unsafe { GetLastError() };
            return Err(format!(
                "failed to query Win32 module handle for Vulkan presentation window (GetLastError={error})"
            ));
        }
        register_external_vulkan_window_class(hinstance)?;

        let title = CString::new("Personal Arcade N64 (Vulkan)")
            .map_err(|_| String::from("failed to build Win32 window title"))?;
        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1) as u32;
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(1) as u32;
        let hwnd = unsafe {
            CreateWindowExA(
                WS_EX_TOPMOST,
                EXTERNAL_VULKAN_WINDOW_CLASS_NAME.as_ptr(),
                title.as_ptr().cast(),
                WS_POPUP,
                0,
                0,
                width as i32,
                height as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            let error = unsafe { GetLastError() };
            return Err(format!(
                "failed to create Win32 window for Vulkan presentation (GetLastError={error})"
            ));
        }

        let overlay_hwnd = unsafe {
            CreateWindowExA(
                0,
                EXTERNAL_VULKAN_OVERLAY_CLASS_NAME.as_ptr(),
                std::ptr::null(),
                WS_CHILD,
                0,
                0,
                300,
                36,
                hwnd,
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            )
        };
        if overlay_hwnd.is_null() {
            unsafe {
                DestroyWindow(hwnd);
            }
            let error = unsafe { GetLastError() };
            return Err(format!(
                "failed to create Win32 overlay window for Vulkan presentation (GetLastError={error})"
            ));
        }
        unsafe {
            ShowWindow(overlay_hwnd, SW_HIDE);
            ShowWindow(hwnd, SW_HIDE);
        }

        Ok(Self {
            hinstance,
            hwnd,
            overlay_hwnd,
            width,
            height,
            visible: false,
        })
    }

    fn descriptor(&self) -> ExternalVulkanWindowDescriptor {
        ExternalVulkanWindowDescriptor::Win32 {
            hwnd: self.hwnd as vk::HWND,
            hinstance: self.hinstance as vk::HINSTANCE,
            width: self.width,
            height: self.height,
        }
    }

    fn set_overlay_message(&mut self, message: Option<&str>) {
        if self.overlay_hwnd.is_null() {
            return;
        }
        let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) else {
            unsafe {
                ShowWindow(self.overlay_hwnd, SW_HIDE);
            }
            return;
        };

        let max_chars = 72usize;
        let sanitized = message
            .chars()
            .filter(|ch| *ch != '\0')
            .take(max_chars)
            .collect::<String>();
        let Ok(text) = CString::new(sanitized) else {
            return;
        };
        let overlay_width = ((text.as_bytes().len() as u32).saturating_mul(9) + 32)
            .clamp(220, self.width.saturating_sub(24).max(220));
        let overlay_height = 36u32;
        let overlay_x = ((self.width.saturating_sub(overlay_width)) / 2) as i32;
        let overlay_y = self
            .height
            .saturating_sub(overlay_height.saturating_add(28)) as i32;

        unsafe {
            MoveWindow(
                self.overlay_hwnd,
                overlay_x,
                overlay_y,
                overlay_width as i32,
                overlay_height as i32,
                1,
            );
            SetWindowTextA(self.overlay_hwnd, text.as_ptr().cast());
            ShowWindow(self.overlay_hwnd, SW_SHOW);
        }
    }

    fn show_fullscreen(&mut self) {
        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1);
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(1);
        unsafe {
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, width, height, SWP_SHOWWINDOW);
            ShowWindow(self.hwnd, SW_SHOW);
            ShowWindow(self.hwnd, SW_SHOWMAXIMIZED);
        }
        (self.width, self.height) = external_vulkan_window_size(self.hwnd);
        if vulkan_debug_enabled() {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "showing external Vulkan window {}x{}",
                self.width,
                self.height
            );
        }
    }

    fn pump_events(&mut self) {
        pump_external_vulkan_window_messages(self.hwnd);
        pump_external_vulkan_window_messages(self.overlay_hwnd);
        (self.width, self.height) = external_vulkan_window_size(self.hwnd);
    }

    fn set_visible(&mut self, visible: bool) {
        if visible {
            if self.visible {
                return;
            }
            self.show_fullscreen();
            self.visible = true;
        } else {
            if !self.visible {
                return;
            }
            unsafe {
                ShowWindow(self.overlay_hwnd, SW_HIDE);
                ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl ExternalVulkanWindow {
    fn create() -> std::result::Result<Self, String> {
        Err(String::from(
            "external Vulkan presentation window is only implemented on Linux/X11 and Windows/Win32",
        ))
    }

    fn descriptor(&self) -> ExternalVulkanWindowDescriptor {
        ExternalVulkanWindowDescriptor::Unsupported {
            width: 1,
            height: 1,
        }
    }

    fn set_overlay_message(&mut self, _message: Option<&str>) {}

    fn pump_events(&mut self) {}

    fn set_visible(&mut self, _visible: bool) {}
}

fn destroy_external_vulkan_window(window: ExternalVulkanWindow) {
    #[cfg(target_os = "linux")]
    unsafe {
        if !window.overlay_gc.is_null() {
            (window.xlib.XFreeGC)(window.display, window.overlay_gc);
        }
        if window.overlay_window != 0 {
            (window.xlib.XDestroyWindow)(window.display, window.overlay_window);
        }
        (window.xlib.XDestroyWindow)(window.display, window.window);
        (window.xlib.XCloseDisplay)(window.display);
    }
    #[cfg(target_os = "windows")]
    unsafe {
        if !window.overlay_hwnd.is_null() {
            DestroyWindow(window.overlay_hwnd);
        }
        if !window.hwnd.is_null() {
            DestroyWindow(window.hwnd);
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = window;
}

fn frontend_supports_external_vulkan_window(state: &HardwareRenderState) -> bool {
    let _ = state;
    let Some(runtime) = active_runtime() else {
        return false;
    };
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let coordinator = runtime.video_coordinator.lock();
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let capabilities = coordinator.frontend_capabilities().clone();
    #[cfg(target_os = "linux")]
    {
        capabilities
            .window_handle_kind
            .as_deref()
            .is_some_and(|kind| kind == "Xlib")
            && capabilities
                .display_handle_kind
                .as_deref()
                .is_some_and(|kind| kind == "Xlib")
    }
    #[cfg(target_os = "windows")]
    {
        if std::env::var_os("ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT").is_none() {
            return false;
        }
        capabilities
            .window_handle_kind
            .as_deref()
            .is_some_and(|kind| kind == "Win32")
            && capabilities
                .display_handle_kind
                .as_deref()
                .is_some_and(|kind| kind == "Windows")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = runtime;
        false
    }
}

fn vulkan_debug_enabled() -> bool {
    std::env::var_os("ARCADE_VULKAN_DEBUG").is_some()
}

fn vulkan_force_fallback_idle() -> bool {
    match std::env::var("ARCADE_VULKAN_FORCE_FALLBACK_IDLE") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            !matches!(normalized.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

fn ensure_external_vulkan_window_for(
    runtime: &HostRuntime,
) -> Option<ExternalVulkanWindowDescriptor> {
    let mut state = runtime.hw_render_state.lock();
    let should_use_external = frontend_supports_external_vulkan_window(&state);
    if !should_use_external {
        if vulkan_debug_enabled() && !state.external_vulkan_probe_logged {
            let capabilities = runtime
                .video_coordinator
                .lock()
                .frontend_capabilities()
                .clone();
            info!(
                target: "arcade_libretro::vulkan_debug",
                "external Vulkan window disabled renderer={:?} window_handle={:?} display_handle={:?} env_win32_external_present={}",
                capabilities.renderer_name,
                capabilities.window_handle_kind,
                capabilities.display_handle_kind,
                std::env::var_os("ARCADE_WINDOWS_EXTERNAL_VULKAN_PRESENT").is_some()
            );
            state.external_vulkan_probe_logged = true;
        }
        return None;
    }

    if state.external_vulkan_window.is_none() {
        match ExternalVulkanWindow::create() {
            Ok(window) => {
                if vulkan_debug_enabled() {
                    let descriptor = window.descriptor();
                    let (width, height) = descriptor.size();
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "created external Vulkan window {}x{}",
                        width,
                        height
                    );
                }
                state.external_vulkan_window = Some(window);
            }
            Err(err) => {
                warn!("libretro hw-render: external Vulkan window unavailable: {err}");
                return None;
            }
        }
    }
    state
        .external_vulkan_window
        .as_ref()
        .map(ExternalVulkanWindow::descriptor)
}

fn pump_external_vulkan_window_events(runtime: &HostRuntime) {
    let mut state = runtime.hw_render_state.lock();
    if let Some(window) = state.external_vulkan_window.as_mut() {
        window.pump_events();
    }
}

fn update_external_vulkan_present_state(runtime: &HostRuntime, active: bool) {
    let mut state = runtime.hw_render_state.lock();
    state.external_vulkan_present_active = active;
    if let Some(window) = state.external_vulkan_window.as_mut() {
        window.set_visible(active);
    }
}

fn build_vulkan_instance_extensions(
    entry: &ash::Entry,
    external_surface: bool,
) -> std::result::Result<Vec<*const i8>, String> {
    fn push_extension_if_available(
        enabled_extensions: &mut Vec<&'static CStr>,
        available_names: &HashSet<Vec<u8>>,
        extension_name: &'static CStr,
    ) {
        if available_names.contains(extension_name.to_bytes())
            && !enabled_extensions
                .iter()
                .any(|enabled| enabled.to_bytes() == extension_name.to_bytes())
        {
            enabled_extensions.push(extension_name);
        }
    }

    let available_extensions = unsafe { entry.enumerate_instance_extension_properties(None) }
        .map_err(|err| format!("failed to enumerate Vulkan instance extensions: {err:?}"))?;
    let available_names: HashSet<Vec<u8>> = available_extensions
        .iter()
        .map(|properties| unsafe { CStr::from_ptr(properties.extension_name.as_ptr()) })
        .map(|name| name.to_bytes().to_vec())
        .collect();

    let mut enabled_extensions: Vec<&'static CStr> = Vec::new();

    if external_surface {
        enabled_extensions.push(ash::vk::KHR_SURFACE_NAME);
        #[cfg(target_os = "linux")]
        enabled_extensions.push(ash::vk::KHR_XLIB_SURFACE_NAME);
        #[cfg(target_os = "windows")]
        enabled_extensions.push(ash::vk::KHR_WIN32_SURFACE_NAME);
    }

    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_EXTERNAL_MEMORY_CAPABILITIES_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_EXTERNAL_SEMAPHORE_CAPABILITIES_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_EXTERNAL_FENCE_CAPABILITIES_NAME,
    );
    push_extension_if_available(
        &mut enabled_extensions,
        &available_names,
        ash::vk::KHR_PORTABILITY_ENUMERATION_NAME,
    );
    if vulkan_debug_enabled() {
        push_extension_if_available(
            &mut enabled_extensions,
            &available_names,
            ash::vk::EXT_DEBUG_UTILS_NAME,
        );
        let enabled_names: Vec<String> = enabled_extensions
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        info!(
            target: "arcade_libretro::vulkan_debug",
            "enabling Vulkan instance extensions: {}",
            enabled_names.join(", ")
        );
    }

    Ok(enabled_extensions
        .into_iter()
        .map(|extension_name| extension_name.as_ptr())
        .collect())
}

#[cfg(target_os = "macos")]
fn macos_vulkan_loader_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os("ARCADE_VULKAN_LOADER") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(vulkan_sdk) = std::env::var_os("VULKAN_SDK") {
        let sdk = PathBuf::from(vulkan_sdk);
        candidates.push(sdk.join("lib").join("libvulkan.dylib"));
        candidates.push(sdk.join("macOS").join("lib").join("libvulkan.dylib"));
        candidates.push(sdk.join("macOS").join("lib").join("libMoltenVK.dylib"));
    }
    for path in [
        "/opt/homebrew/lib/libvulkan.dylib",
        "/opt/homebrew/lib/libvulkan.1.dylib",
        "/opt/homebrew/lib/libMoltenVK.dylib",
        "/usr/local/lib/libvulkan.dylib",
        "/usr/local/lib/libvulkan.1.dylib",
        "/usr/local/lib/libMoltenVK.dylib",
        "/usr/local/lib/libMoltenVK_all.dylib",
    ] {
        candidates.push(PathBuf::from(path));
    }

    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        if seen.insert(candidate.clone()) {
            unique.push(candidate);
        }
    }
    unique
}

fn load_vulkan_entry() -> std::result::Result<ash::Entry, String> {
    #[cfg(target_os = "macos")]
    {
        let mut attempted_paths = vec![PathBuf::from("libvulkan.dylib")];
        let mut last_error = match unsafe { ash::Entry::load() } {
            Ok(entry) => return Ok(entry),
            Err(err) => err.to_string(),
        };

        for candidate in macos_vulkan_loader_candidates() {
            if attempted_paths.iter().any(|existing| existing == &candidate) {
                continue;
            }
            attempted_paths.push(candidate.clone());
            match unsafe { ash::Entry::load_from(&candidate) } {
                Ok(entry) => {
                    if vulkan_debug_enabled() {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "loaded Vulkan entry points from {}",
                            candidate.display()
                        );
                    }
                    return Ok(entry);
                }
                Err(err) => last_error = err.to_string(),
            }
        }

        let attempted = attempted_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut message = format!(
            "failed to load Vulkan entry points on macOS. Tried: {attempted}. \
Install Vulkan loader + MoltenVK (Homebrew: `brew install vulkan-loader molten-vk`) and ensure \
`libvulkan.dylib` or `libMoltenVK.dylib` is discoverable \
(for example `DYLD_FALLBACK_LIBRARY_PATH=/opt/homebrew/lib:/usr/local/lib`)."
        );
        if vulkan_debug_enabled() {
            message.push_str(&format!(" Last loader error: {last_error}"));
        }
        Err(message)
    }

    #[cfg(not(target_os = "macos"))]
    {
        unsafe { ash::Entry::load() }
            .map_err(|err| format!("failed to load Vulkan entry points: {err}"))
    }
}

impl VulkanInterfaceState {
    fn create(
        runtime: &HostRuntime,
        negotiation: Option<VulkanNegotiationCallbacks>,
        external_window: Option<ExternalVulkanWindowDescriptor>,
    ) -> std::result::Result<Self, String> {
        let app_name = CString::new("Personal Arcade Native")
            .map_err(|_| String::from("failed to build Vulkan app name"))?;
        let engine_name = CString::new("arcade-libretro")
            .map_err(|_| String::from("failed to build Vulkan engine name"))?;

        let entry = load_vulkan_entry()?;

        let default_app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(0)
            .engine_name(&engine_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_0);
        let app_info = if let Some(get_application_info) =
            negotiation.and_then(|callbacks| callbacks.get_application_info)
        {
            let raw_app_info = unsafe { get_application_info() };
            if raw_app_info.is_null() {
                &default_app_info
            } else {
                unsafe { &*(raw_app_info as *const vk::ApplicationInfo<'static>) }
            }
        } else {
            &default_app_info
        };
        if vulkan_debug_enabled() {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "creating Vulkan interface negotiation={} app_api_version=0x{:x} external_surface={}",
                negotiation.is_some(),
                app_info.api_version,
                external_window.is_some()
            );
            let loader_version = unsafe { entry.try_enumerate_instance_version() }
                .ok()
                .flatten()
                .unwrap_or(vk::API_VERSION_1_0);
            info!(
                target: "arcade_libretro::vulkan_debug",
                "Vulkan loader instance version=0x{:x}",
                loader_version
            );
        }
        let instance_extensions =
            build_vulkan_instance_extensions(&entry, external_window.is_some())?;
        let portability_enumeration_enabled = instance_extensions
            .iter()
            .any(|&name| name == ash::vk::KHR_PORTABILITY_ENUMERATION_NAME.as_ptr());
        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(app_info)
            .enabled_extension_names(&instance_extensions);
        #[cfg(target_os = "macos")]
        if portability_enumeration_enabled {
            create_info =
                create_info.flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
        }

        let instance = match unsafe { entry.create_instance(&create_info, None) } {
            Ok(instance) => instance,
            Err(err) => {
                #[cfg(target_os = "macos")]
                {
                    // MoltenVK setups can report incompatible driver for higher API versions
                    // requested by cores; retry with Vulkan 1.0 to maximize compatibility.
                    if err == vk::Result::ERROR_INCOMPATIBLE_DRIVER
                        && app_info.api_version > vk::API_VERSION_1_0
                    {
                        warn!(
                            "Vulkan instance creation failed with {:?} at api_version=0x{:x}; retrying with Vulkan 1.0",
                            err,
                            app_info.api_version
                        );
                        let fallback_app_info =
                            default_app_info.api_version(vk::API_VERSION_1_0);
                        let mut fallback_create_info = vk::InstanceCreateInfo::default()
                            .application_info(&fallback_app_info)
                            .enabled_extension_names(&instance_extensions);
                        if portability_enumeration_enabled {
                            fallback_create_info = fallback_create_info
                                .flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
                        }
                        if let Ok(instance) =
                            unsafe { entry.create_instance(&fallback_create_info, None) }
                        {
                            if vulkan_debug_enabled() {
                                info!(
                                    target: "arcade_libretro::vulkan_debug",
                                    "Vulkan instance retry succeeded with api_version=0x{:x}",
                                    vk::API_VERSION_1_0
                                );
                            }
                            instance
                        } else {
                            return Err(format!("failed to create Vulkan instance: {err:?}"));
                        }
                    } else {
                        return Err(format!("failed to create Vulkan instance: {err:?}"));
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    return Err(format!("failed to create Vulkan instance: {err:?}"));
                }
            }
        };

        let surface = external_window
            .map(|external_window| {
                create_external_vulkan_surface(&entry, &instance, external_window)
            })
            .transpose()?;

        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|err| format!("failed to enumerate Vulkan physical devices: {err:?}"))?;
        let mut physical_device = *physical_devices
            .first()
            .ok_or_else(|| String::from("no Vulkan physical devices were reported"))?;

        let mut destroy_device_callback = None;
        let (
            device,
            queue,
            queue_family_index,
            presentation_queue,
            presentation_queue_family_index,
        ) = if let Some(create_device) = negotiation.and_then(|callbacks| callbacks.create_device) {
            let required_features = vk::PhysicalDeviceFeatures::default();
            let required_device_extensions = [ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()];
            let mut frontend_context = RetroVulkanContext {
                gpu: vk::PhysicalDevice::null(),
                device: vk::Device::null(),
                queue: vk::Queue::null(),
                queue_family_index: 0,
                presentation_queue: vk::Queue::null(),
                presentation_queue_family_index: 0,
            };
            let created = unsafe {
                create_device(
                    &mut frontend_context,
                    instance.handle(),
                    physical_device,
                    surface.unwrap_or(vk::SurfaceKHR::null()),
                    entry.static_fn().get_instance_proc_addr,
                    required_device_extensions.as_ptr(),
                    required_device_extensions.len() as u32,
                    std::ptr::null(),
                    0,
                    &required_features,
                )
            };
            if vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "core Vulkan create_device returned={} surface={} required_extensions={}",
                    created,
                    surface.is_some(),
                    required_device_extensions.len()
                );
            }

            if created {
                if frontend_context.gpu == vk::PhysicalDevice::null()
                    || frontend_context.device == vk::Device::null()
                    || frontend_context.queue == vk::Queue::null()
                {
                    unsafe {
                        instance.destroy_instance(None);
                    }
                    return Err(String::from(
                        "core Vulkan create_device reported success without a valid GPU/device/queue",
                    ));
                }

                physical_device = frontend_context.gpu;
                destroy_device_callback =
                    negotiation.and_then(|callbacks| callbacks.destroy_device);
                let device =
                    unsafe { ash::Device::load(instance.fp_v1_0(), frontend_context.device) };
                (
                    device,
                    frontend_context.queue,
                    frontend_context.queue_family_index,
                    if frontend_context.presentation_queue == vk::Queue::null() {
                        frontend_context.queue
                    } else {
                        frontend_context.presentation_queue
                    },
                    if frontend_context.presentation_queue == vk::Queue::null() {
                        frontend_context.queue_family_index
                    } else {
                        frontend_context.presentation_queue_family_index
                    },
                )
            } else {
                let queue_family_index =
                    choose_vulkan_queue_family_index(&instance, physical_device)?;

                let queue_priorities = [1.0_f32];
                let queue_infos = [vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(queue_family_index)
                    .queue_priorities(&queue_priorities)];
                let device_extensions = surface
                    .is_some()
                    .then_some(vec![ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()])
                    .unwrap_or_default();
                let device_create_info = vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queue_infos)
                    .enabled_extension_names(&device_extensions);

                let device =
                    unsafe { instance.create_device(physical_device, &device_create_info, None) }
                        .map_err(|err| format!("failed to create Vulkan logical device: {err:?}"))?;
                let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
                (device, queue, queue_family_index, queue, queue_family_index)
            }
        } else {
            let queue_family_index = choose_vulkan_queue_family_index(&instance, physical_device)?;

            let queue_priorities = [1.0_f32];
            let queue_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&queue_priorities)];
            let device_extensions = surface
                .is_some()
                .then_some(vec![ash::vk::KHR_SWAPCHAIN_NAME.as_ptr()])
                .unwrap_or_default();
            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_infos)
                .enabled_extension_names(&device_extensions);

            let device =
                unsafe { instance.create_device(physical_device, &device_create_info, None) }
                    .map_err(|err| format!("failed to create Vulkan logical device: {err:?}"))?;
            let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
            (device, queue, queue_family_index, queue, queue_family_index)
        };

        if vulkan_debug_enabled() {
            let queue_properties =
                unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
            let queue_flags = queue_properties
                .get(queue_family_index as usize)
                .map(|properties| properties.queue_flags)
                .unwrap_or(vk::QueueFlags::empty());
            let present_queue_flags = queue_properties
                .get(presentation_queue_family_index as usize)
                .map(|properties| properties.queue_flags)
                .unwrap_or(vk::QueueFlags::empty());
            info!(
                target: "arcade_libretro::vulkan_debug",
                "Vulkan device selection queue_family={} queue_flags={:?} present_queue_family={} present_queue_flags={:?}",
                queue_family_index,
                queue_flags,
                presentation_queue_family_index,
                present_queue_flags
            );
        }

        let interface = RetroHwRenderInterfaceVulkan {
            interface_type: RETRO_HW_RENDER_INTERFACE_VULKAN,
            interface_version: 5,
            handle: runtime as *const HostRuntime as *mut c_void,
            instance: instance.handle(),
            gpu: physical_device,
            device: device.handle(),
            get_device_proc_addr: instance.fp_v1_0().get_device_proc_addr,
            get_instance_proc_addr: entry.static_fn().get_instance_proc_addr,
            queue,
            queue_index: queue_family_index,
            set_image: Some(retro_vulkan_set_image),
            get_sync_index: Some(retro_vulkan_get_sync_index),
            get_sync_index_mask: Some(retro_vulkan_get_sync_index_mask),
            set_command_buffers: Some(retro_vulkan_set_command_buffers),
            wait_sync_index: Some(retro_vulkan_wait_sync_index),
            lock_queue: Some(retro_vulkan_lock_queue),
            unlock_queue: Some(retro_vulkan_unlock_queue),
            set_signal_semaphore: Some(retro_vulkan_set_signal_semaphore),
        };

        let present = if let (Some(surface), Some(external_window)) = (surface, external_window) {
            match create_vulkan_present_state(VulkanPresentConfig {
                entry: &entry,
                instance: &instance,
                physical_device,
                device: &device,
                surface,
                external_window,
                present_queue: presentation_queue,
                present_queue_family_index: presentation_queue_family_index,
            }) {
                Ok(present) => Some(present),
                Err(err) => {
                    warn!("libretro hw-render: external Vulkan presentation unavailable: {err}");
                    let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
                    unsafe {
                        surface_loader.destroy_surface(surface, None);
                    }
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            _queue: queue,
            queue_family_index,
            presentation_queue,
            sync_index: present
                .as_ref()
                .map(|_| 0)
                .unwrap_or(VULKAN_FALLBACK_SYNC_FRAMES.saturating_sub(1)),
            sync_frames: present
                .as_ref()
                .map(|present| present.images.len() as u32)
                .unwrap_or(VULKAN_FALLBACK_SYNC_FRAMES)
                .max(1),
            waiting_for_core_wait_sync: false,
            wait_sync_generation: 0,
            queue_locked: false,
            destroy_device_callback,
            interface,
            pending_images: VecDeque::new(),
            present,
            readback: None,
        })
    }

    fn summary(&self) -> String {
        let properties = unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
        };
        let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        format!(
            "gpu={device_name}, queue_family={}",
            self.queue_family_index
        )
    }
}

fn prepare_vulkan_sync_for_frame(runtime: &HostRuntime) -> Result<()> {
    let mut state = runtime.hw_render_state.lock();
    let external_window = state
        .external_vulkan_window
        .as_ref()
        .map(ExternalVulkanWindow::descriptor);
    let Some(vulkan) = state.vulkan.as_mut() else {
        return Ok(());
    };
    if vulkan.present.is_none() {
        let frames = vulkan.sync_frames.max(1);
        vulkan.sync_index = (vulkan.sync_index + 1) % frames;
        vulkan.waiting_for_core_wait_sync = true;
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "prepare fallback sync index={} frames={}",
                    vulkan.sync_index,
                    frames
                );
            }
        }
        return Ok(());
    }

    let mut retried_recreate = false;
    loop {
        let Some(present) = vulkan.present.as_mut() else {
            return Ok(());
        };

        if let Some(image_index) = present.acquired_image_index {
            vulkan.sync_index = image_index;
            vulkan.sync_frames = present.images.len().max(1) as u32;
            vulkan.waiting_for_core_wait_sync = true;
            return Ok(());
        }

        let acquire_result = unsafe {
            present.swapchain_loader.acquire_next_image(
                present.swapchain,
                5_000_000_000,
                present.acquire_semaphore,
                vk::Fence::null(),
            )
        };

        match acquire_result {
            Ok((_, true))
                if !retried_recreate
                    && recreate_win32_vulkan_present_state(
                        vulkan,
                        external_window,
                        "vkAcquireNextImageKHR returned SUBOPTIMAL_KHR",
                    )? =>
            {
                retried_recreate = true;
            }
            Ok((image_index, _)) => {
                if let Some(present) = vulkan.present.as_mut() {
                    present.acquired_image_index = Some(image_index);
                    vulkan.sync_index = image_index;
                    vulkan.sync_frames = present.images.len().max(1) as u32;
                    vulkan.waiting_for_core_wait_sync = true;
                }
                return Ok(());
            }
            Err(err)
                if !retried_recreate
                    && win32_vulkan_present_requires_recreate(err)
                    && recreate_win32_vulkan_present_state(
                        vulkan,
                        external_window,
                        &format!("vkAcquireNextImageKHR returned {err:?}"),
                    )? =>
            {
                retried_recreate = true;
            }
            Err(err) => {
                return Err(anyhow!("failed to acquire Vulkan swapchain image: {err:?}"));
            }
        }
    }
}

struct VulkanPresentConfig<'a> {
    entry: &'a ash::Entry,
    instance: &'a ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &'a ash::Device,
    surface: vk::SurfaceKHR,
    external_window: ExternalVulkanWindowDescriptor,
    present_queue: vk::Queue,
    present_queue_family_index: u32,
}

fn create_vulkan_present_state(
    config: VulkanPresentConfig<'_>,
) -> std::result::Result<VulkanPresentState, String> {
    let VulkanPresentConfig {
        entry,
        instance,
        physical_device,
        device,
        surface,
        external_window,
        present_queue,
        present_queue_family_index,
    } = config;
    for function_name in [
        b"vkCreateSwapchainKHR\0".as_slice(),
        b"vkDestroySwapchainKHR\0".as_slice(),
        b"vkGetSwapchainImagesKHR\0".as_slice(),
        b"vkAcquireNextImageKHR\0".as_slice(),
        b"vkQueuePresentKHR\0".as_slice(),
    ] {
        let function_name = CStr::from_bytes_with_nul(function_name)
            .map_err(|_| String::from("invalid Vulkan swapchain entrypoint name"))?;
        let proc =
            unsafe { instance.get_device_proc_addr(device.handle(), function_name.as_ptr()) };
        if proc.is_none() {
            return Err(format!(
                "core-provided Vulkan device does not expose required swapchain entrypoint {}",
                function_name.to_string_lossy()
            ));
        }
    }

    let surface_loader = ash::khr::surface::Instance::new(entry, instance);
    let present_supported = unsafe {
        surface_loader.get_physical_device_surface_support(
            physical_device,
            present_queue_family_index,
            surface,
        )
    }
    .map_err(|err| format!("failed to query Vulkan surface present support: {err:?}"))?;
    if !present_supported {
        return Err(format!(
            "Vulkan queue family {present_queue_family_index} does not support presentation to the external surface"
        ));
    }
    let surface_capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
    }
    .map_err(|err| format!("failed to query Vulkan surface capabilities: {err:?}"))?;
    let surface_formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface) }
            .map_err(|err| format!("failed to query Vulkan surface formats: {err:?}"))?;
    let surface_format = surface_formats
        .iter()
        .find(|format| {
            matches!(
                format.format,
                vk::Format::B8G8R8A8_UNORM | vk::Format::R8G8B8A8_UNORM
            )
        })
        .copied()
        .or_else(|| surface_formats.first().copied())
        .ok_or_else(|| String::from("no Vulkan surface formats were reported"))?;
    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)
    }
    .map_err(|err| format!("failed to query Vulkan present modes: {err:?}"))?;
    let present_mode = present_modes
        .into_iter()
        .find(|mode| *mode == vk::PresentModeKHR::FIFO)
        .unwrap_or(vk::PresentModeKHR::FIFO);
    let extent = if surface_capabilities.current_extent.width != u32::MAX {
        surface_capabilities.current_extent
    } else {
        let (width, height) = external_window.size();
        vk::Extent2D {
            width: width.max(1),
            height: height.max(1),
        }
    };
    let mut image_count = surface_capabilities.min_image_count.max(2);
    if surface_capabilities.max_image_count > 0 {
        image_count = image_count.min(surface_capabilities.max_image_count);
    }
    if !surface_capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err(String::from(
            "Vulkan surface does not support COLOR_ATTACHMENT swapchain images",
        ));
    }

    let debug_swapchain_readback_supported = vulkan_debug_enabled()
        && surface_capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC);
    let mut swapchain_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
    if debug_swapchain_readback_supported {
        swapchain_usage |= vk::ImageUsageFlags::TRANSFER_SRC;
    } else if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "swapchain debug readback unavailable supported_usage={:?}",
            surface_capabilities.supported_usage_flags
        );
    }

    let swapchain_loader = ash::khr::swapchain::Device::new(instance, device);
    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(swapchain_usage)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(surface_capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true);
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan swapchain: {err:?}"))?;
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
        .map_err(|err| format!("failed to enumerate Vulkan swapchain images: {err:?}"))?;
    let render_pass = create_vulkan_present_render_pass(device, surface_format.format)?;
    let descriptor_set_layout = create_vulkan_present_descriptor_set_layout(device)?;
    let sampler = create_vulkan_present_sampler(device)?;
    let (pipeline_layout, pipeline) =
        create_vulkan_present_pipeline(device, extent, render_pass, descriptor_set_layout)?;
    let descriptor_pool_sizes = [
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::SAMPLER)
            .descriptor_count(images.len() as u32),
        vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(images.len() as u32),
    ];
    let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(&descriptor_pool_sizes)
        .max_sets(images.len() as u32);
    let descriptor_pool = unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) }
        .map_err(|err| format!("failed to create Vulkan descriptor pool: {err:?}"))?;
    let descriptor_layouts = vec![descriptor_set_layout; images.len()];
    let descriptor_set_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool)
        .set_layouts(&descriptor_layouts);
    let descriptor_sets = unsafe { device.allocate_descriptor_sets(&descriptor_set_info) }
        .map_err(|err| format!("failed to allocate Vulkan descriptor sets: {err:?}"))?;
    let debug_readback = if debug_swapchain_readback_supported {
        Some(create_vulkan_present_debug_readback(
            instance,
            physical_device,
            device,
            extent,
        )?)
    } else {
        None
    };
    let mut image_views = Vec::with_capacity(images.len());
    let mut framebuffers = Vec::with_capacity(images.len());
    for image in &images {
        let view_info = vk::ImageViewCreateInfo::default()
            .image(*image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(surface_format.format)
            .components(vk::ComponentMapping::default())
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        let image_view = unsafe { device.create_image_view(&view_info, None) }
            .map_err(|err| format!("failed to create Vulkan swapchain image view: {err:?}"))?;
        let framebuffer_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(std::slice::from_ref(&image_view))
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        let framebuffer = unsafe { device.create_framebuffer(&framebuffer_info, None) }
            .map_err(|err| format!("failed to create Vulkan framebuffer: {err:?}"))?;
        image_views.push(image_view);
        framebuffers.push(framebuffer);
    }
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let acquire_semaphore = unsafe { device.create_semaphore(&semaphore_info, None) }
        .map_err(|err| format!("failed to create Vulkan acquire semaphore: {err:?}"))?;
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let mut frames = Vec::with_capacity(images.len());
    for _ in 0..images.len() {
        let command_pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(present_queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
            .map_err(|err| format!("failed to create Vulkan present command pool: {err:?}"))?;
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = match unsafe { device.allocate_command_buffers(&alloc_info) } {
            Ok(buffers) => buffers.into_iter().next().ok_or_else(|| {
                String::from("failed to allocate a Vulkan present command buffer")
            })?,
            Err(err) => {
                unsafe {
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(format!(
                    "failed to allocate Vulkan present command buffer: {err:?}"
                ));
            }
        };
        let render_semaphore = match unsafe { device.create_semaphore(&semaphore_info, None) } {
            Ok(semaphore) => semaphore,
            Err(err) => {
                unsafe {
                    device.free_command_buffers(command_pool, &[command_buffer]);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(format!("failed to create Vulkan render semaphore: {err:?}"));
            }
        };
        let fence = match unsafe { device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(err) => {
                unsafe {
                    device.destroy_semaphore(render_semaphore, None);
                    device.free_command_buffers(command_pool, &[command_buffer]);
                    device.destroy_command_pool(command_pool, None);
                }
                return Err(format!("failed to create Vulkan present fence: {err:?}"));
            }
        };
        frames.push(VulkanPresentFrameResources {
            command_pool,
            command_buffer,
            render_semaphore,
            fence,
            transient_image_view: None,
        });
    }

    if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "created Vulkan present state extent={}x{} format={:?} images={} queue_family={} present_queue_family={}",
            extent.width,
            extent.height,
            surface_format.format,
            images.len(),
            present_queue_family_index,
            present_queue_family_index
        );
    }

    Ok(VulkanPresentState {
        surface_loader,
        surface,
        swapchain_loader,
        swapchain,
        extent,
        format: surface_format.format,
        image_initialized: vec![false; images.len()],
        images,
        image_views,
        framebuffers,
        present_queue,
        acquire_semaphore,
        present_queue_family_index,
        acquired_image_index: None,
        render_pass,
        descriptor_set_layout,
        descriptor_pool,
        descriptor_sets,
        pipeline_layout,
        pipeline,
        sampler,
        debug_readback,
        frames,
    })
}

fn create_vulkan_present_debug_readback(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    extent: vk::Extent2D,
) -> std::result::Result<VulkanPresentDebugReadback, String> {
    let staging_capacity = extent.width as usize * extent.height as usize * 4;
    let buffer_info = vk::BufferCreateInfo::default()
        .size(staging_capacity as u64)
        .usage(vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let staging_buffer = unsafe { device.create_buffer(&buffer_info, None) }
        .map_err(|err| format!("failed to create Vulkan present debug buffer: {err:?}"))?;
    let requirements = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let memory_type_index = find_memory_type_index(
        &memory_properties,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .ok_or_else(|| String::from("no suitable Vulkan memory type for present debug readback"))?;
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);
    let staging_memory = unsafe { device.allocate_memory(&alloc_info, None) }
        .map_err(|err| format!("failed to allocate Vulkan present debug memory: {err:?}"))?;
    unsafe {
        device
            .bind_buffer_memory(staging_buffer, staging_memory, 0)
            .map_err(|err| format!("failed to bind Vulkan present debug memory: {err:?}"))?;
    }
    Ok(VulkanPresentDebugReadback {
        staging_buffer,
        staging_memory,
        staging_capacity,
    })
}

fn create_external_vulkan_surface(
    entry: &ash::Entry,
    instance: &ash::Instance,
    external_window: ExternalVulkanWindowDescriptor,
) -> std::result::Result<vk::SurfaceKHR, String> {
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = (entry, instance);
    match external_window {
        #[cfg(target_os = "linux")]
        ExternalVulkanWindowDescriptor::Xlib {
            display, window, ..
        } => {
            let xlib_surface = ash::khr::xlib_surface::Instance::new(entry, instance);
            let create_info = vk::XlibSurfaceCreateInfoKHR::default()
                .dpy(display.cast())
                .window(window);
            unsafe { xlib_surface.create_xlib_surface(&create_info, None) }
                .map_err(|err| format!("failed to create Vulkan Xlib surface: {err:?}"))
        }
        #[cfg(target_os = "windows")]
        ExternalVulkanWindowDescriptor::Win32 {
            hwnd, hinstance, ..
        } => {
            let win32_surface = ash::khr::win32_surface::Instance::new(entry, instance);
            let create_info = vk::Win32SurfaceCreateInfoKHR::default()
                .hwnd(hwnd)
                .hinstance(hinstance);
            unsafe { win32_surface.create_win32_surface(&create_info, None) }
                .map_err(|err| format!("failed to create Vulkan Win32 surface: {err:?}"))
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        ExternalVulkanWindowDescriptor::Unsupported { .. } => Err(String::from(
            "external Vulkan surfaces are unsupported on this platform",
        )),
    }
}

fn choose_vulkan_queue_family_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> std::result::Result<u32, String> {
    let properties =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    properties
        .iter()
        .enumerate()
        .find_map(|(index, properties)| {
            (properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && properties.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .then_some(index as u32)
        })
        .or_else(|| {
            properties
                .iter()
                .enumerate()
                .find_map(|(index, properties)| {
                    properties
                        .queue_flags
                        .contains(vk::QueueFlags::GRAPHICS)
                        .then_some(index as u32)
                })
        })
        .ok_or_else(|| String::from("no Vulkan graphics queue family was reported"))
}

fn create_vulkan_shader_module(
    device: &ash::Device,
    bytes: &[u8],
) -> std::result::Result<vk::ShaderModule, String> {
    let words = ash::util::read_spv(&mut std::io::Cursor::new(bytes))
        .map_err(|err| format!("failed to decode embedded SPIR-V shader: {err}"))?;
    let create_info = vk::ShaderModuleCreateInfo::default().code(&words);
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan shader module: {err:?}"))
}

fn create_vulkan_present_render_pass(
    device: &ash::Device,
    format: vk::Format,
) -> std::result::Result<vk::RenderPass, String> {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
    let color_attachment_refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachment_refs)];
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    unsafe { device.create_render_pass(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan render pass: {err:?}"))
}

fn create_vulkan_present_descriptor_set_layout(
    device: &ash::Device,
) -> std::result::Result<vk::DescriptorSetLayout, String> {
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe { device.create_descriptor_set_layout(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan descriptor set layout: {err:?}"))
}

fn create_vulkan_present_sampler(device: &ash::Device) -> std::result::Result<vk::Sampler, String> {
    let create_info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .min_lod(0.0)
        .max_lod(0.0);
    unsafe { device.create_sampler(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan sampler: {err:?}"))
}

fn create_vulkan_present_pipeline(
    device: &ash::Device,
    extent: vk::Extent2D,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> std::result::Result<(vk::PipelineLayout, vk::Pipeline), String> {
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(std::slice::from_ref(&descriptor_set_layout));
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| format!("failed to create Vulkan pipeline layout: {err:?}"))?;

    let vertex_module = create_vulkan_shader_module(device, VULKAN_PRESENT_VERT_SPV)?;
    let fragment_module = match create_vulkan_shader_module(device, VULKAN_PRESENT_FRAG_SPV) {
        Ok(module) => module,
        Err(err) => {
            unsafe {
                device.destroy_pipeline_layout(pipeline_layout, None);
                device.destroy_shader_module(vertex_module, None);
            }
            return Err(err);
        }
    };
    let entry_point =
        CString::new("main").map_err(|_| String::from("invalid Vulkan shader entry point"))?;
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(&entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(&entry_point),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);
    let pipeline = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_, err)| format!("failed to create Vulkan graphics pipeline: {err:?}"))?
    .into_iter()
    .next()
    .ok_or_else(|| String::from("Vulkan graphics pipeline creation returned no pipeline"))?;

    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }

    let _ = extent;
    Ok((pipeline_layout, pipeline))
}

fn destroy_vulkan_present_state(device: &ash::Device, state: VulkanPresentState) {
    unsafe {
        device.destroy_semaphore(state.acquire_semaphore, None);
        device.destroy_sampler(state.sampler, None);
        device.destroy_pipeline(state.pipeline, None);
        device.destroy_pipeline_layout(state.pipeline_layout, None);
        device.destroy_descriptor_pool(state.descriptor_pool, None);
        device.destroy_descriptor_set_layout(state.descriptor_set_layout, None);
        for framebuffer in state.framebuffers {
            device.destroy_framebuffer(framebuffer, None);
        }
        for image_view in state.image_views {
            device.destroy_image_view(image_view, None);
        }
        device.destroy_render_pass(state.render_pass, None);
        for frame in state.frames {
            if let Some(image_view) = frame.transient_image_view {
                device.destroy_image_view(image_view, None);
            }
            device.destroy_fence(frame.fence, None);
            device.destroy_semaphore(frame.render_semaphore, None);
            device.free_command_buffers(frame.command_pool, &[frame.command_buffer]);
            device.destroy_command_pool(frame.command_pool, None);
        }
        if let Some(debug_readback) = state.debug_readback {
            device.destroy_buffer(debug_readback.staging_buffer, None);
            device.free_memory(debug_readback.staging_memory, None);
        }
        state
            .swapchain_loader
            .destroy_swapchain(state.swapchain, None);
        state.surface_loader.destroy_surface(state.surface, None);
    }
}

fn present_vulkan_image(
    vulkan: &mut VulkanInterfaceState,
    source_size: (u32, u32),
    external_window: Option<ExternalVulkanWindowDescriptor>,
) -> Result<bool> {
    if vulkan.present.is_none() {
        return Ok(false);
    }
    if current_pending_vulkan_image(vulkan).is_none() {
        return Ok(false);
    }

    let debug_step = if vulkan_debug_enabled() {
        Some(VULKAN_DEBUG_STEP_COUNTER.fetch_add(1, Ordering::Relaxed))
    } else {
        None
    };
    let (image_index, present_result, debug_present_readback) = {
        let mut image =
            take_current_pending_vulkan_image(vulkan).expect("pending image checked above");
        let present = vulkan
            .present
            .as_mut()
            .expect("present state checked above");

        if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "present step={} begin src={}x{} dst={}x{} semaphores={} layout={:?}",
                debug_step,
                source_size.0,
                source_size.1,
                present.extent.width,
                present.extent.height,
                image.semaphores.len(),
                image.image_layout
            );
        }

        let image_index = present.acquired_image_index.take().ok_or_else(|| {
            anyhow!("no acquired Vulkan swapchain image was prepared for this frame")
        })?;

        if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "present step={} acquired swapchain image {}",
                debug_step,
                image_index
            );
        }

        let framebuffer = present
            .framebuffers
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| anyhow!("invalid Vulkan framebuffer index: {image_index}"))?;
        let descriptor_set = present
            .descriptor_sets
            .get(image_index as usize)
            .copied()
            .ok_or_else(|| anyhow!("invalid Vulkan descriptor set index: {image_index}"))?;
        let frame = present
            .frames
            .get_mut(image_index as usize)
            .ok_or_else(|| anyhow!("invalid Vulkan present frame index: {image_index}"))?;

        let wait_semaphores = image.semaphores.clone();
        let signal_semaphore = image.signal_semaphore.take();
        let image_subresource_range = image.subresource_range;
        let direct_image_view_sampling = should_sample_pending_vulkan_image_directly(&image);
        let retired_image_view = frame.transient_image_view.take();
        let debug_present_readback = present.debug_readback.as_ref().and_then(|debug| {
            (vulkan_debug_enabled() && VULKAN_PRESENT_DEBUG_COUNTER.load(Ordering::Relaxed) < 8)
                .then_some((
                    frame.fence,
                    debug.staging_memory,
                    debug.staging_buffer,
                    debug.staging_capacity,
                    present.extent,
                    present.format,
                    present.images[image_index as usize],
                ))
        });
        let src_queue_family_index = if image.src_queue_family == u32::MAX {
            vk::QUEUE_FAMILY_IGNORED
        } else {
            image.src_queue_family
        };

        unsafe {
            vulkan
                .device
                .wait_for_fences(&[frame.fence], true, 5_000_000_000)
                .map_err(|err| anyhow!("failed waiting for Vulkan present fence: {err:?}"))?;
            if let Some(image_view) = retired_image_view {
                vulkan.device.destroy_image_view(image_view, None);
            }
            vulkan
                .device
                .reset_fences(&[frame.fence])
                .map_err(|err| anyhow!("failed resetting Vulkan present fence: {err:?}"))?;
            vulkan
                .device
                .reset_command_pool(frame.command_pool, vk::CommandPoolResetFlags::empty())
                .map_err(|err| anyhow!("failed resetting Vulkan present command pool: {err:?}"))?;

            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            vulkan
                .device
                .begin_command_buffer(frame.command_buffer, &begin_info)
                .map_err(|err| anyhow!("failed to begin Vulkan present command buffer: {err:?}"))?;

            let image_view = if direct_image_view_sampling {
                let view = create_sampling_image_view_for_pending_image(&vulkan.device, &image)?;
                frame.transient_image_view = Some(view);
                view
            } else {
                image.image_view
            };

            if !direct_image_view_sampling {
                let source_to_sample = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(image.image_layout)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(src_queue_family_index)
                    .dst_queue_family_index(present.present_queue_family_index)
                    .image(image.image)
                    .subresource_range(image_subresource_range);
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[source_to_sample],
                );
            }

            let sampler_info = vk::DescriptorImageInfo::default().sampler(present.sampler);
            let image_info = vk::DescriptorImageInfo::default()
                .image_view(image_view)
                .image_layout(if direct_image_view_sampling {
                    image.image_layout
                } else {
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                });
            let descriptor_writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(std::slice::from_ref(&sampler_info)),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&image_info)),
            ];
            vulkan
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_pass_info = vk::RenderPassBeginInfo::default()
                .render_pass(present.render_pass)
                .framebuffer(framebuffer)
                .render_area(vk::Rect2D::default().extent(present.extent))
                .clear_values(&clear_values);
            vulkan.device.cmd_begin_render_pass(
                frame.command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );
            let viewport = vk::Viewport::default()
                .x(0.0)
                .y(0.0)
                .width(present.extent.width as f32)
                .height(present.extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0);
            let scissor = vk::Rect2D::default().extent(present.extent);
            vulkan.device.cmd_set_viewport(
                frame.command_buffer,
                0,
                std::slice::from_ref(&viewport),
            );
            vulkan
                .device
                .cmd_set_scissor(frame.command_buffer, 0, std::slice::from_ref(&scissor));
            vulkan.device.cmd_bind_pipeline(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                present.pipeline,
            );
            vulkan.device.cmd_bind_descriptor_sets(
                frame.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                present.pipeline_layout,
                0,
                std::slice::from_ref(&descriptor_set),
                &[],
            );
            vulkan.device.cmd_draw(frame.command_buffer, 3, 1, 0, 0);
            vulkan.device.cmd_end_render_pass(frame.command_buffer);

            if let Some((_, _, staging_buffer, _, extent, _, swapchain_image)) =
                debug_present_readback
            {
                let to_transfer_src = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(swapchain_image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_transfer_src],
                );

                let copy_region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .mip_level(0)
                            .base_array_layer(0)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    });
                vulkan.device.cmd_copy_image_to_buffer(
                    frame.command_buffer,
                    swapchain_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    staging_buffer,
                    std::slice::from_ref(&copy_region),
                );

                let to_present = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(swapchain_image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1),
                    );
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_present],
                );
            }

            if !direct_image_view_sampling {
                let source_restore = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(image.image_layout)
                    .src_queue_family_index(present.present_queue_family_index)
                    .dst_queue_family_index(src_queue_family_index)
                    .image(image.image)
                    .subresource_range(image_subresource_range);
                vulkan.device.cmd_pipeline_barrier(
                    frame.command_buffer,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[source_restore],
                );
            }

            vulkan
                .device
                .end_command_buffer(frame.command_buffer)
                .map_err(|err| anyhow!("failed to end Vulkan present command buffer: {err:?}"))?;

            let mut submit_wait_semaphores = Vec::with_capacity(wait_semaphores.len() + 1);
            submit_wait_semaphores.push(present.acquire_semaphore);
            submit_wait_semaphores.extend(wait_semaphores.iter().copied());
            let submit_wait_stages =
                vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT; submit_wait_semaphores.len()];
            let mut submit_signal_semaphores = Vec::with_capacity(2);
            submit_signal_semaphores.push(frame.render_semaphore);
            if let Some(signal_semaphore) = signal_semaphore {
                submit_signal_semaphores.push(signal_semaphore);
            }
            let submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&submit_wait_semaphores)
                .wait_dst_stage_mask(&submit_wait_stages)
                .command_buffers(std::slice::from_ref(&frame.command_buffer))
                .signal_semaphores(&submit_signal_semaphores);
            vulkan
                .device
                .queue_submit(vulkan.presentation_queue, &[submit_info], frame.fence)
                .map_err(|err| anyhow!("failed submitting Vulkan present work: {err:?}"))?;

            if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "present step={} submitted present work",
                    debug_step
                );
            }

            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(std::slice::from_ref(&frame.render_semaphore))
                .swapchains(std::slice::from_ref(&present.swapchain))
                .image_indices(std::slice::from_ref(&image_index));
            let present_result = present
                .swapchain_loader
                .queue_present(present.present_queue, &present_info);
            (image_index, present_result, debug_present_readback)
        }
    };

    match present_result {
        Ok(true)
            if recreate_win32_vulkan_present_state(
                vulkan,
                external_window,
                "vkQueuePresentKHR returned SUBOPTIMAL_KHR",
            )? =>
        {
            return Ok(false);
        }
        Ok(_) => {}
        Err(err)
            if win32_vulkan_present_requires_recreate(err)
                && recreate_win32_vulkan_present_state(
                    vulkan,
                    external_window,
                    &format!("vkQueuePresentKHR returned {err:?}"),
                )? =>
        {
            return Ok(false);
        }
        Err(err) => {
            return Err(anyhow!("failed presenting Vulkan swapchain image: {err:?}"));
        }
    }

    if let Some(debug_step) = debug_step.filter(|step| *step < 8) {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "present step={} queue_present completed",
            debug_step
        );
    }

    if let Some(initialized) = vulkan
        .present
        .as_mut()
        .and_then(|present| present.image_initialized.get_mut(image_index as usize))
    {
        *initialized = true;
    }

    if let Some((fence, staging_memory, _, staging_capacity, extent, format, _)) =
        debug_present_readback
    {
        unsafe {
            vulkan
                .device
                .wait_for_fences(&[fence], true, 5_000_000_000)
                .map_err(|err| anyhow!("failed waiting for Vulkan present debug fence: {err:?}"))?;
            let mapped = vulkan
                .device
                .map_memory(
                    staging_memory,
                    0,
                    staging_capacity as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(|err| anyhow!("failed to map Vulkan present debug memory: {err:?}"))?;
            let pixels = std::slice::from_raw_parts(mapped as *const u8, staging_capacity);
            let mut normalized = pixels.to_vec();
            if matches!(
                format,
                vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
            ) {
                for pixel in normalized.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            let (checksum, non_black_pixels, first_rgba, center_rgba) =
                summarize_rgba_debug_pixels_grid(&normalized, extent.width, extent.height);
            let frame_index = VULKAN_PRESENT_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
            info!(
                target: "arcade_libretro::vulkan_debug",
                "present readback frame={} size={}x{} format={:?} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} center_rgba={:02x},{:02x},{:02x},{:02x}",
                frame_index,
                extent.width,
                extent.height,
                format,
                non_black_pixels,
                first_rgba[0],
                first_rgba[1],
                first_rgba[2],
                first_rgba[3],
                center_rgba[0],
                center_rgba[1],
                center_rgba[2],
                center_rgba[3],
            );
            vulkan.device.unmap_memory(staging_memory);
        }
    }

    Ok(true)
}

fn win32_vulkan_present_requires_recreate(result: vk::Result) -> bool {
    #[cfg(target_os = "windows")]
    {
        matches!(
            result,
            vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::ERROR_SURFACE_LOST_KHR
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = result;
        false
    }
}

fn recreate_win32_vulkan_present_state(
    vulkan: &mut VulkanInterfaceState,
    external_window: Option<ExternalVulkanWindowDescriptor>,
    trigger: &str,
) -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        let Some(ExternalVulkanWindowDescriptor::Win32 { .. }) = external_window else {
            return Ok(false);
        };
        let Some(old_present) = vulkan.present.take() else {
            return Ok(false);
        };

        warn!(
            target: "arcade_libretro::vulkan_debug",
            "recreating Win32 Vulkan present state after {trigger}"
        );

        wait_for_vulkan_device_idle(vulkan)?;

        let surface = create_external_vulkan_surface(
            &vulkan._entry,
            &vulkan.instance,
            external_window.unwrap(),
        )
        .map_err(|err| anyhow!("failed to recreate Win32 Vulkan surface: {err}"))?;
        let present_queue_family_index = old_present.present_queue_family_index;
        destroy_vulkan_present_state(&vulkan.device, old_present);

        match create_vulkan_present_state(VulkanPresentConfig {
            entry: &vulkan._entry,
            instance: &vulkan.instance,
            physical_device: vulkan.physical_device,
            device: &vulkan.device,
            surface,
            external_window: external_window.unwrap(),
            present_queue: vulkan.presentation_queue,
            present_queue_family_index,
        }) {
            Ok(present) => {
                if vulkan_debug_enabled() {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "recreated Win32 Vulkan present state {}x{} images={}",
                        present.extent.width,
                        present.extent.height,
                        present.images.len()
                    );
                }
                vulkan.sync_index = 0;
                vulkan.sync_frames = present.images.len().max(1) as u32;
                vulkan.pending_images.clear();
                vulkan.present = Some(present);
                Ok(true)
            }
            Err(err) => {
                let surface_loader =
                    ash::khr::surface::Instance::new(&vulkan._entry, &vulkan.instance);
                unsafe {
                    surface_loader.destroy_surface(surface, None);
                }
                Err(anyhow!("failed to recreate Win32 Vulkan swapchain: {err}"))
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = vulkan;
        let _ = external_window;
        let _ = trigger;
        Ok(false)
    }
}

fn destroy_vulkan_interface_state(state: VulkanInterfaceState) {
    if let Some(present) = state.present {
        destroy_vulkan_present_state(&state.device, present);
    }
    if let Some(readback) = state.readback {
        unsafe {
            state.device.destroy_fence(readback.fence, None);
            state
                .device
                .free_command_buffers(readback.command_pool, &[readback.command_buffer]);
            state
                .device
                .destroy_command_pool(readback.command_pool, None);
            state.device.destroy_buffer(readback.staging_buffer, None);
            state.device.free_memory(readback.staging_memory, None);
        }
    }
    if let Some(destroy_device) = state.destroy_device_callback {
        unsafe {
            destroy_device();
        }
    }
    unsafe {
        state.device.destroy_device(None);
        state.instance.destroy_instance(None);
    }
}

fn ensure_vulkan_interface_state_for(runtime: &HostRuntime) -> std::result::Result<String, String> {
    let negotiation = {
        let state = runtime.hw_render_state.lock();
        if let Some(vulkan) = state.vulkan.as_ref() {
            return Ok(vulkan.summary());
        }
        state.vulkan_negotiation
    };
    let external_window = ensure_external_vulkan_window_for(runtime);

    let vulkan = VulkanInterfaceState::create(runtime, negotiation, external_window)?;
    let summary = vulkan.summary();
    if vulkan_debug_enabled() {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "created Vulkan interface state {summary}"
        );
    }

    let mut state = runtime.hw_render_state.lock();
    if let Some(existing) = state.vulkan.as_ref() {
        return Ok(existing.summary());
    }
    state.vulkan = Some(vulkan);
    Ok(summary)
}

fn rebuild_vulkan_interface_state_for(
    runtime: &HostRuntime,
) -> std::result::Result<String, String> {
    let existing = {
        let mut state = runtime.hw_render_state.lock();
        state.vulkan.take()
    };
    if let Some(existing) = existing {
        destroy_vulkan_interface_state(existing);
    }
    ensure_vulkan_interface_state_for(runtime)
}

fn find_memory_type_index(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    required_flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    (0..memory_properties.memory_type_count).find(|index| {
        let supported = (memory_type_bits & (1_u32 << index)) != 0;
        let flags = memory_properties.memory_types[*index as usize].property_flags;
        supported && flags.contains(required_flags)
    })
}

fn ensure_vulkan_readback_resources(
    vulkan: &mut VulkanInterfaceState,
    required_size: usize,
) -> Result<()> {
    if vulkan
        .readback
        .as_ref()
        .map(|readback| readback.staging_capacity >= required_size)
        .unwrap_or(false)
    {
        return Ok(());
    }

    if let Some(readback) = vulkan.readback.take() {
        unsafe {
            vulkan.device.destroy_fence(readback.fence, None);
            vulkan
                .device
                .free_command_buffers(readback.command_pool, &[readback.command_buffer]);
            vulkan
                .device
                .destroy_command_pool(readback.command_pool, None);
            vulkan.device.destroy_buffer(readback.staging_buffer, None);
            vulkan.device.free_memory(readback.staging_memory, None);
        }
    }

    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(vulkan.queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    let command_pool = unsafe { vulkan.device.create_command_pool(&command_pool_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan command pool: {err:?}"))?;

    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer = unsafe { vulkan.device.allocate_command_buffers(&alloc_info) }
        .map_err(|err| anyhow!("failed to allocate Vulkan command buffer: {err:?}"))?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("failed to allocate a Vulkan command buffer"))?;

    let fence_info = vk::FenceCreateInfo::default();
    let fence = unsafe { vulkan.device.create_fence(&fence_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan fence: {err:?}"))?;

    let buffer_info = vk::BufferCreateInfo::default()
        .size(required_size as u64)
        .usage(vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let staging_buffer = unsafe { vulkan.device.create_buffer(&buffer_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan staging buffer: {err:?}"))?;
    let requirements = unsafe { vulkan.device.get_buffer_memory_requirements(staging_buffer) };
    let memory_properties = unsafe {
        vulkan
            .instance
            .get_physical_device_memory_properties(vulkan.physical_device)
    };
    let memory_type_index = find_memory_type_index(
        &memory_properties,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .ok_or_else(|| anyhow!("no suitable Vulkan host-visible memory type found"))?;
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);
    let staging_memory = unsafe { vulkan.device.allocate_memory(&alloc_info, None) }
        .map_err(|err| anyhow!("failed to allocate Vulkan staging memory: {err:?}"))?;
    unsafe {
        vulkan
            .device
            .bind_buffer_memory(staging_buffer, staging_memory, 0)
            .map_err(|err| anyhow!("failed to bind Vulkan staging memory: {err:?}"))?;
    }

    vulkan.readback = Some(VulkanReadbackState {
        command_pool,
        command_buffer,
        fence,
        staging_buffer,
        staging_memory,
        staging_capacity: required_size,
    });

    Ok(())
}

fn debug_readback_vulkan_source_image(
    vulkan: &mut VulkanInterfaceState,
    frame_size: (u32, u32),
) -> Result<()> {
    if !vulkan_debug_enabled() || VULKAN_SOURCE_IMAGE_DEBUG_COUNTER.load(Ordering::Relaxed) >= 8 {
        return Ok(());
    }

    let Some(image) = current_pending_vulkan_image(vulkan) else {
        return Ok(());
    };
    if should_sample_pending_vulkan_image_directly(image) {
        return Ok(());
    }
    if !image.semaphores.is_empty() || image.signal_semaphore.is_some() {
        return Ok(());
    }

    let format = image.format;
    let image_handle = image.image;
    let image_layout = image.image_layout;
    let image_subresource_range = image.subresource_range;
    let image_subresource_layers = image.subresource_layers;
    let src_queue_family_index = if image.src_queue_family == u32::MAX {
        vk::QUEUE_FAMILY_IGNORED
    } else {
        image.src_queue_family
    };

    let needs_bgra_swizzle = match format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => false,
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => true,
        _ => {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "source image debug readback skipped unsupported_format={:?}",
                format
            );
            return Ok(());
        }
    };

    let required_size = frame_size.0 as usize * frame_size.1 as usize * 4;
    ensure_vulkan_readback_resources(vulkan, required_size)?;

    let readback = vulkan
        .readback
        .as_ref()
        .ok_or_else(|| anyhow!("Vulkan readback resources were not initialized"))?;
    let command_buffer = readback.command_buffer;
    let fence = readback.fence;
    let staging_buffer = readback.staging_buffer;
    let staging_memory = readback.staging_memory;
    let queue = vulkan.interface.queue;
    let queue_family_index = vulkan.queue_family_index;

    unsafe {
        vulkan
            .device
            .reset_fences(&[fence])
            .map_err(|err| anyhow!("failed to reset Vulkan source debug fence: {err:?}"))?;
        vulkan
            .device
            .reset_command_pool(readback.command_pool, vk::CommandPoolResetFlags::empty())
            .map_err(|err| anyhow!("failed to reset Vulkan source debug command pool: {err:?}"))?;

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        vulkan
            .device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                anyhow!("failed to begin Vulkan source debug command buffer: {err:?}")
            })?;

        let to_transfer = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(image_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(src_queue_family_index)
            .dst_queue_family_index(queue_family_index)
            .image(image_handle)
            .subresource_range(image_subresource_range);
        vulkan.device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_transfer],
        );

        let copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(image_subresource_layers)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: frame_size.0,
                height: frame_size.1,
                depth: 1,
            });
        vulkan.device.cmd_copy_image_to_buffer(
            command_buffer,
            image_handle,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            staging_buffer,
            &[copy],
        );

        let restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(image_layout)
            .src_queue_family_index(queue_family_index)
            .dst_queue_family_index(src_queue_family_index)
            .image(image_handle)
            .subresource_range(image_subresource_range);
        vulkan.device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[restore],
        );

        vulkan
            .device
            .end_command_buffer(command_buffer)
            .map_err(|err| anyhow!("failed to end Vulkan source debug command buffer: {err:?}"))?;

        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));
        vulkan
            .device
            .queue_submit(queue, &[submit_info], fence)
            .map_err(|err| anyhow!("failed to submit Vulkan source debug readback: {err:?}"))?;
        vulkan
            .device
            .wait_for_fences(&[fence], true, 5_000_000_000)
            .map_err(|err| anyhow!("failed waiting for Vulkan source debug fence: {err:?}"))?;

        let mapped = vulkan
            .device
            .map_memory(
                staging_memory,
                0,
                required_size as u64,
                vk::MemoryMapFlags::empty(),
            )
            .map_err(|err| anyhow!("failed to map Vulkan source debug staging memory: {err:?}"))?;
        let src = std::slice::from_raw_parts(mapped as *const u8, required_size);
        let mut pixels = src.to_vec();
        vulkan.device.unmap_memory(staging_memory);

        if needs_bgra_swizzle {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        let (checksum, non_black_pixels, first_rgba, center_rgba) =
            summarize_rgba_debug_pixels_grid(&pixels, frame_size.0, frame_size.1);
        let frame_index = VULKAN_SOURCE_IMAGE_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
        info!(
            target: "arcade_libretro::vulkan_debug",
            "source image readback frame={} size={}x{} format={:?} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} center_rgba={:02x},{:02x},{:02x},{:02x}",
            frame_index,
            frame_size.0,
            frame_size.1,
            format,
            non_black_pixels,
            first_rgba[0],
            first_rgba[1],
            first_rgba[2],
            first_rgba[3],
            center_rgba[0],
            center_rgba[1],
            center_rgba[2],
            center_rgba[3],
        );
    }

    Ok(())
}

#[cfg(test)]
fn probe_basic_vulkan_bootstrap() -> std::result::Result<String, String> {
    let app_name = CString::new("Personal Arcade Native")
        .map_err(|_| String::from("failed to build Vulkan app name"))?;
    let engine_name = CString::new("arcade-libretro")
        .map_err(|_| String::from("failed to build Vulkan engine name"))?;

    let entry = load_vulkan_entry()?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(0)
        .engine_name(&engine_name)
        .engine_version(0)
        .api_version(vk::API_VERSION_1_0);
    let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|err| format!("failed to create Vulkan instance: {err:?}"))?;

    let result = (|| {
        let physical_devices = unsafe { instance.enumerate_physical_devices() }
            .map_err(|err| format!("failed to enumerate Vulkan physical devices: {err:?}"))?;
        let physical_device = *physical_devices
            .first()
            .ok_or_else(|| String::from("no Vulkan physical devices were reported"))?;

        let queue_family_index =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                .iter()
                .enumerate()
                .find_map(|(index, properties)| {
                    properties
                        .queue_flags
                        .contains(vk::QueueFlags::GRAPHICS)
                        .then_some(index as u32)
                })
                .ok_or_else(|| String::from("no Vulkan graphics queue family was reported"))?;

        let queue_priorities = [1.0_f32];
        let queue_infos = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&queue_priorities)];
        let device_create_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos);

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None) }
            .map_err(|err| format!("failed to create Vulkan logical device: {err:?}"))?;

        let summary = {
            let properties = unsafe { instance.get_physical_device_properties(physical_device) };
            let device_name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            let _queue = unsafe { device.get_device_queue(queue_family_index, 0) };
            format!("gpu={device_name}, queue_family={queue_family_index}")
        };

        unsafe {
            device.destroy_device(None);
        }

        Ok(summary)
    })();

    unsafe {
        instance.destroy_instance(None);
    }

    result
}

#[cfg(test)]
fn vulkan_unimplemented_message(runtime: Option<&HostRuntime>) -> String {
    let frontend_summary = runtime.and_then(frontend_windowing_summary_for);

    match probe_basic_vulkan_bootstrap() {
        Ok(vulkan_summary) => match frontend_summary {
            Some(frontend_summary) => format!(
                "core requested Vulkan hardware render, and the frontend can expose {frontend_summary}; basic Vulkan bootstrap succeeded ({vulkan_summary}), but this host does not implement Vulkan frame presentation yet"
            ),
            None => format!(
                "core requested Vulkan hardware render; basic Vulkan bootstrap succeeded ({vulkan_summary}), but this host does not implement Vulkan frame presentation yet"
            ),
        },
        Err(probe_error) => match frontend_summary {
            Some(frontend_summary) => format!(
                "core requested Vulkan hardware render, and the frontend can expose {frontend_summary}, but basic Vulkan bootstrap failed: {probe_error}"
            ),
            None => format!(
                "core requested Vulkan hardware render, but basic Vulkan bootstrap failed: {probe_error}"
            ),
        },
    }
}

fn record_runtime_load_error(message: impl Into<String>) {
    let message = message.into();
    let _ = with_active_runtime(|runtime| {
        runtime.environment_context.lock().last_load_error = Some(message.clone());
    });
}

unsafe extern "C" fn retro_vulkan_set_image(
    handle: *mut c_void,
    image: *const RetroVulkanImage,
    num_semaphores: u32,
    semaphores: *const vk::Semaphore,
    src_queue_family: u32,
) {
    if handle.is_null() || image.is_null() {
        return;
    }

    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let image = unsafe { &*image };
    let semaphore_values = if num_semaphores == 0 || semaphores.is_null() {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(semaphores, num_semaphores as usize) }.to_vec()
    };

    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return;
    };
    let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
    if vulkan_debug_enabled() && debug_step < 8 {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "retro_vulkan_set_image step={} format={:?} src_queue_family={} semaphores={} base_mip={} base_layer={} layers={}",
            debug_step,
            image.create_info.format,
            src_queue_family,
            semaphore_values.len(),
            image.create_info.subresource_range.base_mip_level,
            image.create_info.subresource_range.base_array_layer,
            image.create_info.subresource_range.layer_count.max(1)
        );
    }
    vulkan.pending_images.push_back(PendingVulkanImage {
        image: image.create_info.image,
        image_view: image.image_view,
        image_layout: image.image_layout,
        format: image.create_info.format,
        view_type: image.create_info.view_type,
        components: image.create_info.components,
        subresource_range: PendingVulkanImage::color_subresource_range(&image.create_info),
        subresource_layers: PendingVulkanImage::color_subresource_layers(&image.create_info),
        semaphores: semaphore_values,
        src_queue_family,
        signal_semaphore: None,
    });
    let max_pending_images = vulkan.sync_frames.max(1) as usize + 2;
    while vulkan.pending_images.len() > max_pending_images {
        vulkan.pending_images.pop_front();
    }
}

unsafe extern "C" fn retro_vulkan_get_sync_index(_handle: *mut c_void) -> u32 {
    let Some(runtime) = active_runtime() else {
        return 0;
    };
    current_vulkan_sync_index(&runtime)
}

unsafe extern "C" fn retro_vulkan_get_sync_index_mask(_handle: *mut c_void) -> u32 {
    let Some(runtime) = active_runtime() else {
        return 1;
    };
    current_vulkan_sync_mask(&runtime)
}

unsafe extern "C" fn retro_vulkan_set_command_buffers(
    handle: *mut c_void,
    num_cmd: u32,
    cmd: *const vk::CommandBuffer,
) {
    if handle.is_null() || cmd.is_null() || num_cmd == 0 {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let command_buffers = unsafe { std::slice::from_raw_parts(cmd, num_cmd as usize) };
    if vulkan_debug_enabled() {
        let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
        if debug_step < 8 {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "set_command_buffers step={} count={}",
                debug_step,
                command_buffers.len()
            );
        }
    }
    if let Err(err) = submit_vulkan_command_buffers_immediately(runtime, command_buffers) {
        warn!("libretro hw-render: {err}");
        record_runtime_load_error(err.to_string());
    }
}

unsafe extern "C" fn retro_vulkan_wait_sync_index(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }

    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return;
    };
    let Some(present) = vulkan.present.as_ref() else {
        let sync_index = vulkan.sync_index;
        if vulkan_force_fallback_idle() {
            let wait_result = wait_for_vulkan_device_idle(vulkan);
            if let Err(err) = wait_result {
                warn!(
                    "libretro hw-render: wait_sync_index failed waiting for fallback queue idle: {err}"
                );
                record_runtime_load_error(format!(
                    "wait_sync_index failed waiting for fallback queue idle: {err}"
                ));
            }
        }
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "wait_sync_index completed fallback step={} sync_index={} force_idle={}",
                    debug_step,
                    sync_index,
                    vulkan_force_fallback_idle()
                );
            }
        }
        vulkan.waiting_for_core_wait_sync = false;
        vulkan.wait_sync_generation = vulkan.wait_sync_generation.wrapping_add(1);
        return;
    };
    let Some(frame) = present
        .frames
        .get(vulkan.sync_index as usize)
        .or_else(|| present.frames.first())
    else {
        return;
    };

    let wait_result = unsafe {
        vulkan
            .device
            .wait_for_fences(&[frame.fence], true, 5_000_000_000)
    };
    if let Err(err) = wait_result {
        warn!("libretro hw-render: wait_sync_index failed waiting for present fence: {err:?}");
        record_runtime_load_error(format!(
            "wait_sync_index failed waiting for present fence: {err:?}"
        ));
    } else if vulkan_debug_enabled() {
        let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
        if debug_step < 16 {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "wait_sync_index completed step={} sync_index={}",
                debug_step,
                vulkan.sync_index
            );
        }
    }
    vulkan.waiting_for_core_wait_sync = false;
    vulkan.wait_sync_generation = vulkan.wait_sync_generation.wrapping_add(1);
}

unsafe extern "C" fn retro_vulkan_lock_queue(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    lock_vulkan_queue(runtime);
}

unsafe extern "C" fn retro_vulkan_unlock_queue(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    unlock_vulkan_queue(runtime);
}

unsafe extern "C" fn retro_vulkan_set_signal_semaphore(
    handle: *mut c_void,
    semaphore: vk::Semaphore,
) {
    if handle.is_null() {
        return;
    }
    let runtime = unsafe { &*(handle as *const HostRuntime) };
    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return;
    };
    if let Some(image) = vulkan.pending_images.back_mut() {
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 8 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "set_signal_semaphore step={} semaphore={:?}",
                    debug_step,
                    semaphore
                );
            }
        }
        image.signal_semaphore = Some(semaphore);
    }
}

fn release_hw_render_target(state: &mut HardwareRenderState) {
    let Some(target) = state.target.take() else {
        return;
    };
    let Some(runtime) = active_runtime() else {
        return;
    };
    let Some(gl) = runtime
        .video_coordinator
        .lock()
        .frontend_capabilities()
        .gl_context
        .clone()
    else {
        return;
    };

    unsafe {
        gl.delete_renderbuffer(target.depth_stencil);
        gl.delete_texture(target.color_texture);
        gl.delete_framebuffer(target.framebuffer);
    }
}

fn destroy_hw_render_session_for(runtime: &HostRuntime) {
    runtime.video_coordinator.lock().end_session(runtime);
    let (callbacks, vulkan, external_vulkan_window) = {
        let mut state = runtime.hw_render_state.lock();
        release_hw_render_target(&mut state);
        let vulkan = state.vulkan.take();
        let external_vulkan_window = state.external_vulkan_window.take();
        state.external_vulkan_present_active = false;
        state.vulkan_fallback_frame_size = None;
        state.vulkan_negotiation = None;
        state.context_ready = false;
        state.context_type = None;
        (state.callbacks.take(), vulkan, external_vulkan_window)
    };

    if let Some(callbacks) = callbacks {
        if let Some(context_destroy) = callbacks.context_destroy {
            unsafe {
                context_destroy();
            }
        }
    }

    if let Some(vulkan) = vulkan {
        destroy_vulkan_interface_state(vulkan);
    }
    if let Some(external_vulkan_window) = external_vulkan_window {
        destroy_external_vulkan_window(external_vulkan_window);
    }
}

fn destroy_hw_render_session() {
    let Some(runtime) = active_runtime() else {
        return;
    };
    destroy_hw_render_session_for(&runtime);
}

fn invoke_hw_context_reset() -> Result<()> {
    let (callbacks, context_type) = {
        let Some(runtime) = active_runtime() else {
            return Ok(());
        };
        let state = runtime.hw_render_state.lock();
        if state.context_ready {
            return Ok(());
        }
        (state.callbacks, state.context_type)
    };
    let Some(callbacks) = callbacks else {
        return Ok(());
    };
    if context_type == Some(RETRO_HW_CONTEXT_VULKAN) {
        let Some(runtime) = active_runtime() else {
            return Err(anyhow!("hardware-render core requires an active runtime"));
        };
        ensure_vulkan_interface_state_for(&runtime).map_err(|err| anyhow!(err))?;
    } else if !hardware_render_frontend_available() {
        return Err(anyhow!(
            "hardware-render core requested a GL context before the frontend GL context was ready"
        ));
    }
    if let Some(context_reset) = callbacks.context_reset {
        unsafe {
            context_reset();
        }
    }
    if let Some(runtime) = active_runtime() {
        runtime.hw_render_state.lock().context_ready = true;
    }
    Ok(())
}

fn initialize_hw_render_context(target_size: (u32, u32)) -> Result<()> {
    let is_vulkan = with_active_runtime(|runtime| {
        runtime.hw_render_state.lock().context_type == Some(RETRO_HW_CONTEXT_VULKAN)
    })
    .unwrap_or(false);

    if !is_vulkan {
        ensure_hw_render_target(target_size)?;
    }

    invoke_hw_context_reset()
}

fn ensure_hw_render_target(target_size: (u32, u32)) -> Result<()> {
    let Some(runtime) = active_runtime() else {
        return Err(anyhow!("hardware-render core requires an active runtime"));
    };
    let Some(gl) = runtime.hw_render_state.lock().frontend_gl_context.clone() else {
        return Err(anyhow!(
            "hardware-render core requires an active GL context"
        ));
    };
    let mut state = runtime.hw_render_state.lock();

    if state
        .target
        .as_ref()
        .map(|target| target.width == target_size.0 && target.height == target_size.1)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let (width, height) = target_size;
    let (framebuffer, color_texture, depth_stencil) = if let Some(existing) = state.target.as_ref()
    {
        (
            existing.framebuffer,
            existing.color_texture,
            existing.depth_stencil,
        )
    } else {
        let color_texture = unsafe { gl.create_texture() }
            .map_err(|err| anyhow!("failed to create hardware-render texture: {err}"))?;
        let framebuffer = unsafe { gl.create_framebuffer() }
            .map_err(|err| anyhow!("failed to create hardware-render framebuffer: {err}"))?;
        let depth_stencil = unsafe { gl.create_renderbuffer() }
            .map_err(|err| anyhow!("failed to create hardware-render depth buffer: {err}"))?;
        (framebuffer, color_texture, depth_stencil)
    };

    unsafe {
        let previous_texture = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
        let previous_framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let previous_renderbuffer = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);

        let previous_texture = NonZeroU32::new(previous_texture as u32).map(glow::NativeTexture);
        let previous_framebuffer =
            NonZeroU32::new(previous_framebuffer as u32).map(glow::NativeFramebuffer);
        let previous_renderbuffer =
            NonZeroU32::new(previous_renderbuffer as u32).map(glow::NativeRenderbuffer);

        gl.bind_texture(glow::TEXTURE_2D, Some(color_texture));
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
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(color_texture),
            0,
        );
        gl.draw_buffer(glow::COLOR_ATTACHMENT0);
        gl.read_buffer(glow::COLOR_ATTACHMENT0);

        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_stencil));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::DEPTH24_STENCIL8,
            width as i32,
            height as i32,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_STENCIL_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth_stencil),
        );

        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_renderbuffer(glow::RENDERBUFFER, previous_renderbuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
        gl.bind_texture(glow::TEXTURE_2D, previous_texture);

        if status != glow::FRAMEBUFFER_COMPLETE {
            if state.target.is_none() {
                gl.delete_renderbuffer(depth_stencil);
                gl.delete_texture(color_texture);
                gl.delete_framebuffer(framebuffer);
            }
            return Err(anyhow!(
                "failed to create a complete hardware-render framebuffer (status=0x{status:04x})"
            ));
        }
    }

    if let Some(target) = state.target.as_mut() {
        target.width = width;
        target.height = height;
    } else {
        state.target = Some(HardwareRenderTarget {
            framebuffer,
            color_texture,
            depth_stencil,
            width,
            height,
        });
    }
    Ok(())
}

fn take_vulkan_render_frame(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> Result<Option<FrameBuffer>> {
    let frame_size = resolve_vulkan_frame_size(runtime, pending);
    if vulkan_debug_enabled()
        && (frame_size.0 != pending.width || frame_size.1 != pending.height)
        && VULKAN_READBACK_DEBUG_COUNTER.load(Ordering::Relaxed) < 16
    {
        info!(
            target: "arcade_libretro::vulkan_debug",
            "readback using fallback size pending={}x{} resolved={}x{}",
            pending.width,
            pending.height,
            frame_size.0,
            frame_size.1,
        );
    }
    wait_for_unsignaled_vulkan_image(runtime)?;
    let required_size = frame_size.0 as usize * frame_size.1 as usize * 4;

    let mut state = runtime.hw_render_state.lock();
    let external_window = state
        .external_vulkan_window
        .as_ref()
        .map(ExternalVulkanWindow::descriptor);
    let Some(vulkan) = state.vulkan.as_mut() else {
        return Err(anyhow!(
            "Vulkan hardware-render frame requested before Vulkan interface setup"
        ));
    };
    debug_readback_vulkan_source_image(vulkan, frame_size)?;
    if present_vulkan_image(vulkan, frame_size, external_window)? {
        return Ok(None);
    }
    if vulkan_force_fallback_idle() {
        wait_for_vulkan_device_idle(vulkan)?;
    }
    ensure_vulkan_readback_resources(vulkan, required_size)?;

    let Some(mut image) = take_current_pending_vulkan_image(vulkan) else {
        return Ok(None);
    };

    let supported_format = match image.format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => Some(false),
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => Some(true),
        _ => None,
    };
    let Some(needs_bgra_swizzle) = supported_format else {
        return Err(anyhow!(
            "unsupported Vulkan image format for readback: {:?}",
            image.format
        ));
    };

    let readback = vulkan
        .readback
        .as_ref()
        .ok_or_else(|| anyhow!("Vulkan readback resources were not initialized"))?;
    let command_buffer = readback.command_buffer;
    let fence = readback.fence;
    let staging_buffer = readback.staging_buffer;
    let staging_memory = readback.staging_memory;
    let queue = vulkan.interface.queue;
    let queue_family_index = vulkan.queue_family_index;
    let image_handle = image.image;
    let image_layout = image.image_layout;
    let image_subresource_range = image.subresource_range;
    let image_subresource_layers = image.subresource_layers;
    let wait_semaphores = std::mem::take(&mut image.semaphores);
    let signal_semaphore = image.signal_semaphore.take();
    let src_queue_family_index = if image.src_queue_family == u32::MAX {
        vk::QUEUE_FAMILY_IGNORED
    } else {
        image.src_queue_family
    };

    unsafe {
        vulkan
            .device
            .reset_fences(&[fence])
            .map_err(|err| anyhow!("failed to reset Vulkan fence: {err:?}"))?;
        vulkan
            .device
            .reset_command_pool(readback.command_pool, vk::CommandPoolResetFlags::empty())
            .map_err(|err| anyhow!("failed to reset Vulkan command pool: {err:?}"))?;

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        vulkan
            .device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| anyhow!("failed to begin Vulkan command buffer: {err:?}"))?;

        let to_transfer = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(image_layout)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(src_queue_family_index)
            .dst_queue_family_index(queue_family_index)
            .image(image_handle)
            .subresource_range(image_subresource_range);
        vulkan.device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_transfer],
        );

        let copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(image_subresource_layers)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: frame_size.0,
                height: frame_size.1,
                depth: 1,
            });
        vulkan.device.cmd_copy_image_to_buffer(
            command_buffer,
            image_handle,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            staging_buffer,
            &[copy],
        );

        let restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(image_layout)
            .src_queue_family_index(queue_family_index)
            .dst_queue_family_index(src_queue_family_index)
            .image(image_handle)
            .subresource_range(image_subresource_range);
        vulkan.device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[restore],
        );

        vulkan
            .device
            .end_command_buffer(command_buffer)
            .map_err(|err| anyhow!("failed to end Vulkan command buffer: {err:?}"))?;

        let wait_stage_masks = vec![vk::PipelineStageFlags::TRANSFER; wait_semaphores.len()];
        let signal_semaphores = signal_semaphore
            .map(|value| vec![value])
            .unwrap_or_default();
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stage_masks)
            .command_buffers(std::slice::from_ref(&command_buffer))
            .signal_semaphores(&signal_semaphores);
        vulkan
            .device
            .queue_submit(queue, &[submit_info], fence)
            .map_err(|err| anyhow!("failed to submit Vulkan readback work: {err:?}"))?;
        vulkan
            .device
            .wait_for_fences(&[fence], true, 5_000_000_000)
            .map_err(|err| anyhow!("failed waiting for Vulkan readback fence: {err:?}"))?;
    }

    let mut pixels = vec![0_u8; required_size];
    unsafe {
        let mapped = vulkan
            .device
            .map_memory(
                staging_memory,
                0,
                required_size as u64,
                vk::MemoryMapFlags::empty(),
            )
            .map_err(|err| anyhow!("failed to map Vulkan staging memory: {err:?}"))?;
        let src = std::slice::from_raw_parts(mapped as *const u8, required_size);
        pixels.copy_from_slice(src);
        vulkan.device.unmap_memory(staging_memory);
    }
    drop(state);

    // Vulkan readback frames are displayed as standalone game images in the UI. Some cores
    // leave alpha undefined or zero, which makes the texture effectively invisible even though
    // RGB data is valid.
    if needs_bgra_swizzle {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
    } else {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
    }

    if pending.bottom_left_origin {
        let row_len = frame_size.0 as usize * 4;
        let half_height = frame_size.1 as usize / 2;
        for row in 0..half_height {
            let top = row * row_len;
            let bottom = (frame_size.1 as usize - 1 - row) * row_len;
            let (head, tail) = pixels.split_at_mut(bottom);
            let top_row = &mut head[top..top + row_len];
            let bottom_row = &mut tail[..row_len];
            top_row.swap_with_slice(bottom_row);
        }
    }

    if vulkan_debug_enabled() {
        let frame_index = VULKAN_READBACK_DEBUG_COUNTER.fetch_add(1, Ordering::Relaxed);
        if frame_index < 16 {
            let (checksum, non_black_pixels, first_rgba) = summarize_rgba_debug_pixels(&pixels);
            info!(
                target: "arcade_libretro::vulkan_debug",
                "readback frame={} size={}x{} pitch={} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} bottom_left_origin={}",
                frame_index,
                frame_size.0,
                frame_size.1,
                frame_size.0 as usize * 4,
                non_black_pixels,
                first_rgba[0],
                first_rgba[1],
                first_rgba[2],
                first_rgba[3],
                pending.bottom_left_origin,
            );
        }
    }

    Ok(Some(FrameBuffer {
        width: frame_size.0,
        height: frame_size.1,
        pitch: frame_size.0 as usize * 4,
        data: pixels,
        pixel_format: PixelFormat::Rgba8888,
    }))
}

fn wait_for_unsignaled_vulkan_image(runtime: &HostRuntime) -> Result<()> {
    let mut logged_wait = false;
    let mut logged_wait_sync = false;
    let started_at = std::time::Instant::now();

    loop {
        let mut state = runtime.hw_render_state.lock();
        let Some(vulkan) = state.vulkan.as_mut() else {
            return Ok(());
        };
        let Some(image) = current_pending_vulkan_image(vulkan) else {
            return Ok(());
        };
        if !image.semaphores.is_empty() || image.signal_semaphore.is_some() {
            return Ok(());
        }
        if vulkan.queue_locked {
            drop(state);
            if !logged_wait && vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "waiting for core Vulkan queue unlock before consuming unsignaled image"
                );
                logged_wait = true;
            }
            if started_at.elapsed() > std::time::Duration::from_secs(5) {
                return Err(anyhow!(
                    "timed out waiting for core Vulkan queue unlock for unsignaled image"
                ));
            }
            std::thread::yield_now();
            continue;
        }

        if vulkan.waiting_for_core_wait_sync {
            drop(state);
            if !logged_wait_sync && vulkan_debug_enabled() {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "waiting for core wait_sync_index before consuming unsignaled image"
                );
                logged_wait_sync = true;
            }
            if started_at.elapsed() > std::time::Duration::from_millis(50) {
                if vulkan_debug_enabled() {
                    let state = runtime.hw_render_state.lock();
                    if let Some(vulkan) = state.vulkan.as_ref() {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "timed out waiting for core wait_sync_index; proceeding without forced device idle sync_index={} generation={} force_idle={}",
                            vulkan.sync_index,
                            vulkan.wait_sync_generation,
                            vulkan_force_fallback_idle()
                        );
                    }
                }
                let mut state = runtime.hw_render_state.lock();
                if let Some(vulkan) = state.vulkan.as_mut() {
                    vulkan.waiting_for_core_wait_sync = false;
                    if vulkan_force_fallback_idle() {
                        if vulkan_debug_enabled()
                            && VULKAN_READBACK_DEBUG_COUNTER.load(Ordering::Relaxed) < 16
                        {
                            info!(
                                target: "arcade_libretro::vulkan_debug",
                                "forcing Vulkan device idle for unsignaled image readiness"
                            );
                        }
                        return wait_for_vulkan_device_idle(vulkan);
                    }
                }
                return Ok(());
            }
            std::thread::yield_now();
            continue;
        }

        if should_sample_pending_vulkan_image_directly(image) {
            return Ok(());
        }

        if vulkan_force_fallback_idle() {
            if vulkan_debug_enabled() && VULKAN_READBACK_DEBUG_COUNTER.load(Ordering::Relaxed) < 16
            {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "forcing Vulkan device idle for unsignaled image readiness"
                );
            }
            return wait_for_vulkan_device_idle(vulkan);
        }
        return Ok(());
    }
}

fn submit_vulkan_command_buffers_immediately(
    runtime: &HostRuntime,
    command_buffers: &[vk::CommandBuffer],
) -> Result<()> {
    if command_buffers.is_empty() {
        return Ok(());
    }

    let mut state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_mut() else {
        return Err(anyhow!(
            "Vulkan command buffers were submitted before Vulkan interface setup"
        ));
    };

    let fence_info = vk::FenceCreateInfo::default();
    let fence = unsafe { vulkan.device.create_fence(&fence_info, None) }
        .map_err(|err| anyhow!("failed to create Vulkan command fence: {err:?}"))?;

    let submit_info = vk::SubmitInfo::default().command_buffers(command_buffers);
    let submit_result = unsafe {
        vulkan
            .device
            .queue_submit(vulkan.interface.queue, &[submit_info], fence)
    };
    if let Err(err) = submit_result {
        unsafe {
            vulkan.device.destroy_fence(fence, None);
        }
        return Err(anyhow!(
            "failed to submit core Vulkan command buffers: {err:?}"
        ));
    }

    let wait_result = unsafe { vulkan.device.wait_for_fences(&[fence], true, 5_000_000_000) };
    unsafe {
        vulkan.device.destroy_fence(fence, None);
    }
    wait_result.map_err(|err| anyhow!("timed out waiting for core Vulkan command buffers: {err:?}"))
}

fn wait_for_vulkan_device_idle(vulkan: &VulkanInterfaceState) -> Result<()> {
    unsafe {
        vulkan
            .device
            .device_wait_idle()
            .map_err(|err| anyhow!("failed waiting for Vulkan device idle: {err:?}"))
    }
}

fn drain_frontend_gl_errors(runtime: &HostRuntime, stage: &str) {
    let (gl, context_type) = {
        let state = runtime.hw_render_state.lock();
        (state.frontend_gl_context.clone(), state.context_type)
    };
    if context_type == Some(RETRO_HW_CONTEXT_VULKAN) {
        return;
    }
    let Some(gl) = gl else {
        return;
    };

    trace_frontend_gl_framebuffer(&gl, stage);

    let mut errors = Vec::new();
    unsafe {
        loop {
            let error = gl.get_error();
            if error == glow::NO_ERROR {
                break;
            }
            errors.push(error);
            if errors.len() >= 16 {
                break;
            }
        }
    }

    if !errors.is_empty() && std::env::var_os("LIBRETRO_TRACE_GL_ERRORS").is_some() {
        eprintln!("drained frontend GL errors after {stage}: {errors:#x?}");
    }
}

fn force_default_gl_framebuffer(_runtime: &HostRuntime) -> bool {
    std::env::var_os("ARCADE_GL_FORCE_DEFAULT_FRAMEBUFFER").is_some()
}

fn lock_vulkan_queue(runtime: &HostRuntime) {
    loop {
        {
            let mut state = runtime.hw_render_state.lock();
            let Some(vulkan) = state.vulkan.as_mut() else {
                return;
            };
            if !vulkan.queue_locked {
                vulkan.queue_locked = true;
                if vulkan_debug_enabled() {
                    let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
                    if debug_step < 16 {
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "lock_queue step={} sync_index={}",
                            debug_step,
                            vulkan.sync_index
                        );
                    }
                }
                return;
            }
        }
        std::thread::yield_now();
    }
}

fn unlock_vulkan_queue(runtime: &HostRuntime) {
    let mut state = runtime.hw_render_state.lock();
    if let Some(vulkan) = state.vulkan.as_mut() {
        vulkan.queue_locked = false;
        if vulkan_debug_enabled() {
            let debug_step = VULKAN_DEBUG_STEP_COUNTER.load(Ordering::Relaxed);
            if debug_step < 16 {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "unlock_queue step={} sync_index={}",
                    debug_step,
                    vulkan.sync_index
                );
            }
        }
    }
}

fn current_vulkan_sync_index(runtime: &HostRuntime) -> u32 {
    let state = runtime.hw_render_state.lock();
    state
        .vulkan
        .as_ref()
        .map(|vulkan| vulkan.sync_index)
        .unwrap_or(0)
}

fn current_vulkan_sync_mask(runtime: &HostRuntime) -> u32 {
    let state = runtime.hw_render_state.lock();
    let Some(vulkan) = state.vulkan.as_ref() else {
        return 1;
    };
    let frames = vulkan
        .present
        .as_ref()
        .map(|present| present.images.len() as u32)
        .unwrap_or(vulkan.sync_frames)
        .clamp(1, 31);
    ((1_u32 << frames) - 1).max(1)
}

fn read_opengl_render_frame(
    runtime: &HostRuntime,
    pending: PendingHardwareFrame,
) -> Result<FrameBuffer> {
    let (gl, framebuffer) = {
        let state = runtime.hw_render_state.lock();
        let Some(gl) = state.frontend_gl_context.clone() else {
            return Err(anyhow!(
                "hardware-render frame requested without an active GL context"
            ));
        };
        let framebuffer = if force_default_gl_framebuffer(runtime) {
            None
        } else {
            let Some(target) = state.target.as_ref() else {
                return Err(anyhow!(
                    "hardware-render frame requested before framebuffer setup"
                ));
            };
            Some(target.framebuffer)
        };
        (gl, framebuffer)
    };

    let read_framebuffer = |framebuffer: Option<glow::Framebuffer>| -> Vec<u8> {
        let mut pixels = vec![0_u8; pending.width as usize * pending.height as usize * 4];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer);
            if framebuffer.is_some() {
                gl.read_buffer(glow::COLOR_ATTACHMENT0);
            }
            gl.read_pixels(
                0,
                0,
                pending.width as i32,
                pending.height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut pixels)),
            );
        }
        pixels
    };

    let mut pixels = unsafe {
        let previous_framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let previous_framebuffer =
            NonZeroU32::new(previous_framebuffer as u32).map(glow::NativeFramebuffer);

        let pixels = read_framebuffer(framebuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
        pixels
    };

    let (_, non_black_pixels, _) = summarize_rgba_debug_pixels(&pixels);
    if non_black_pixels == 0 {
        let fallback_pixels = unsafe {
            let previous_framebuffer = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
            let previous_framebuffer =
                NonZeroU32::new(previous_framebuffer as u32).map(glow::NativeFramebuffer);
            let pixels = read_framebuffer(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
            pixels
        };
        let (_, fallback_non_black_pixels, _) = summarize_rgba_debug_pixels(&fallback_pixels);
        if fallback_non_black_pixels > 0 {
            if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
                eprintln!(
                    "gl readback switching to default framebuffer fallback non_black_samples={fallback_non_black_pixels}/64"
                );
            }
            pixels = fallback_pixels;
        }
    }

    if pending.bottom_left_origin {
        let row_len = pending.width as usize * 4;
        let half_height = pending.height as usize / 2;
        for row in 0..half_height {
            let top = row * row_len;
            let bottom = (pending.height as usize - 1 - row) * row_len;
            for offset in 0..row_len {
                pixels.swap(top + offset, bottom + offset);
            }
        }
    }

    // Some OpenGL cores leave alpha undefined or zero, which makes the UI texture effectively
    // invisible even when RGB data is valid.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }

    if std::env::var_os("LIBRETRO_TRACE_GL_READBACK").is_some() {
        let (checksum, non_black_pixels, first_rgba) = summarize_rgba_debug_pixels(&pixels);
        eprintln!(
            "gl readback size={}x{} pitch={} checksum=0x{checksum:016x} non_black_samples={}/64 first_rgba={:02x},{:02x},{:02x},{:02x} bottom_left_origin={}",
            pending.width,
            pending.height,
            pending.width as usize * 4,
            non_black_pixels,
            first_rgba[0],
            first_rgba[1],
            first_rgba[2],
            first_rgba[3],
            pending.bottom_left_origin,
        );
    }

    Ok(FrameBuffer {
        width: pending.width,
        height: pending.height,
        pitch: pending.width as usize * 4,
        data: pixels,
        pixel_format: PixelFormat::Rgba8888,
    })
}

#[cfg(feature = "audio")]
fn set_audio_source_sample_rate(sample_rate: f64) {
    let _ = with_active_runtime(|runtime| {
        let mut state = runtime.audio_state.lock();
        state.source_sample_rate = sample_rate;
        state.resample_phase = 0.0;
        state.current_frame = None;
        state.next_frame = None;
    });
}

#[cfg(not(feature = "audio"))]
fn set_audio_source_sample_rate(_sample_rate: f64) {}

#[cfg(feature = "audio")]
fn set_audio_output_sample_rate(sample_rate: f64) {
    let _ = with_active_runtime(|runtime| {
        let mut state = runtime.audio_state.lock();
        state.output_sample_rate = sample_rate;
        state.resample_phase = 0.0;
        state.current_frame = None;
        state.next_frame = None;
    });
}

fn configure_environment_context(
    runtime: &HostRuntime,
    system_root: &Path,
    save_root: &Path,
    core_name: &str,
    backend: VideoBackendKind,
    emulation: &EmulationConfig,
) {
    {
        let mut context = runtime.environment_context.lock();
        context.system_dir = StableCStringBuffer::from_path(system_root);
        context.save_dir = StableCStringBuffer::from_path(save_root);
        context.variables = default_core_variables_for(core_name, backend, emulation);
        context.variables_updated = false;
        context.allow_vfs = !core_name.eq_ignore_ascii_case("fbneo");
        context.controller_info.clear();
        context.requested_hw_render = false;
        context.requested_hw_context_type = None;
        context.last_load_error = None;
        context.last_negotiation_interface = None;
        if vulkan_debug_enabled() && core_name.eq_ignore_ascii_case("parallel_n64") {
            let gfx = context
                .variables
                .get("parallel-n64-gfxplugin")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");
            let cpucore = context
                .variables
                .get("parallel-n64-cpucore")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<unset>");
            info!(
                target: "arcade_libretro::vulkan_debug",
                "configured parallel_n64 core vars backend={:?} parallel-n64-gfxplugin={} parallel-n64-cpucore={}",
                backend,
                gfx,
                cpucore
            );
        }
    }
    runtime.hw_render_state.lock().vulkan_negotiation = None;
}

fn core_has_embedded_software_video_fallback(core_name: &str) -> bool {
    matches!(core_name, "mupen64plus_next" | "parallel_n64")
}

unsafe fn load_api(library: &Library) -> Result<CoreApi> {
    let load = |name: &[u8]| -> Result<*const ()> {
        let symbol: Symbol<*const ()> = library.get(name).with_context(|| {
            format!(
                "missing required libretro symbol {}",
                String::from_utf8_lossy(name)
            )
        })?;
        Ok(*symbol)
    };
    let load_optional = |name: &[u8]| -> Option<*const ()> {
        let symbol: std::result::Result<Symbol<*const ()>, _> = library.get(name);
        symbol.ok().map(|symbol| *symbol)
    };

    Ok(CoreApi {
        set_environment: std::mem::transmute::<*const (), RetroSetEnvironment>(load(
            b"retro_set_environment",
        )?),
        set_video_refresh: std::mem::transmute::<*const (), RetroSetVideoRefresh>(load(
            b"retro_set_video_refresh",
        )?),
        set_audio_sample: std::mem::transmute::<*const (), RetroSetAudioSample>(load(
            b"retro_set_audio_sample",
        )?),
        set_audio_sample_batch: std::mem::transmute::<*const (), RetroSetAudioSampleBatch>(load(
            b"retro_set_audio_sample_batch",
        )?),
        set_input_poll: std::mem::transmute::<*const (), RetroSetInputPoll>(load(
            b"retro_set_input_poll",
        )?),
        set_input_state: std::mem::transmute::<*const (), RetroSetInputState>(load(
            b"retro_set_input_state",
        )?),
        set_controller_port_device: load_optional(b"retro_set_controller_port_device")
            .map(|symbol| std::mem::transmute::<*const (), RetroSetControllerPortDevice>(symbol)),
        init: std::mem::transmute::<*const (), RetroInit>(load(b"retro_init")?),
        deinit: std::mem::transmute::<*const (), RetroDeinit>(load(b"retro_deinit")?),
        api_version: std::mem::transmute::<*const (), RetroApiVersion>(load(b"retro_api_version")?),
        get_system_info: std::mem::transmute::<*const (), RetroGetSystemInfo>(load(
            b"retro_get_system_info",
        )?),
        get_system_av_info: std::mem::transmute::<*const (), RetroGetSystemAvInfo>(load(
            b"retro_get_system_av_info",
        )?),
        load_game: std::mem::transmute::<*const (), RetroLoadGame>(load(b"retro_load_game")?),
        unload_game: std::mem::transmute::<*const (), RetroUnloadGame>(load(b"retro_unload_game")?),
        reset: std::mem::transmute::<*const (), RetroReset>(load(b"retro_reset")?),
        run: std::mem::transmute::<*const (), RetroRun>(load(b"retro_run")?),
        serialize_size: std::mem::transmute::<*const (), RetroSerializeSize>(load(
            b"retro_serialize_size",
        )?),
        serialize: std::mem::transmute::<*const (), RetroSerialize>(load(b"retro_serialize")?),
        unserialize: std::mem::transmute::<*const (), RetroUnserialize>(load(
            b"retro_unserialize",
        )?),
    })
}

unsafe extern "C" fn retro_environment(cmd: u32, data: *mut c_void) -> bool {
    const RETRO_ENVIRONMENT_GET_CAN_DUPE: u32 = 3;
    const RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL: u32 = 8;
    const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: u32 = 9;
    const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: u32 = 10;
    const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: u32 = 11;
    const RETRO_ENVIRONMENT_SET_HW_RENDER: u32 = 14;
    const RETRO_ENVIRONMENT_GET_VARIABLE: u32 = 15;
    const RETRO_ENVIRONMENT_SET_VARIABLES: u32 = 16;
    const RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: u32 = 17;
    const RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME: u32 = 18;
    const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: u32 = 27;
    const RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: u32 = 31;
    const RETRO_ENVIRONMENT_SET_GEOMETRY: u32 = 37;
    const RETRO_ENVIRONMENT_SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE: u32 =
        43 | RETRO_ENVIRONMENT_EXPERIMENTAL;
    const RETRO_ENVIRONMENT_SET_CONTROLLER_INFO: u32 = 35;
    const RETRO_ENVIRONMENT_GET_LANGUAGE: u32 = 39;
    const RETRO_ENVIRONMENT_SET_SUPPORT_ACHIEVEMENTS: u32 = 42 | 0x10000;
    const RETRO_ENVIRONMENT_GET_VFS_INTERFACE: u32 = 45 | 0x10000;
    const RETRO_ENVIRONMENT_GET_INPUT_BITMASKS: u32 = 51 | 0x10000;
    const RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION: u32 = 59;
    const RETRO_ENVIRONMENT_SET_FASTFORWARDING_OVERRIDE: u32 = 64;
    const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK: u32 = 69;
    const RETRO_ENVIRONMENT_SET_VARIABLE: u32 = 70;

    const RETRO_PIXEL_FORMAT_0RGB1555: u32 = 0;
    const RETRO_PIXEL_FORMAT_XRGB8888: u32 = 1;
    const RETRO_PIXEL_FORMAT_RGB565: u32 = 2;
    const RETRO_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE_VULKAN: u32 = 0;

    if std::env::var_os("LIBRETRO_TRACE_ENV").is_some() {
        eprintln!("libretro env cmd={cmd}");
    }

    match cmd {
        RETRO_ENVIRONMENT_GET_CAN_DUPE => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut bool) = true;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL
        | RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS
        | RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME
        | RETRO_ENVIRONMENT_SET_SUPPORT_ACHIEVEMENTS
        | RETRO_ENVIRONMENT_SET_FASTFORWARDING_OVERRIDE
        | RETRO_ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK => true,
        RETRO_ENVIRONMENT_GET_HW_RENDER_INTERFACE => {
            if data.is_null() {
                record_runtime_load_error(
                    "core requested hardware-render interface with a null payload",
                );
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let state = runtime.hw_render_state.lock();
            if state.context_type != Some(RETRO_HW_CONTEXT_VULKAN) {
                record_runtime_load_error(
                    "core requested Vulkan render interface before Vulkan SET_HW_RENDER completed",
                );
                return false;
            }
            let Some(vulkan) = state.vulkan.as_ref() else {
                record_runtime_load_error(
                    "core requested Vulkan render interface before the frontend Vulkan backend was initialized",
                );
                return false;
            };
            unsafe {
                *(data as *mut *const RetroHwRenderInterface) = &vulkan.interface
                    as *const RetroHwRenderInterfaceVulkan
                    as *const RetroHwRenderInterface;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_HW_RENDER => {
            if data.is_null() {
                record_runtime_load_error(
                    "core requested hardware render with a null callback payload",
                );
                warn!("libretro hw-render: SET_HW_RENDER denied because callback data was null");
                return false;
            }
            let callback = unsafe { &mut *(data as *mut RetroHwRenderCallback) };
            let runtime = active_runtime();
            if let Some(runtime) = runtime.as_ref() {
                let chosen_backend = runtime.video_coordinator.lock().current_backend_kind();
                let mut context = runtime.environment_context.lock();
                context.requested_hw_render = true;
                context.requested_hw_context_type = Some(callback.context_type);
                if vulkan_debug_enabled() {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "SET_HW_RENDER requested context_type={} ({}) chosen_backend={:?}",
                        callback.context_type,
                        hw_context_type_name(callback.context_type),
                        chosen_backend
                    );
                }
                if chosen_backend == VideoBackendKind::Vulkan
                    && callback.context_type != RETRO_HW_CONTEXT_VULKAN
                {
                    warn!(
                        "libretro hw-render: core requested {} ({}) while host selected Vulkan; \
this core binary is likely built without Vulkan hardware-render support",
                        callback.context_type,
                        hw_context_type_name(callback.context_type)
                    );
                }
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            if callback.context_type == RETRO_HW_CONTEXT_VULKAN {
                let existing = {
                    let mut state = runtime.hw_render_state.lock();
                    state.vulkan.take()
                };
                if let Some(existing) = existing {
                    destroy_vulkan_interface_state(existing);
                }
            } else if !hardware_render_frontend_available() {
                record_runtime_load_error(
                    "core requested OpenGL hardware render, but no frontend GL context is available",
                );
                warn!("libretro hw-render: SET_HW_RENDER denied because no frontend GL context is available");
                return false;
            }
            if callback.context_type != RETRO_HW_CONTEXT_VULKAN
                && !hw_context_type_supported(callback.context_type)
            {
                let message = format!(
                    "core requested unsupported hardware context {} ({})",
                    callback.context_type,
                    hw_context_type_name(callback.context_type)
                );
                record_runtime_load_error(message.clone());
                warn!(
                    "libretro hw-render: rejecting unsupported context type {} ({})",
                    callback.context_type,
                    hw_context_type_name(callback.context_type)
                );
                return false;
            }
            if callback.context_type != RETRO_HW_CONTEXT_VULKAN {
                callback.get_current_framebuffer = Some(retro_hw_get_current_framebuffer);
                callback.get_proc_address = Some(retro_hw_get_proc_address);
            }
            let mut hw_state = runtime.hw_render_state.lock();
            hw_state.context_type = Some(callback.context_type);
            hw_state.callbacks = Some(HardwareRenderCallbacks {
                context_reset: callback.context_reset,
                context_destroy: callback.context_destroy,
                bottom_left_origin: callback.bottom_left_origin,
            });
            hw_state.context_ready = false;
            true
        }
        RETRO_ENVIRONMENT_SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE => {
            if data.is_null() {
                record_runtime_load_error(
                    "core requested hardware-render negotiation with a null payload",
                );
                return false;
            }
            let negotiation =
                unsafe { &*(data as *const RetroHwRenderContextNegotiationInterface) };
            let runtime = active_runtime();
            if let Some(runtime) = runtime.as_ref() {
                runtime
                    .environment_context
                    .lock()
                    .last_negotiation_interface =
                    Some((negotiation.interface_type, negotiation.interface_version));
            }
            if negotiation.interface_type == RETRO_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE_VULKAN {
                let Some(runtime) = runtime else {
                    return false;
                };
                let negotiation =
                    unsafe { &*(data as *const RetroHwRenderContextNegotiationInterfaceVulkan) };
                if vulkan_debug_enabled() {
                    info!(
                        target: "arcade_libretro::vulkan_debug",
                        "core requested Vulkan negotiation interface version={} get_application_info={} create_device={} destroy_device={}",
                        negotiation.interface_version,
                        negotiation.get_application_info.is_some(),
                        negotiation.create_device.is_some(),
                        negotiation.destroy_device.is_some()
                    );
                }
                let should_create = {
                    let mut state = runtime.hw_render_state.lock();
                    if state.context_type != Some(RETRO_HW_CONTEXT_VULKAN) {
                        return false;
                    }
                    state.vulkan_negotiation = Some(VulkanNegotiationCallbacks {
                        get_application_info: negotiation.get_application_info,
                        create_device: negotiation.create_device,
                        destroy_device: negotiation.destroy_device,
                    });
                    state.vulkan.is_some()
                };
                if should_create {
                    return match rebuild_vulkan_interface_state_for(&runtime) {
                        Ok(_) => true,
                        Err(err) => {
                            let message = format!(
                                "core requested Vulkan render negotiation, but frontend Vulkan backend initialization failed: {err}"
                            );
                            record_runtime_load_error(message.clone());
                            warn!("libretro hw-render: {message}");
                            false
                        }
                    };
                }
                return true;
            }
            let message = format!(
                "core requested unsupported render negotiation interface {} ({}) v{}",
                negotiation.interface_type,
                hw_render_interface_type_name(negotiation.interface_type),
                negotiation.interface_version
            );
            record_runtime_load_error(message.clone());
            warn!("libretro hw-render: {message}");
            false
        }
        RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let context = runtime.environment_context.lock();
            let out = data as *mut *const c_char;
            if let Some(path) = context.system_dir.as_ref() {
                unsafe {
                    *out = path.as_ptr();
                }
                true
            } else {
                false
            }
        }
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
            if data.is_null() {
                return false;
            }
            let requested = unsafe { *(data as *mut u32) };
            let pixel_format = match requested {
                RETRO_PIXEL_FORMAT_0RGB1555 => PixelFormat::Argb1555,
                RETRO_PIXEL_FORMAT_XRGB8888 => PixelFormat::Xrgb8888,
                RETRO_PIXEL_FORMAT_RGB565 => PixelFormat::Rgb565,
                _ => return false,
            };
            let Some(runtime) = active_runtime() else {
                return false;
            };
            runtime.callback_state.lock().pixel_format = pixel_format;
            true
        }
        RETRO_ENVIRONMENT_SET_GEOMETRY => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let geometry = unsafe { *(data as *const RetroGameGeometry) };
            runtime
                .video_coordinator
                .lock()
                .handle_geometry_update(&runtime, geometry);
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let variable = unsafe { &mut *(data as *mut RetroVariable) };
            if variable.key.is_null() {
                return false;
            }
            let Ok(key) = unsafe { CStr::from_ptr(variable.key) }.to_str() else {
                variable.value = std::ptr::null();
                return true;
            };
            let Some(runtime) = active_runtime() else {
                variable.value = std::ptr::null();
                return true;
            };
            let context = runtime.environment_context.lock();
            let value = context
                .variables
                .get(key)
                .map(|value| value.as_ptr())
                .unwrap_or(std::ptr::null());
            if vulkan_debug_enabled() && key.starts_with("parallel-n64-") {
                let printable = context
                    .variables
                    .get(key)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("<unset>");
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "GET_VARIABLE key={} value={}",
                    key,
                    printable
                );
            }
            if std::env::var_os("LIBRETRO_TRACE_VARIABLES").is_some()
                && key.starts_with("parallel-n64-")
            {
                let printable = context
                    .variables
                    .get(key)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("<unset>");
                eprintln!("env GET_VARIABLE key={key} value={printable}");
            }
            variable.value = value;
            true
        }
        RETRO_ENVIRONMENT_SET_VARIABLES => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            let mut variable = data as *const RetroVariable;
            while !variable.is_null() {
                let current = unsafe { &*variable };
                if current.key.is_null() {
                    break;
                }
                let Ok(key) = unsafe { CStr::from_ptr(current.key) }.to_str() else {
                    break;
                };
                let value = if current.value.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(current.value) }
                        .to_string_lossy()
                        .to_string()
                };
                store_default_variable(&mut context, key, &value);
                variable = unsafe { variable.add(1) };
            }
            context.variables_updated = true;
            true
        }
        RETRO_ENVIRONMENT_SET_CONTROLLER_INFO => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            context.controller_info = parse_controller_info(data as *const RetroControllerInfo);
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            unsafe {
                *(data as *mut bool) = context.variables_updated;
            }
            context.variables_updated = false;
            true
        }
        RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let context = runtime.environment_context.lock();
            let out = data as *mut *const c_char;
            if let Some(path) = context.save_dir.as_ref() {
                unsafe {
                    *out = path.as_ptr();
                }
                true
            } else {
                false
            }
        }
        RETRO_ENVIRONMENT_GET_LANGUAGE => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut u32) = 0;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_INPUT_BITMASKS => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut bool) = false;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_LOG_INTERFACE => {
            if data.is_null() {
                return false;
            }

            let callback = data as *mut RetroLogCallback;
            unsafe {
                (*callback).log = arcade_libretro_log_printf as *const c_void;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_VFS_INTERFACE => {
            if data.is_null() {
                return false;
            }
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let allow_vfs = runtime.environment_context.lock().allow_vfs;
            if !allow_vfs {
                vfs_trace("environment GET_VFS_INTERFACE refused for fbneo");
                return false;
            }
            if vfs_disabled() {
                vfs_trace("environment GET_VFS_INTERFACE refused by LIBRETRO_DISABLE_VFS");
                return false;
            }
            let info = unsafe { &mut *(data as *mut RetroVfsInterfaceInfo) };
            if info.required_interface_version > 2 {
                return false;
            }
            info.iface = &VFS_INTERFACE as *const RetroVfsInterface as *mut RetroVfsInterface;
            true
        }
        RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION => {
            if data.is_null() {
                return false;
            }
            unsafe {
                *(data as *mut u32) = 0;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let variable = unsafe { &*(data as *const RetroVariable) };
            if variable.key.is_null() || variable.value.is_null() {
                return false;
            }
            let Ok(key) = unsafe { CStr::from_ptr(variable.key) }.to_str() else {
                return false;
            };
            let value = unsafe { CStr::from_ptr(variable.value) }
                .to_string_lossy()
                .to_string();
            let Ok(value) = CString::new(value) else {
                return false;
            };
            let Some(runtime) = active_runtime() else {
                return false;
            };
            let mut context = runtime.environment_context.lock();
            if vulkan_debug_enabled() && key.starts_with("parallel-n64-") {
                info!(
                    target: "arcade_libretro::vulkan_debug",
                    "SET_VARIABLE key={} value={}",
                    key,
                    value.to_str().unwrap_or("<invalid>")
                );
            }
            if std::env::var_os("LIBRETRO_TRACE_VARIABLES").is_some()
                && key.starts_with("parallel-n64-")
            {
                eprintln!(
                    "env SET_VARIABLE key={key} value={}",
                    value.to_str().unwrap_or("<invalid>")
                );
            }
            context.variables.insert(key.to_string(), value);
            context.variables_updated = true;
            true
        }
        _ => {
            if std::env::var_os("LIBRETRO_TRACE_ENV").is_some() {
                eprintln!("Unhandled libretro environment cmd={cmd}");
            }
            false
        }
    }
}

unsafe extern "C" fn retro_hw_get_current_framebuffer() -> usize {
    with_active_runtime(|runtime| {
        if force_default_gl_framebuffer(runtime) {
            return 0;
        }
        runtime
            .hw_render_state
            .lock()
            .target
            .as_ref()
            .map(|target| target.framebuffer.0.get() as usize)
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

unsafe extern "C" fn retro_hw_get_proc_address(sym: *const c_char) -> *const c_void {
    if sym.is_null() {
        return std::ptr::null();
    }
    let symbol = unsafe { CStr::from_ptr(sym) };
    GL_PROC_LOADER.lock().get(symbol)
}

fn parse_default_variable_value(spec: &str) -> Option<&str> {
    let (_, values) = spec.split_once(';')?;
    let default = values.split('|').next()?.trim();
    if default.is_empty() {
        None
    } else {
        Some(default)
    }
}

fn parse_controller_info(mut info: *const RetroControllerInfo) -> Vec<Vec<u32>> {
    let mut ports = Vec::new();
    while !info.is_null() {
        let current = unsafe { &*info };
        if current.types.is_null() || current.num_types == 0 {
            break;
        }

        let mut supported = Vec::with_capacity(current.num_types as usize);
        for index in 0..current.num_types as usize {
            let description = unsafe { &*current.types.add(index) };
            supported.push(description.id);
        }
        ports.push(supported);
        info = unsafe { info.add(1) };
    }
    ports
}

fn preferred_controller_device(devices: Option<&[u32]>) -> u32 {
    const RETRO_DEVICE_JOYPAD: u32 = 1;
    const RETRO_DEVICE_MASK: u32 = 0xff;

    let Some(devices) = devices else {
        return RETRO_DEVICE_JOYPAD;
    };

    devices
        .iter()
        .copied()
        .find(|device| *device == RETRO_DEVICE_JOYPAD)
        .or_else(|| {
            devices
                .iter()
                .copied()
                .find(|device| *device != 0 && (*device & RETRO_DEVICE_MASK) == RETRO_DEVICE_JOYPAD)
        })
        .or_else(|| devices.iter().copied().find(|device| *device != 0))
        .unwrap_or(RETRO_DEVICE_JOYPAD)
}

fn with_vfs_handle<T>(
    stream: *mut RetroVfsFileHandle,
    f: impl FnOnce(&mut RetroVfsFileHandle) -> T,
) -> Option<T> {
    if stream.is_null() {
        None
    } else {
        Some(f(unsafe { &mut *stream }))
    }
}

fn vfs_trace_enabled() -> bool {
    *VFS_TRACE_ENABLED
}

fn vfs_seek_compat_enabled() -> bool {
    *VFS_SEEK_COMPAT_ENABLED
}

fn vfs_disabled() -> bool {
    *VFS_DISABLED
}

fn vfs_trace(message: impl AsRef<str>) {
    if vfs_trace_enabled() {
        eprintln!("[libretro-vfs] {}", message.as_ref());
    }
}

fn vfs_seek_whence_name(whence: i32) -> &'static str {
    match whence {
        RETRO_VFS_SEEK_POSITION_START => "start",
        RETRO_VFS_SEEK_POSITION_CURRENT => "current",
        RETRO_VFS_SEEK_POSITION_END => "end",
        _ => "unknown",
    }
}

fn is_archive_path(path: &CStr) -> bool {
    let path = path.to_string_lossy();
    path.ends_with(".zip") || path.ends_with(".7z")
}

unsafe extern "C" fn retro_vfs_get_path(stream: *mut RetroVfsFileHandle) -> *const c_char {
    with_vfs_handle(stream, |handle| handle.path.as_ptr()).unwrap_or(std::ptr::null())
}

unsafe extern "C" fn retro_vfs_open(
    path: *const c_char,
    mode: u32,
    _hints: u32,
) -> *mut RetroVfsFileHandle {
    if path.is_null() {
        return std::ptr::null_mut();
    }
    let path_str = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string();
    let mut options = OpenOptions::new();
    let wants_write = (mode & RETRO_VFS_FILE_ACCESS_WRITE) != 0;
    let wants_update = (mode & RETRO_VFS_FILE_ACCESS_UPDATE_EXISTING) != 0;
    let wants_read = (mode & RETRO_VFS_FILE_ACCESS_READ) != 0 || !wants_write || wants_update;
    options.read(wants_read);
    if wants_write || wants_update {
        options.write(true);
    }
    if wants_write && !wants_update {
        options.create(true).truncate(true);
    }

    let Ok(file) = options.open(&path_str) else {
        vfs_trace(format!(
            "open path={} mode=0x{mode:x} hints=0x{_hints:x} failed",
            path_str
        ));
        return std::ptr::null_mut();
    };
    let Ok(path) = CString::new(path_str) else {
        return std::ptr::null_mut();
    };
    let id = NEXT_VFS_HANDLE_ID.fetch_add(1, Ordering::Relaxed);
    vfs_trace(format!(
        "open id={id} path={} mode=0x{mode:x} hints=0x{_hints:x} read={} write={} update={}",
        path.to_string_lossy(),
        wants_read,
        wants_write,
        wants_update
    ));
    Box::into_raw(Box::new(RetroVfsFileHandle { id, file, path }))
}

unsafe extern "C" fn retro_vfs_close(stream: *mut RetroVfsFileHandle) -> i32 {
    if stream.is_null() {
        return -1;
    }
    unsafe {
        drop(Box::from_raw(stream));
    }
    0
}

unsafe extern "C" fn retro_vfs_size(stream: *mut RetroVfsFileHandle) -> i64 {
    with_vfs_handle(stream, |handle| {
        let result = handle
            .file
            .metadata()
            .map(|meta| meta.len() as i64)
            .unwrap_or(-1);
        vfs_trace(format!(
            "size id={} path={} -> {}",
            handle.id,
            handle.path.to_string_lossy(),
            result
        ));
        result
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_tell(stream: *mut RetroVfsFileHandle) -> i64 {
    with_vfs_handle(stream, |handle| {
        handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1)
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_seek(
    stream: *mut RetroVfsFileHandle,
    offset: i64,
    whence: i32,
) -> i64 {
    with_vfs_handle(stream, |handle| {
        let before = handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        let seek_from = match whence {
            RETRO_VFS_SEEK_POSITION_START => {
                if offset < 0 {
                    if vfs_seek_compat_enabled() && offset == -1 && is_archive_path(&handle.path) {
                        let result = handle
                            .file
                            .seek(SeekFrom::Start(0))
                            .map(|pos| pos as i64)
                            .unwrap_or(-1);
                        vfs_trace(format!(
                            "seek-compat id={} path={} whence={} offset={} before={} -> {}",
                            handle.id,
                            handle.path.to_string_lossy(),
                            vfs_seek_whence_name(whence),
                            offset,
                            before,
                            result
                        ));
                        return result;
                    }
                    vfs_trace(format!(
                        "seek id={} path={} whence={} offset={} before={} -> -1",
                        handle.id,
                        handle.path.to_string_lossy(),
                        vfs_seek_whence_name(whence),
                        offset,
                        before
                    ));
                    return -1;
                }
                SeekFrom::Start(offset as u64)
            }
            RETRO_VFS_SEEK_POSITION_CURRENT => SeekFrom::Current(offset),
            RETRO_VFS_SEEK_POSITION_END => SeekFrom::End(offset),
            _ => {
                vfs_trace(format!(
                    "seek id={} path={} whence={} offset={} before={} -> -1",
                    handle.id,
                    handle.path.to_string_lossy(),
                    vfs_seek_whence_name(whence),
                    offset,
                    before
                ));
                return -1;
            }
        };
        let result = handle
            .file
            .seek(seek_from)
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        vfs_trace(format!(
            "seek id={} path={} whence={} offset={} before={} -> {}",
            handle.id,
            handle.path.to_string_lossy(),
            vfs_seek_whence_name(whence),
            offset,
            before,
            result
        ));
        result
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_read(
    stream: *mut RetroVfsFileHandle,
    buffer: *mut c_void,
    len: u64,
) -> i64 {
    if buffer.is_null() {
        return -1;
    }
    with_vfs_handle(stream, |handle| {
        let before = handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        let slice = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, len as usize) };
        let result = handle
            .file
            .read(slice)
            .map(|read| read as i64)
            .unwrap_or(-1);
        let after = handle
            .file
            .stream_position()
            .map(|pos| pos as i64)
            .unwrap_or(-1);
        vfs_trace(format!(
            "read id={} path={} len={} before={} -> {} after={}",
            handle.id,
            handle.path.to_string_lossy(),
            len,
            before,
            result,
            after
        ));
        result
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_write(
    stream: *mut RetroVfsFileHandle,
    buffer: *const c_void,
    len: u64,
) -> i64 {
    if buffer.is_null() {
        return -1;
    }
    with_vfs_handle(stream, |handle| {
        let slice = unsafe { std::slice::from_raw_parts(buffer as *const u8, len as usize) };
        handle
            .file
            .write(slice)
            .map(|written| written as i64)
            .unwrap_or(-1)
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_flush(stream: *mut RetroVfsFileHandle) -> i32 {
    with_vfs_handle(stream, |handle| {
        handle.file.flush().map(|_| 0).unwrap_or(-1)
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_remove(path: *const c_char) -> i32 {
    if path.is_null() {
        return -1;
    }
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .to_string();
    fs::remove_file(path).map(|_| 0).unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_rename(old_path: *const c_char, new_path: *const c_char) -> i32 {
    if old_path.is_null() || new_path.is_null() {
        return -1;
    }
    let old_path = unsafe { CStr::from_ptr(old_path) }
        .to_string_lossy()
        .to_string();
    let new_path = unsafe { CStr::from_ptr(new_path) }
        .to_string_lossy()
        .to_string();
    fs::rename(old_path, new_path).map(|_| 0).unwrap_or(-1)
}

unsafe extern "C" fn retro_vfs_truncate(stream: *mut RetroVfsFileHandle, length: i64) -> i64 {
    if length < 0 {
        return -1;
    }
    with_vfs_handle(stream, |handle| {
        handle.file.set_len(length as u64).map(|_| 0).unwrap_or(-1)
    })
    .unwrap_or(-1)
}

unsafe extern "C" fn retro_video_refresh(
    data: *const c_void,
    width: u32,
    height: u32,
    pitch: usize,
) {
    const RETRO_HW_FRAME_BUFFER_VALID: usize = usize::MAX;

    if std::env::var_os("LIBRETRO_TRACE_VIDEO").is_some() {
        eprintln!(
            "libretro video data={:p} width={} height={} pitch={}",
            data, width, height, pitch
        );
    }

    if width == 0 || height == 0 {
        return;
    }

    if data as usize == RETRO_HW_FRAME_BUFFER_VALID {
        let Some(runtime) = active_runtime() else {
            return;
        };
        let bottom_left_origin = runtime
            .hw_render_state
            .lock()
            .callbacks
            .map(|callbacks| callbacks.bottom_left_origin)
            .unwrap_or(true);
        let mut state = runtime.callback_state.lock();
        state.latest_hw_frame = Some(PendingHardwareFrame {
            width,
            height,
            bottom_left_origin,
        });
        return;
    }

    if data.is_null() || pitch == 0 {
        return;
    }

    let byte_len = pitch.saturating_mul(height as usize);
    let src = unsafe { std::slice::from_raw_parts(data as *const u8, byte_len) };
    let Some(runtime) = active_runtime() else {
        return;
    };
    let mut state = runtime.callback_state.lock();
    let pixel_format = state.pixel_format;
    state.latest_frame = Some(FrameBuffer {
        width,
        height,
        pitch,
        data: src.to_vec(),
        pixel_format,
    });
}

unsafe extern "C" fn retro_audio_sample(left: i16, right: i16) {
    push_audio_samples(&[left, right]);
}

unsafe extern "C" fn retro_audio_sample_batch(data: *const i16, frames: usize) -> usize {
    if data.is_null() || frames == 0 {
        return 0;
    }

    let sample_count = frames.saturating_mul(2);
    let samples = unsafe { std::slice::from_raw_parts(data, sample_count) };
    push_audio_samples(samples);
    frames
}

unsafe extern "C" fn retro_input_poll() {}

unsafe extern "C" fn retro_input_state(port: u32, device: u32, index: u32, id: u32) -> i16 {
    const RETRO_DEVICE_MASK: u32 = 0xff;

    let Some(runtime) = active_runtime() else {
        return 0;
    };
    let state = runtime.callback_state.lock();
    if let Some(value) = state.input_state.get(&(port, device, index, id)) {
        return *value;
    }

    let base_device = device & RETRO_DEVICE_MASK;
    if base_device != device {
        return state
            .input_state
            .get(&(port, base_device, index, id))
            .copied()
            .unwrap_or(0);
    }

    0
}

fn push_audio_samples(samples: &[i16]) {
    if samples.is_empty() {
        return;
    }

    let Some(runtime) = active_runtime() else {
        return;
    };
    let mut state = runtime.audio_state.lock();
    let queue = &mut state.samples;
    let overflow = queue
        .len()
        .saturating_add(samples.len())
        .saturating_sub(MAX_AUDIO_SAMPLES);
    for _ in 0..overflow {
        let _ = queue.pop_front();
    }
    queue.extend(samples.iter().copied());
}

#[cfg(feature = "audio")]
fn audio_frames_for_latency(sample_rate_hz: f64, latency_secs: f64) -> usize {
    if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 || latency_secs <= 0.0 {
        return 0;
    }

    (sample_rate_hz * latency_secs).round() as usize
}

#[cfg(feature = "audio")]
fn trim_audio_queue_for_latency(state: &mut AudioState) {
    let output_rate = state.output_sample_rate;
    if output_rate <= 0.0 {
        return;
    }

    let target_frames = audio_frames_for_latency(output_rate, AUDIO_TARGET_LATENCY_SECS);
    let max_frames =
        audio_frames_for_latency(output_rate, AUDIO_MAX_LATENCY_SECS).max(target_frames + 1);
    let queued_frames = state.samples.len() / 2;
    if queued_frames <= max_frames {
        return;
    }

    let frames_to_drop = queued_frames.saturating_sub(target_frames);
    let samples_to_drop = frames_to_drop.saturating_mul(2).min(state.samples.len());
    state.samples.drain(..samples_to_drop);
    state.current_frame = None;
    state.next_frame = None;
    state.resample_phase = 0.0;
}

#[cfg(feature = "audio")]
fn pop_stereo_frame(state: &mut AudioState) -> (i16, i16) {
    let left = state.samples.pop_front().unwrap_or(0);
    let right = state.samples.pop_front().unwrap_or(0);
    (left, right)
}

#[cfg(feature = "audio")]
fn pop_stereo_i16_with_state(state: &mut AudioState) -> (i16, i16) {
    let source_rate = state.source_sample_rate;
    let output_rate = state.output_sample_rate;

    if source_rate <= 0.0 || output_rate <= 0.0 {
        state.current_frame = None;
        state.next_frame = None;
        state.resample_phase = 0.0;
        return pop_stereo_frame(state);
    }

    let target_queue_frames =
        audio_frames_for_latency(output_rate, AUDIO_TARGET_LATENCY_SECS) as f64;
    let queued_frames = (state.samples.len() / 2) as f64;
    let queue_error = if target_queue_frames > 0.0 {
        ((queued_frames - target_queue_frames) / target_queue_frames).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    let base_ratio = source_rate / output_rate;
    let mut ratio = base_ratio * (1.0 + queue_error * AUDIO_RESAMPLE_QUEUE_CORRECTION);
    let ratio_min = base_ratio * (1.0 - AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT);
    let ratio_max = base_ratio * (1.0 + AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT);
    ratio = ratio.clamp(ratio_min, ratio_max);

    let near_unity_ratio = (ratio - 1.0).abs() < 0.0005;
    if near_unity_ratio && state.current_frame.is_none() && state.next_frame.is_none() {
        state.resample_phase = 0.0;
        return pop_stereo_frame(state);
    }

    if state.current_frame.is_none() {
        state.current_frame = Some(pop_stereo_frame(state));
    }
    if state.next_frame.is_none() {
        state.next_frame = Some(pop_stereo_frame(state));
    }

    let (current_left, current_right) = state.current_frame.unwrap_or((0, 0));
    let (next_left, next_right) = state.next_frame.unwrap_or((0, 0));
    let frac = state.resample_phase as f32;

    let left = current_left as f32 + (next_left as f32 - current_left as f32) * frac;
    let right = current_right as f32 + (next_right as f32 - current_right as f32) * frac;

    state.resample_phase += ratio;
    while state.resample_phase >= 1.0 {
        state.current_frame = state.next_frame.take();
        state.next_frame = Some(pop_stereo_frame(state));
        state.resample_phase -= 1.0;
    }

    (left.round() as i16, right.round() as i16)
}

#[cfg(feature = "audio")]
fn write_output_i16(output: &mut [i16], channels: usize) {
    if channels == 0 {
        return;
    }

    let Some(runtime) = active_runtime() else {
        output.fill(0);
        return;
    };
    let mut state = runtime.audio_state.lock();
    trim_audio_queue_for_latency(&mut state);
    for frame in output.chunks_mut(channels) {
        let (left, right) = pop_stereo_i16_with_state(&mut state);
        if channels == 1 {
            frame[0] = ((left as i32 + right as i32) / 2) as i16;
            continue;
        }

        frame[0] = left;
        frame[1] = right;
        for sample in &mut frame[2..] {
            *sample = 0;
        }
    }
}

#[cfg(feature = "audio")]
fn write_output_u16(output: &mut [u16], channels: usize) {
    if channels == 0 {
        return;
    }

    let Some(runtime) = active_runtime() else {
        output.fill(i16::MAX as u16);
        return;
    };
    let mut state = runtime.audio_state.lock();
    trim_audio_queue_for_latency(&mut state);
    for frame in output.chunks_mut(channels) {
        let (left, right) = pop_stereo_i16_with_state(&mut state);
        if channels == 1 {
            let mixed = ((left as i32 + right as i32) / 2) as i16;
            frame[0] = (mixed as i32 + i16::MAX as i32 + 1) as u16;
            continue;
        }

        frame[0] = (left as i32 + i16::MAX as i32 + 1) as u16;
        frame[1] = (right as i32 + i16::MAX as i32 + 1) as u16;
        for sample in &mut frame[2..] {
            *sample = i16::MAX as u16;
        }
    }
}

#[cfg(feature = "audio")]
fn write_output_f32(output: &mut [f32], channels: usize) {
    if channels == 0 {
        return;
    }

    let Some(runtime) = active_runtime() else {
        output.fill(0.0);
        return;
    };
    let mut state = runtime.audio_state.lock();
    trim_audio_queue_for_latency(&mut state);
    for frame in output.chunks_mut(channels) {
        let (left, right) = pop_stereo_i16_with_state(&mut state);
        if channels == 1 {
            frame[0] = (left as f32 + right as f32) / 65536.0;
            continue;
        }

        frame[0] = left as f32 / 32768.0;
        frame[1] = right as f32 / 32768.0;
        for sample in &mut frame[2..] {
            *sample = 0.0;
        }
    }
}
const CORE_PATH_BUFFER_LEN: usize = 4096;
#[derive(Debug)]
struct StableCStringBuffer {
    bytes: Box<[u8]>,
}

impl StableCStringBuffer {
    #[cfg(unix)]
    fn from_path(path: &Path) -> Option<Self> {
        let raw = path.as_os_str().as_bytes();
        if raw.contains(&0) || raw.len() + 1 > CORE_PATH_BUFFER_LEN {
            return None;
        }

        let mut bytes = vec![0_u8; CORE_PATH_BUFFER_LEN];
        bytes[..raw.len()].copy_from_slice(raw);
        Some(Self {
            bytes: bytes.into_boxed_slice(),
        })
    }

    #[cfg(not(unix))]
    fn from_path(path: &Path) -> Option<Self> {
        let value = path.to_string_lossy();
        if value.as_bytes().contains(&0) || value.len() + 1 > CORE_PATH_BUFFER_LEN {
            return None;
        }

        let mut bytes = vec![0_u8; CORE_PATH_BUFFER_LEN];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self {
            bytes: bytes.into_boxed_slice(),
        })
    }

    fn as_ptr(&self) -> *const c_char {
        self.bytes.as_ptr() as *const c_char
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn controller_device_prefers_joypad_over_none() {
        assert_eq!(preferred_controller_device(Some(&[0, 1, 257])), 1);
    }

    #[test]
    fn controller_device_prefers_joypad_family_subtype_when_plain_joypad_is_missing() {
        assert_eq!(preferred_controller_device(Some(&[0, 257, 513])), 257);
    }

    #[test]
    fn controller_device_falls_back_to_joypad_when_only_none_is_advertised() {
        assert_eq!(preferred_controller_device(Some(&[0])), 1);
    }

    #[test]
    fn resolve_core_candidates_match_platform_convention() {
        let dir = tempdir().expect("tempdir");
        let host = LibretroHost::new(
            dir.path().join("cores"),
            dir.path().join("bios"),
            dir.path().join("saves"),
            EmulationConfig::default(),
        );

        let candidates = host.resolve_core_candidates("fceumm");
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].parent(), Some(host.core_root()));

        #[cfg(target_os = "windows")]
        assert_eq!(
            candidates
                .iter()
                .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["fceumm_libretro.dll", "fceumm.dll"]
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            candidates
                .iter()
                .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["fceumm_libretro.dylib"]
        );

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        assert_eq!(
            candidates
                .iter()
                .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["fceumm_libretro.so"]
        );
    }

    #[test]
    fn resolve_core_path_prefers_existing_candidate() {
        let dir = tempdir().expect("tempdir");
        let core_root = dir.path().join("cores");
        fs::create_dir_all(&core_root).expect("create core root");
        let host = LibretroHost::new(
            core_root.clone(),
            dir.path().join("bios"),
            dir.path().join("saves"),
            EmulationConfig::default(),
        );

        let candidates = host.resolve_core_candidates("snes9x");
        let preferred_existing = candidates.last().cloned().expect("at least one candidate");
        fs::write(&preferred_existing, b"core").expect("write core file");

        assert_eq!(host.resolve_core_path("snes9x"), preferred_existing);
    }

    #[test]
    fn parallel_n64_preflight_accepts_frontend_windowing_probe() {
        let runtime = HostRuntime::default();
        runtime
            .video_coordinator
            .lock()
            .apply_frontend_capabilities(
                &runtime,
                FrontendCapabilities {
                    renderer_name: Some(String::from("eframe_glow")),
                    gl_context: None,
                    window_handle_kind: Some(String::from("Xlib")),
                    display_handle_kind: Some(String::from("Xlib")),
                },
            );
        runtime
            .video_coordinator
            .lock()
            .plan_session(VideoSessionInfo {
                core_name: String::from("parallel_n64"),
                requires_hw_render: true,
                requested_hw_context_type: None,
            });

        assert!(hardware_render_preflight_available_for_core(
            &runtime,
            "parallel_n64"
        ));

        let other_runtime = HostRuntime::default();
        other_runtime
            .video_coordinator
            .lock()
            .apply_frontend_capabilities(
                &other_runtime,
                FrontendCapabilities {
                    renderer_name: Some(String::from("eframe_non_gl")),
                    gl_context: None,
                    window_handle_kind: Some(String::from("Xlib")),
                    display_handle_kind: Some(String::from("Xlib")),
                },
            );
        assert!(!hardware_render_preflight_available_for_core(
            &other_runtime,
            "beetle_psx_hw"
        ));
    }

    #[test]
    fn vulkan_unimplemented_message_includes_frontend_windowing_details() {
        let runtime = HostRuntime::default();
        runtime
            .video_coordinator
            .lock()
            .apply_frontend_capabilities(
                &runtime,
                FrontendCapabilities {
                    renderer_name: Some(String::from("eframe_glow")),
                    gl_context: None,
                    window_handle_kind: Some(String::from("Xlib")),
                    display_handle_kind: Some(String::from("Xlib")),
                },
            );

        let message = vulkan_unimplemented_message(Some(&runtime));

        assert!(message.contains("renderer=eframe_glow"));
        assert!(message.contains("window_handle=Xlib"));
        assert!(message.contains("display_handle=Xlib"));
        assert!(
            message.contains("basic Vulkan bootstrap")
                || message.contains("RETRO_ENVIRONMENT_GET_HW_RENDER_INTERFACE")
        );
    }

    #[test]
    fn active_runtime_registration_roundtrip() {
        *ACTIVE_RUNTIME.lock() = None;
        let runtime = Arc::new(HostRuntime::default());

        assert!(active_runtime().is_none());
        register_active_runtime(&runtime);
        assert!(with_active_runtime(|_| true).unwrap_or(false));

        clear_active_runtime(&runtime);
        assert!(active_runtime().is_none());
    }

    #[test]
    fn frame_callback_writes_into_active_runtime() {
        *ACTIVE_RUNTIME.lock() = None;
        let runtime = Arc::new(HostRuntime::default());
        register_active_runtime(&runtime);
        runtime.callback_state.lock().pixel_format = PixelFormat::Rgba8888;

        let pixels = [1_u8, 2, 3, 4];
        unsafe {
            retro_video_refresh(pixels.as_ptr() as *const c_void, 1, 1, 4);
        }

        let frame = runtime
            .callback_state
            .lock()
            .latest_frame
            .take()
            .expect("frame written");
        assert_eq!(frame.width, 1);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.data, pixels);
        assert_eq!(frame.pixel_format, PixelFormat::Rgba8888);

        clear_active_runtime(&runtime);
    }
}
