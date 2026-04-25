mod audio;
mod callbacks;
mod content;
mod core_variables;
mod diagnostics;
mod hardware_render;
mod vfs;
mod video;
mod vulkan_render;

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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use anyhow::{anyhow, Context, Result};
use arcade_domain::{resolve_core, EmulationConfig, N64CpuCoreMode};
use ash::vk;
#[cfg(feature = "audio")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glow::HasContext;
use libloading::{Library, Symbol};
#[cfg(target_os = "macos")]
use objc::runtime::{Object, NO, YES};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tempfile::TempDir;
use thiserror::Error;
use tracing::{info, warn};

use self::content::{
    build_game_info, inspect_core_requirements, prepare_game_content, prepare_launch_session,
    strategy_uses_data, LaunchSession, LoadGameStrategy,
};
use self::core_variables::{
    apply_core_runtime_env_defaults, default_core_variables_for, store_default_variable,
};
use self::diagnostics::*;
use self::video::{FrameDelivery, VideoCoordinator, VideoSessionInfo};
use self::{audio::*, callbacks::*, hardware_render::*, vfs::*, vulkan_render::*};

pub use self::video::{FrontendCapabilities, VideoBackendKind};

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioQueueSnapshot {
    pub queue_frames: usize,
    pub target_frames: usize,
    pub max_frames: usize,
    pub source_rate: f64,
    pub output_rate: f64,
    pub trimmed_total_frames: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PixelFormat {
    #[default]
    Argb1555,
    Xrgb8888,
    Rgb565,
    Rgba8888,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VulkanPresentTestMetrics {
    pub backend_kind: VideoBackendKind,
    pub external_window_created: bool,
    pub queue_present_attempts: u64,
    pub queue_present_successes: u64,
    pub external_present_deliveries: u64,
    pub cpu_frame_deliveries: u64,
    pub source_non_black_seen: bool,
    pub swapchain_non_black_seen: bool,
    pub non_tiny_source_frame_seen: bool,
    pub max_consecutive_tiny_source_frames: u64,
}

impl Default for VulkanPresentTestMetrics {
    fn default() -> Self {
        Self {
            backend_kind: VideoBackendKind::Software,
            external_window_created: false,
            queue_present_attempts: 0,
            queue_present_successes: 0,
            external_present_deliveries: 0,
            cpu_frame_deliveries: 0,
            source_non_black_seen: false,
            swapchain_non_black_seen: false,
            non_tiny_source_frame_seen: false,
            max_consecutive_tiny_source_frames: 0,
        }
    }
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

#[derive(Debug)]
struct AudioState {
    samples: VecDeque<i16>,
    source_sample_rate: f64,
    output_sample_rate: f64,
    resample_phase: f64,
    current_frame: Option<(i16, i16)>,
    next_frame: Option<(i16, i16)>,
    target_latency_secs: f64,
    max_latency_secs: f64,
    resample_queue_correction: f64,
    resample_ratio_drift_limit: f64,
    trim_to_target_on_overflow: bool,
    drop_excess_silence_when_buffered: bool,
    produced_samples_total: u64,
    consumed_samples_total: u64,
    trimmed_samples_total: u64,
    produced_samples_window: u64,
    consumed_samples_window: u64,
    trimmed_samples_window: u64,
    flow_window_started_at: Option<std::time::Instant>,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
            source_sample_rate: 0.0,
            output_sample_rate: 0.0,
            resample_phase: 0.0,
            current_frame: None,
            next_frame: None,
            target_latency_secs: DEFAULT_AUDIO_TARGET_LATENCY_SECS,
            max_latency_secs: DEFAULT_AUDIO_MAX_LATENCY_SECS,
            resample_queue_correction: DEFAULT_AUDIO_RESAMPLE_QUEUE_CORRECTION,
            resample_ratio_drift_limit: DEFAULT_AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT,
            trim_to_target_on_overflow: true,
            drop_excess_silence_when_buffered: false,
            produced_samples_total: 0,
            consumed_samples_total: 0,
            trimmed_samples_total: 0,
            produced_samples_window: 0,
            consumed_samples_window: 0,
            trimmed_samples_window: 0,
            flow_window_started_at: None,
        }
    }
}
// Absolute safety cap for the inter-thread audio queue (stereo samples, not frames).
// Keep this comfortably above the largest per-core max-latency profile so push-side
// overflow does not become the first drop mechanism.
const MAX_AUDIO_SAMPLES: usize = 98_304;
#[cfg(feature = "audio")]
// Default target latency for most cores. Individual cores can override this
// profile at runtime when they need more tolerant pacing behavior.
const DEFAULT_AUDIO_TARGET_LATENCY_SECS: f64 = 0.100;
#[cfg(feature = "audio")]
const DEFAULT_AUDIO_MAX_LATENCY_SECS: f64 = 0.300;
#[cfg(feature = "audio")]
const DEFAULT_AUDIO_RESAMPLE_QUEUE_CORRECTION: f64 = 0.05;
#[cfg(feature = "audio")]
const DEFAULT_AUDIO_RESAMPLE_RATIO_DRIFT_LIMIT: f64 = 0.05;
const VULKAN_FALLBACK_SYNC_FRAMES: u32 = 3;
const DISPLAY_FPS_TOLERANCE: f64 = 2.0;
const COMMON_DISPLAY_FPS: &[f64] = &[60.0, 50.0, 30.0];
/// Function pointer type for the libretro keyboard event callback.
/// Signature: `void callback(bool down, unsigned keycode, uint32_t character, uint16_t key_modifiers)`
type RetroKeyboardEventFn =
    unsafe extern "C" fn(down: bool, keycode: u32, character: u32, key_modifiers: u16);
/// Function pointer type for the libretro audio callback pull model.
/// Signature: `void callback(void)`
type RetroAudioCallbackFn = unsafe extern "C" fn();
/// Function pointer type to enable/disable the audio callback.
/// Signature: `void set_state(bool enabled)`
type RetroAudioSetStateCallbackFn = unsafe extern "C" fn(enabled: bool);
/// Function pointer type for the libretro frame-time callback.
/// Signature: `void callback(int64_t usec)`
type RetroFrameTimeCallbackFn = unsafe extern "C" fn(usec: i64);

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
    loaded_core_name: Option<String>,
    loaded_backend: Option<VideoBackendKind>,
    keyboard_event_cb: Option<RetroKeyboardEventFn>,
    audio_callback: Option<RetroAudioCallbackFn>,
    audio_set_state_callback: Option<RetroAudioSetStateCallbackFn>,
    audio_callback_enabled: bool,
    frame_time_callback: Option<RetroFrameTimeCallbackFn>,
    frame_time_reference_usecs: i64,
    frame_time_last_instant: Option<std::time::Instant>,
    runtime_video_fps: Option<f64>,
    run_fps_probe_start: Option<std::time::Instant>,
    run_fps_probe_frames: u32,
    run_fps_probe_logged: bool,
}

#[derive(Clone, Copy)]
struct HardwareRenderCallbacks {
    context_reset: Option<RetroHwContextResetFn>,
    context_destroy: Option<RetroHwContextResetFn>,
    bottom_left_origin: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PlayTextureSignature {
    checksum: u64,
    non_black_samples: u16,
    luminance_range: u16,
    uniform: bool,
    first_rgb: [u8; 3],
    center_rgb: [u8; 3],
}

struct HardwareRenderTarget {
    framebuffer: glow::Framebuffer,
    color_texture: glow::Texture,
    depth_stencil: glow::Renderbuffer,
    width: u32,
    height: u32,
    /// FBO created lazily in the emu thread's GL context wrapping the shared color_texture.
    /// FBOs are not shared between GL contexts, but textures are — so when a core (e.g.
    /// mupen64plus-next/GLideN64) renders on a background emu thread using a shared context,
    /// this FBO is what get_current_framebuffer() returns.  The main-context FBO above is
    /// used for readback (it wraps the same color_texture via the shared texture namespace).
    emu_ctx_framebuffer: Option<glow::NativeFramebuffer>,
    /// Discovered game-frame texture handle.  When a core (mupen64plus-next/GLideN64) renders
    /// into its own internal texture via a shared GL context, textures from that context are
    /// visible here (textures ARE shared; FBOs are NOT).  We scan texture handles once on the
    /// first black frame, find the one with game content, cache it here, and read from it via
    /// a temporary FBO on every subsequent frame.
    emu_game_texture: Option<glow::NativeTexture>,
    /// Heuristic score for the currently cached game texture. Lower is better.
    play_texture_cache_score: f32,
    /// Consecutive readback frames with no sampled non-black signal. Used by play-core
    /// warmup gating so we wait briefly for core-linked FBO output before scan fallback.
    play_blank_frame_streak: u32,
    /// Incrementing frame counter for Play GL texture-cache health checks.
    play_texture_frame_counter: u64,
    /// Last frame signature seen in the Play texture cache path.
    play_texture_last_signature: Option<PlayTextureSignature>,
    /// Repeated near-static signature streak used to detect stale/intermediate textures.
    play_texture_stale_signature_streak: u32,
    /// Frame counter value when texture scan/revalidation was last attempted.
    play_texture_last_scan_frame: u64,
    /// Frame counter value when the cached texture last switched.
    play_texture_last_switch_frame: u64,
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

struct VulkanReadbackState {
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    staging_capacity: usize,
}

#[cfg(target_os = "macos")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct NSSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

#[cfg(target_os = "macos")]
unsafe impl objc::Encode for NSPoint {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGPoint=dd}") }
    }
}

#[cfg(target_os = "macos")]
unsafe impl objc::Encode for NSSize {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGSize=dd}") }
    }
}

#[cfg(target_os = "macos")]
unsafe impl objc::Encode for NSRect {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

#[derive(Clone, Copy)]
enum ExternalVulkanWindowDescriptor {
    Metal {
        layer: *const c_void,
        width: u32,
        height: u32,
    },
}

impl ExternalVulkanWindowDescriptor {
    fn size(self) -> (u32, u32) {
        match self {
            Self::Metal { width, height, .. } => (width, height),
        }
    }
}

struct ExternalVulkanWindow {
    ns_window: *mut Object,
    metal_layer: *mut Object,
    width: u32,
    height: u32,
    visible: bool,
}

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
    /// Pointer-sized identity of the eframe GL context at the time it was registered.
    /// Used to detect when a core's retro_video_refresh callback is invoked from a
    /// different (core-owned) context.  0 = not yet captured.
    eframe_gl_ctx_id: usize,
}

#[derive(Default)]
struct HostRuntime {
    callback_state: Mutex<CallbackState>,
    audio_state: Mutex<AudioState>,
    environment_context: Mutex<EnvironmentContext>,
    video_coordinator: Mutex<VideoCoordinator>,
    hw_render_state: Mutex<HardwareRenderState>,
    vulkan_present_metrics: Mutex<VulkanPresentMetricsState>,
    shutdown_requested: AtomicBool,
}

#[derive(Debug, Clone, Default)]
struct VulkanPresentMetricsState {
    queue_present_attempts: u64,
    queue_present_successes: u64,
    external_present_deliveries: u64,
    cpu_frame_deliveries: u64,
    source_non_black_seen: bool,
    swapchain_non_black_seen: bool,
    non_tiny_source_frame_seen: bool,
    consecutive_tiny_source_frames: u64,
    max_consecutive_tiny_source_frames: u64,
    source_sample_checks: u64,
    swapchain_sample_checks: u64,
    black_fail_fast_error: Option<String>,
    tiny_frame_fail_fast_error: Option<String>,
}

#[derive(Clone)]
struct FrontendGlStateSnapshot {
    active_texture: i32,
    texture_units: Vec<FrontendGlTextureUnitState>,
    current_program: Option<glow::Program>,
    vertex_array: Option<glow::VertexArray>,
    array_buffer: Option<glow::Buffer>,
    element_array_buffer: Option<glow::Buffer>,
    renderbuffer: Option<glow::Renderbuffer>,
    framebuffer: Option<glow::Framebuffer>,
    read_framebuffer: Option<glow::Framebuffer>,
    draw_framebuffer: Option<glow::Framebuffer>,
    unpack_alignment: i32,
    pack_alignment: i32,
    unpack_row_length: i32,
    pack_row_length: i32,
    viewport: [i32; 4],
    scissor_box: [i32; 4],
    blend_enabled: bool,
    cull_face_enabled: bool,
    depth_test_enabled: bool,
    scissor_test_enabled: bool,
    stencil_test_enabled: bool,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_alpha: i32,
    blend_dst_alpha: i32,
    blend_equation_rgb: i32,
    blend_equation_alpha: i32,
    color_mask: [bool; 4],
    depth_mask: bool,
    stencil_mask_front: i32,
    stencil_mask_back: i32,
}

#[derive(Clone)]
struct FrontendGlTextureUnitState {
    texture_2d: Option<glow::Texture>,
    sampler: Option<glow::Sampler>,
}

struct GlProcLoader {
    opengl_framework: Option<Library>,
}

static GL_PROC_LOADER: Lazy<Mutex<GlProcLoader>> = Lazy::new(|| Mutex::new(GlProcLoader::new()));
static ACTIVE_RUNTIME: Lazy<Mutex<Option<Weak<HostRuntime>>>> = Lazy::new(|| Mutex::new(None));
static VULKAN_DEBUG_STEP_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_READBACK_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_FALLBACK_SIZE_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_PRESENT_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_SOURCE_IMAGE_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_RUN_FRAME_DEBUG_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_HANDOFF_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);
static VULKAN_VIDEO_REFRESH_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);
static GL_CONTEXT_DEBUG_LOGGED: AtomicU64 = AtomicU64::new(0);

fn active_runtime() -> Option<Arc<HostRuntime>> {
    ACTIVE_RUNTIME.lock().as_ref()?.upgrade()
}

fn with_active_runtime<T>(f: impl FnOnce(&HostRuntime) -> T) -> Option<T> {
    let runtime = active_runtime()?;
    Some(f(&runtime))
}

fn sanitized_video_fps(fps: f64) -> Option<f64> {
    if fps.is_finite() && fps > 0.0 {
        Some(fps)
    } else {
        None
    }
}

fn is_audio_master_pacing_core_name(core_name: &str) -> bool {
    core_name.eq_ignore_ascii_case("flycast")
        || core_name.eq_ignore_ascii_case("mupen64plus_next")
        || core_name.eq_ignore_ascii_case("mednafen_psx_hw")
        || core_name.eq_ignore_ascii_case("mednafen_saturn")
        || core_name.eq_ignore_ascii_case("pcsx2")
        || core_name.eq_ignore_ascii_case("play")
}

fn frame_time_usecs_for_core(
    is_fixed_step_core: bool,
    reference_usecs: i64,
    measured_delta_usecs: Option<i64>,
) -> i64 {
    if is_fixed_step_core {
        return reference_usecs.max(0);
    }

    measured_delta_usecs.unwrap_or(reference_usecs).max(0)
}

fn audio_frames_for_latency(sample_rate_hz: f64, latency_secs: f64) -> usize {
    if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 || latency_secs <= 0.0 {
        return 0;
    }

    (sample_rate_hz * latency_secs).round() as usize
}

fn log_core_binary_load_metadata(core_name: &str, core_path: &Path) {
    let canonical_path = core_path
        .canonicalize()
        .unwrap_or_else(|_| core_path.to_path_buf());
    let metadata = fs::metadata(core_path).ok();
    let modified_unix_secs = metadata
        .as_ref()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let size_bytes = metadata.as_ref().map(|meta| meta.len());

    info!(
        target: "arcade_libretro::core_loader",
        "loading core binary core={} path={} modified_unix_secs={} size_bytes={}",
        core_name,
        canonical_path.display(),
        modified_unix_secs
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("unknown")),
        size_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("unknown")),
    );
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

#[derive(Clone, Copy)]
#[repr(C)]
struct RetroSystemTiming {
    fps: f64,
    sample_rate: f64,
}

#[derive(Clone, Copy)]
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
    format!("{core_name}_libretro.dylib")
}

fn core_library_filename_candidates(core_name: &str, emulation: &EmulationConfig) -> Vec<String> {
    let mut candidates = Vec::new();

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    if core_name.eq_ignore_ascii_case("mupen64plus_next")
        && emulation.n64.cpu_core_mode == N64CpuCoreMode::DynamicRecompiler
    {
        candidates.push(String::from(
            "mupen64plus_next_dynarec_arm64_libretro.dylib",
        ));
    }

    candidates.push(default_core_library_filename(core_name));
    if core_name.eq_ignore_ascii_case("mednafen_pce_fast") {
        candidates.push(String::from("beetle_pce_fast_libretro.dylib"));
    } else if core_name.eq_ignore_ascii_case("mednafen_saturn") {
        candidates.push(String::from("beetle_saturn_libretro.dylib"));
    }
    candidates
}

impl GlProcLoader {
    fn new() -> Self {
        unsafe {
            let opengl_framework =
                Library::new("/System/Library/Frameworks/OpenGL.framework/OpenGL").ok();

            Self { opengl_framework }
        }
    }

    fn get(&self, sym: &CStr) -> *const c_void {
        if let Some(lib) = self.opengl_framework.as_ref() {
            if let Ok(symbol) = unsafe { lib.get::<*const c_void>(sym.to_bytes_with_nul()) } {
                if !(*symbol).is_null() {
                    return *symbol;
                }
            }
        }
        std::ptr::null()
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
        if capabilities.gl_context.is_some() {
            let mut state = self.runtime.hw_render_state.lock();
            if state.eframe_gl_ctx_id == 0 {
                state.eframe_gl_ctx_id = current_gl_ctx_id();
            }
        }
        if changed {
            self.runtime
                .hw_render_state
                .lock()
                .external_vulkan_probe_logged = false;
        }
        if changed && vulkan_debug_enabled() {
            info!(
                target: "arcade_libretro::vulkan_debug",
                "frontend windowing renderer={:?} window_handle={:?} display_handle={:?}",
                capabilities.renderer_name,
                capabilities.window_handle_kind,
                capabilities.display_handle_kind
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

    pub fn expects_external_vulkan_present_window(&self) -> bool {
        self.runtime
            .video_coordinator
            .lock()
            .current_selection()
            .map(|selection| selection.allows_external_present)
            .unwrap_or(false)
    }

    pub fn vulkan_present_test_metrics(&self) -> VulkanPresentTestMetrics {
        let backend_kind = self.runtime.video_coordinator.lock().current_backend_kind();
        let external_window_created = self
            .runtime
            .hw_render_state
            .lock()
            .external_vulkan_window
            .is_some();
        let snapshot = self.runtime.vulkan_present_metrics.lock().clone();
        VulkanPresentTestMetrics {
            backend_kind,
            external_window_created,
            queue_present_attempts: snapshot.queue_present_attempts,
            queue_present_successes: snapshot.queue_present_successes,
            external_present_deliveries: snapshot.external_present_deliveries,
            cpu_frame_deliveries: snapshot.cpu_frame_deliveries,
            source_non_black_seen: snapshot.source_non_black_seen,
            swapchain_non_black_seen: snapshot.swapchain_non_black_seen,
            non_tiny_source_frame_seen: snapshot.non_tiny_source_frame_seen,
            max_consecutive_tiny_source_frames: snapshot.max_consecutive_tiny_source_frames,
        }
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
        core_library_filename_candidates(core_name, &self.emulation)
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

        log_core_binary_load_metadata(core_name, core_path);
        let _ = self.unload();
        self.runtime
            .shutdown_requested
            .store(false, Ordering::Relaxed);
        reset_vulkan_present_metrics(&self.runtime);
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
                "backend selection core={} chosen={:?} fallbacks={:?} requires_hw_render={}",
                core_name,
                selection.chosen,
                selection.fallbacks,
                requirements.requires_hw_render,
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

        let mut prepared = prepare_game_content(launch_rom_path, &requirements)?;
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
                context.runtime_video_fps = None;
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
                {
                    let mut context = self.runtime.environment_context.lock();
                    context.runtime_video_fps = sanitized_video_fps(av_info.timing.fps);
                }
                let _version = unsafe { (api.api_version)() };
                let normalized_video_fps = normalize_display_fps(av_info.timing.fps);
                let pacing_interval_ms =
                    if normalized_video_fps.is_finite() && normalized_video_fps > 0.0 {
                        1000.0 / normalized_video_fps
                    } else {
                        0.0
                    };
                if core_name.eq_ignore_ascii_case("play") {
                    info!(
                        target: "arcade_libretro::core_loader",
                        core = core_name,
                        reported_video_fps = av_info.timing.fps,
                        normalized_video_fps,
                        pacing_interval_ms,
                        sample_rate_hz = av_info.timing.sample_rate,
                        base_width = av_info.geometry.base_width,
                        base_height = av_info.geometry.base_height,
                        max_width = av_info.geometry.max_width,
                        max_height = av_info.geometry.max_height,
                        "play startup AV timing"
                    );
                }
                let preferred_sample_rate_hz =
                    if av_info.timing.sample_rate.is_finite() && av_info.timing.sample_rate > 0.0 {
                        Some(av_info.timing.sample_rate.round() as u32)
                    } else {
                        None
                    };
                configure_audio_profile_for_core(core_name);
                set_audio_source_sample_rate(av_info.timing.sample_rate);
                if let Err(err) = self.ensure_audio_output_started(preferred_sample_rate_hz) {
                    eprintln!("Audio output unavailable, continuing without sound: {err}");
                    warn!("Audio output unavailable, continuing without sound: {err}");
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
            // Re-plan the video session so subsequent retry attempts dispatch to
            // the correct backend. destroy_hw_render_session() calls end_session()
            // which clears VideoCoordinator.session; without this, current_backend_kind()
            // falls back to Software and HW frames are never read back.
            //
            // Use the context type requested by the core during the failed
            // attempt (when available) so retries can pivot to the correct
            // backend for that core binary.
            {
                let mut coordinator = self.runtime.video_coordinator.lock();
                coordinator.plan_session(VideoSessionInfo {
                    core_name: core_name.to_string(),
                    requires_hw_render: requirements.requires_hw_render,
                    requested_hw_context_type,
                });
            }
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
        let loaded_fps = self.loaded.lock().as_ref().map(|c| c.video_fps)?;
        let mut fps = loaded_fps;
        let (is_audio_master_core, runtime_video_fps) = {
            let context = self.runtime.environment_context.lock();
            let is_audio_master_core = context
                .loaded_core_name
                .as_deref()
                .is_some_and(is_audio_master_pacing_core_name);
            (is_audio_master_core, context.runtime_video_fps)
        };
        if is_audio_master_core {
            fps = runtime_video_fps.unwrap_or(loaded_fps);
        }
        let fps = normalize_display_fps(sanitized_video_fps(fps)?);

        Some(std::time::Duration::from_secs_f64(1.0 / fps))
    }

    pub fn audio_queue_snapshot(&self) -> Option<AudioQueueSnapshot> {
        if !self.is_loaded() {
            return None;
        }
        let state = self.runtime.audio_state.lock();
        let output_rate = state.output_sample_rate;
        let target_frames = audio_frames_for_latency(output_rate, state.target_latency_secs);
        if target_frames == 0 {
            return None;
        }
        let max_frames =
            audio_frames_for_latency(output_rate, state.max_latency_secs).max(target_frames + 1);
        Some(AudioQueueSnapshot {
            queue_frames: state.samples.len() / 2,
            target_frames,
            max_frames,
            source_rate: state.source_sample_rate,
            output_rate,
            trimmed_total_frames: state.trimmed_samples_total / 2,
        })
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
        if self.runtime.shutdown_requested.load(Ordering::Relaxed) {
            return Err(anyhow!(
                "libretro core requested frontend shutdown via RETRO_ENVIRONMENT_SHUTDOWN"
            ));
        }
        if let Some(error) = vulkan_present_fail_fast_error(&self.runtime) {
            // Fail-fast is terminal for the current session: once Vulkan output is
            // confirmed unhealthy, stop stepping the core and surface the explicit
            // error without attempting fallback.
            return Err(anyhow!(error));
        }
        let uses_hw_render = loaded.uses_hw_render;
        let loaded_video_fps = loaded.video_fps;
        let target_size = hw_render_target_size(loaded.video_max_size, loaded.video_base_size);
        drop(loaded_guard);
        let (is_audio_master_core, runtime_video_fps) = {
            let context = self.runtime.environment_context.lock();
            let is_audio_master_core = context
                .loaded_core_name
                .as_deref()
                .is_some_and(is_audio_master_pacing_core_name);
            (is_audio_master_core, context.runtime_video_fps)
        };
        let pacing_video_fps = if is_audio_master_core {
            runtime_video_fps.unwrap_or(loaded_video_fps)
        } else {
            loaded_video_fps
        };
        let default_frame_time_usecs = sanitized_video_fps(pacing_video_fps)
            .map(|fps| (1_000_000.0 / fps).round() as i64)
            .unwrap_or(16_667);

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
        let frontend_gl_state = if uses_hw_render {
            capture_frontend_gl_state(&self.runtime)
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
        let frame_time_callback = {
            let mut context = self.runtime.environment_context.lock();
            let callback = context.frame_time_callback;
            callback.map(|callback| {
                let now = std::time::Instant::now();
                let is_fixed_step_core = context
                    .loaded_core_name
                    .as_deref()
                    .is_some_and(is_audio_master_pacing_core_name);
                let reference_usecs = if context.frame_time_reference_usecs > 0 {
                    context.frame_time_reference_usecs
                } else {
                    default_frame_time_usecs
                };
                let measured_delta_usecs = if let Some(previous) = context.frame_time_last_instant {
                    let delta = now.saturating_duration_since(previous).as_micros();
                    Some(delta.min(i64::MAX as u128) as i64)
                } else {
                    None
                };
                context.frame_time_last_instant = Some(now);
                let usec = frame_time_usecs_for_core(
                    is_fixed_step_core,
                    reference_usecs,
                    measured_delta_usecs,
                );
                (callback, usec)
            })
        };
        if let Some((callback, usec)) = frame_time_callback {
            unsafe { callback(usec) };
        }
        let audio_callback = {
            let mut context = self.runtime.environment_context.lock();
            let is_flycast = context
                .loaded_core_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("flycast"));
            if is_flycast {
                // Flycast should stay on push-audio pacing. Keep pull-callbacks
                // disabled even if a core-side state change tries to re-enable them.
                context.audio_callback_enabled = false;
                None
            } else {
                context
                    .audio_callback_enabled
                    .then_some(context.audio_callback)
                    .flatten()
            }
        };
        if let Some(callback) = audio_callback {
            unsafe { callback() };
        }
        let run_started_at = std::time::Instant::now();
        unsafe {
            (loaded.api.run)();
        }
        if self.runtime.shutdown_requested.load(Ordering::Relaxed) {
            return Err(anyhow!(
                "libretro core requested frontend shutdown via RETRO_ENVIRONMENT_SHUTDOWN"
            ));
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
        {
            let mut context = self.runtime.environment_context.lock();
            if !context.run_fps_probe_logged
                && context
                    .loaded_core_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case("play"))
            {
                let now = std::time::Instant::now();
                let start = context.run_fps_probe_start.unwrap_or(now);
                if context.run_fps_probe_start.is_none() {
                    context.run_fps_probe_start = Some(now);
                }
                context.run_fps_probe_frames = context.run_fps_probe_frames.saturating_add(1);
                let elapsed = now.saturating_duration_since(start);
                if context.run_fps_probe_frames >= 180
                    && elapsed >= std::time::Duration::from_millis(500)
                {
                    let measured_run_fps =
                        context.run_fps_probe_frames as f64 / elapsed.as_secs_f64();
                    info!(
                        target: "arcade_libretro::core_loader",
                        core = "play",
                        reported_video_fps = loaded.video_fps,
                        normalized_video_fps = normalize_display_fps(loaded.video_fps),
                        measured_run_fps,
                        sample_frames = context.run_fps_probe_frames,
                        sample_secs = elapsed.as_secs_f64(),
                        "play startup measured run cadence"
                    );
                    context.run_fps_probe_logged = true;
                }
            }
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
        record_frame_delivery_metrics(&self.runtime, using_vulkan_hw_render, &delivery);
        if let Some(snapshot) = frontend_gl_state.as_ref() {
            restore_frontend_gl_state(&self.runtime, snapshot);
        }

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
                    Ok(FrameDelivery::ExternalPresent) => info!(
                        target: "arcade_libretro::vulkan_debug",
                        "run_frame step={} hw frame result=external_present present_ms={:.3}",
                        debug_step,
                        present_duration.as_secs_f64() * 1000.0
                    ),
                    Ok(FrameDelivery::NoFrame) => {
                        let (present_configured, pending_images, acquired_image_index) = {
                            let state = self.runtime.hw_render_state.lock();
                            let vulkan = state.vulkan.as_ref();
                            (
                                vulkan.and_then(|vulkan| vulkan.present.as_ref()).is_some(),
                                vulkan
                                    .map(|vulkan| vulkan.pending_images.len())
                                    .unwrap_or(0),
                                vulkan
                                    .and_then(|vulkan| vulkan.present.as_ref())
                                    .and_then(|present| present.acquired_image_index),
                            )
                        };
                        info!(
                            target: "arcade_libretro::vulkan_debug",
                            "run_frame step={} hw frame result=no_frame present_ms={:.3} present_configured={} pending_images={} acquired_image_index={:?}",
                            debug_step,
                            present_duration.as_secs_f64() * 1000.0,
                            present_configured,
                            pending_images,
                            acquired_image_index
                        );
                    }
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

        if let Some(error) = vulkan_present_fail_fast_error(&self.runtime) {
            return Err(anyhow!(error));
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

    /// Send a keyboard event to the core via the registered keyboard callback.
    /// `down` = true for key press, false for release.
    /// `keycode` = RETROK_* value, `character` = unicode codepoint (0 if n/a),
    /// `key_modifiers` = bitmask of RETROKMOD_* flags.
    pub fn send_keyboard_event(
        &self,
        down: bool,
        keycode: u32,
        character: u32,
        key_modifiers: u16,
    ) {
        let cb = self.runtime.environment_context.lock().keyboard_event_cb;
        if let Some(cb) = cb {
            unsafe { cb(down, keycode, character, key_modifiers) };
        }
    }

    /// Returns true if the core has registered a keyboard callback.
    pub fn has_keyboard_callback(&self) -> bool {
        self.runtime
            .environment_context
            .lock()
            .keyboard_event_cb
            .is_some()
    }

    pub fn configure_default_controller_ports(&self, max_ports: u32) -> Result<()> {
        let set_controller_port_device = {
            let loaded_guard = self.loaded.lock();
            let Some(loaded) = loaded_guard.as_ref() else {
                return Ok(());
            };
            let Some(set_controller_port_device) = loaded.api.set_controller_port_device else {
                return Ok(());
            };
            set_controller_port_device
        };

        // Compute preferred devices while holding the context lock, then release
        // all host locks before invoking the core callback to avoid re-entrant
        // deadlocks in cores that query environment state during port updates.
        let devices: Vec<u32> = {
            let context = self.runtime.environment_context.lock();
            (0..max_ports as usize)
                .map(|port| {
                    preferred_controller_device(
                        context.controller_info.get(port).map(Vec::as_slice),
                    )
                })
                .collect()
        };

        for (port, device) in devices.into_iter().enumerate() {
            unsafe {
                set_controller_port_device(port as u32, device);
            }
        }

        Ok(())
    }

    pub fn unload(&self) -> Result<()> {
        let mut loaded_guard = self.loaded.lock();
        if let Some(loaded) = loaded_guard.take() {
            let core_label = loaded
                .core_path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown_core")
                .to_owned();
            let audio_set_state_callback = {
                let mut context = self.runtime.environment_context.lock();
                context.audio_callback_enabled = false;
                context.audio_set_state_callback
            };
            if let Some(set_state) = audio_set_state_callback {
                unsafe { set_state(false) };
            }
            unsafe {
                (loaded.api.unload_game)();
            }
            log_vulkan_present_metrics_summary(
                &self.runtime,
                &core_label,
                loaded.uses_hw_render,
                "final",
            );
            destroy_hw_render_session();
            unsafe {
                (loaded.api.deinit)();
            }
        } else {
            destroy_hw_render_session();
        }
        // Drop the audio output *before* clearing the active runtime so the
        // cpal stream stops its callbacks.  Without this the stream keeps
        // firing into write_output_*, sees no runtime, and spams "audio
        // runtime not available" warnings until the LibretroHost is dropped.
        self.audio_output.borrow_mut().take();
        reset_callback_video_state(&self.runtime);
        self.runtime.callback_state.lock().input_state.clear();
        self.runtime
            .shutdown_requested
            .store(false, Ordering::Relaxed);
        {
            let mut context = self.runtime.environment_context.lock();
            context.controller_info.clear();
            context.runtime_video_fps = None;
        }
        reset_vulkan_present_metrics(&self.runtime);
        clear_active_runtime(&self.runtime);
        Ok(())
    }

    /// Push updated emulation settings so they take effect on the next game
    /// launch (and on the next frame if a game is already running).
    pub fn update_core_variables(&mut self, emulation: &EmulationConfig) {
        self.emulation = emulation.clone();
        let mut context = self.runtime.environment_context.lock();
        let Some(core_name) = context.loaded_core_name.clone() else {
            return;
        };
        let Some(backend) = context.loaded_backend else {
            return;
        };
        context.variables = default_core_variables_for(&core_name, backend, emulation);
        context.variables_updated = true;
    }

    fn ensure_audio_output_started(&self, preferred_sample_rate_hz: Option<u32>) -> Result<()> {
        let mut guard = self.audio_output.borrow_mut();
        if let Some(existing) = guard.as_ref() {
            let requested = preferred_sample_rate_hz.unwrap_or(existing.sample_rate_hz);
            if requested == existing.sample_rate_hz {
                // The cpal stream is already running at the right rate, but
                // reset_callback_video_state may have zeroed the AudioState
                // rates.  Restore output_sample_rate so the resampler uses a
                // valid ratio instead of source_rate / 0.0 = +Inf (which
                // causes an infinite loop in the resample advance loop).
                set_audio_output_sample_rate(existing.sample_rate_hz as f64);
                return Ok(());
            }
        }

        let output = AudioOutput::new(preferred_sample_rate_hz)?;
        info!(
            target: "arcade_libretro::audio",
            preferred_sample_rate_hz = preferred_sample_rate_hz.unwrap_or(0),
            output_sample_rate_hz = output.sample_rate_hz,
            "audio output stream started"
        );
        *guard = Some(output);
        Ok(())
    }
}

impl Drop for LibretroHost {
    fn drop(&mut self) {
        let _ = self.unload();
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
        assert_eq!(
            candidates
                .iter()
                .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["fceumm_libretro.dylib"]
        );
    }

    #[test]
    fn resolve_core_candidates_include_pce_fast_compatibility_fallback_name() {
        let dir = tempdir().expect("tempdir");
        let host = LibretroHost::new(
            dir.path().join("cores"),
            dir.path().join("bios"),
            dir.path().join("saves"),
            EmulationConfig::default(),
        );

        let candidates = host.resolve_core_candidates("mednafen_pce_fast");
        let file_names = candidates
            .iter()
            .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
            .collect::<Vec<_>>();

        assert_eq!(
            file_names,
            vec![
                "mednafen_pce_fast_libretro.dylib",
                "beetle_pce_fast_libretro.dylib"
            ]
        );
    }

    #[test]
    fn resolve_core_candidates_include_saturn_compatibility_fallback_name() {
        let dir = tempdir().expect("tempdir");
        let host = LibretroHost::new(
            dir.path().join("cores"),
            dir.path().join("bios"),
            dir.path().join("saves"),
            EmulationConfig::default(),
        );

        let candidates = host.resolve_core_candidates("mednafen_saturn");
        let file_names = candidates
            .iter()
            .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
            .collect::<Vec<_>>();

        assert_eq!(
            file_names,
            vec![
                "mednafen_saturn_libretro.dylib",
                "beetle_saturn_libretro.dylib"
            ]
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
    fn resolve_core_candidates_uses_default_n64_filename_for_cached_lane() {
        let dir = tempdir().expect("tempdir");
        let mut emulation = EmulationConfig::default();
        emulation.n64.cpu_core_mode = N64CpuCoreMode::CachedInterpreter;
        let host = LibretroHost::new(
            dir.path().join("cores"),
            dir.path().join("bios"),
            dir.path().join("saves"),
            emulation,
        );

        let candidates = host.resolve_core_candidates("mupen64plus_next");
        let file_names = candidates
            .iter()
            .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
            .collect::<Vec<_>>();

        assert_eq!(file_names, vec!["mupen64plus_next_libretro.dylib"]);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn resolve_core_candidates_prefers_experimental_n64_dynarec_binary_on_arm64() {
        let dir = tempdir().expect("tempdir");
        let mut emulation = EmulationConfig::default();
        emulation.n64.cpu_core_mode = N64CpuCoreMode::DynamicRecompiler;
        let host = LibretroHost::new(
            dir.path().join("cores"),
            dir.path().join("bios"),
            dir.path().join("saves"),
            emulation,
        );

        let candidates = host.resolve_core_candidates("mupen64plus_next");
        let file_names = candidates
            .iter()
            .map(|path| path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
            .collect::<Vec<_>>();

        assert_eq!(
            file_names,
            vec![
                "mupen64plus_next_dynarec_arm64_libretro.dylib",
                "mupen64plus_next_libretro.dylib"
            ]
        );
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
                    window_handle_kind: Some(String::from("AppKit")),
                    display_handle_kind: Some(String::from("AppKit")),
                },
            );

        let message = vulkan_unimplemented_message(Some(&runtime));

        assert!(message.contains("renderer=eframe_glow"));
        assert!(message.contains("window_handle=AppKit"));
        assert!(message.contains("display_handle=AppKit"));
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

    #[test]
    fn environment_shutdown_request_sets_runtime_shutdown_flag() {
        *ACTIVE_RUNTIME.lock() = None;
        let runtime = Arc::new(HostRuntime::default());
        register_active_runtime(&runtime);

        assert!(!runtime.shutdown_requested.load(Ordering::Relaxed));
        let handled = unsafe { retro_environment(7, std::ptr::null_mut()) };
        assert!(handled);
        assert!(runtime.shutdown_requested.load(Ordering::Relaxed));

        clear_active_runtime(&runtime);
    }

    #[test]
    fn unload_clears_runtime_shutdown_request() {
        let dir = tempdir().expect("tempdir");
        let host = LibretroHost::new(
            dir.path().join("cores"),
            dir.path().join("bios"),
            dir.path().join("saves"),
            EmulationConfig::default(),
        );
        host.runtime
            .shutdown_requested
            .store(true, Ordering::Relaxed);

        host.unload().expect("unload");

        assert!(!host.runtime.shutdown_requested.load(Ordering::Relaxed));
    }

    #[test]
    fn frame_time_usecs_for_fixed_step_core_stays_fixed() {
        assert_eq!(
            frame_time_usecs_for_core(true, 16_667, Some(33_333)),
            16_667
        );
    }

    #[test]
    fn frame_time_usecs_for_variable_delta_core_uses_measured_delta() {
        assert_eq!(
            frame_time_usecs_for_core(false, 16_667, Some(20_000)),
            20_000
        );
        assert_eq!(frame_time_usecs_for_core(false, 16_667, None), 16_667);
    }

    #[test]
    fn audio_master_pacing_core_set_includes_n64_psx_and_ps2() {
        assert!(is_audio_master_pacing_core_name("flycast"));
        assert!(is_audio_master_pacing_core_name("mupen64plus_next"));
        assert!(is_audio_master_pacing_core_name("mednafen_psx_hw"));
        assert!(is_audio_master_pacing_core_name("mednafen_saturn"));
        assert!(is_audio_master_pacing_core_name("pcsx2"));
        assert!(is_audio_master_pacing_core_name("play"));
        assert!(!is_audio_master_pacing_core_name("fceumm"));
    }

    #[test]
    fn set_system_av_info_updates_runtime_video_fps() {
        *ACTIVE_RUNTIME.lock() = None;
        let runtime = Arc::new(HostRuntime::default());
        register_active_runtime(&runtime);

        let mut av_info = RetroSystemAvInfo {
            geometry: RetroGameGeometry {
                base_width: 640,
                base_height: 480,
                max_width: 640,
                max_height: 480,
                aspect_ratio: 4.0 / 3.0,
            },
            timing: RetroSystemTiming {
                fps: 59.94,
                sample_rate: 44_100.0,
            },
        };
        let handled = unsafe {
            retro_environment(
                32,
                (&mut av_info as *mut RetroSystemAvInfo).cast::<c_void>(),
            )
        };

        assert!(handled);
        assert_eq!(
            runtime.environment_context.lock().runtime_video_fps,
            Some(59.94)
        );

        clear_active_runtime(&runtime);
    }
}
