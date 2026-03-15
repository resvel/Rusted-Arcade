use std::collections::BTreeSet;
use std::path::PathBuf;

use arcade_domain::EmulationConfig;
use arcade_libretro::LibretroHost;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir should have workspace parent")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf()
}

fn genesis_host_and_rom() -> (LibretroHost, PathBuf) {
    let root = workspace_root();
    let core_root = root.join("target/debug/cores");
    let bios_root = root.join("target/debug/bios");
    let save_root = root.join("target/debug/data/save-states");
    let rom_path = root.join("target/debug/roms/genesis/Contra - Hard Corps (USA, Korea).md");

    #[cfg(target_os = "macos")]
    let core_name = "genesis_plus_gx_libretro.dylib";
    #[cfg(target_os = "linux")]
    let core_name = "genesis_plus_gx_libretro.so";
    #[cfg(target_os = "windows")]
    let core_name = "genesis_plus_gx_libretro.dll";

    assert!(
        core_root.join(core_name).exists(),
        "genesis_plus_gx core is missing from {}",
        core_root.display()
    );
    assert!(
        rom_path.exists(),
        "test ROM is missing at {}",
        rom_path.display()
    );

    (
        LibretroHost::new(core_root, bios_root, save_root, EmulationConfig::default()),
        rom_path,
    )
}

#[test]
#[ignore = "requires locally staged genesis_plus_gx core and ROM"]
fn genesis_plus_gx_smoke_runs_frames() {
    let (host, rom_path) = genesis_host_and_rom();
    host.load_for_rom("GENESIS", Some("genesis_plus_gx"), &rom_path)
        .expect("genesis_plus_gx should load");

    let mut cpu_frames = 0_u32;
    let mut empty_frames = 0_u32;
    let mut first_frame_info = None;
    let mut first_stride_non_zero_span = None;
    let mut first_320_span = None;
    let mut frames_with_non_zero = 0_u32;
    let mut max_non_zero_bytes = 0_usize;
    let mut max_visible_non_zero_bytes = 0_usize;
    let mut observed_sizes = BTreeSet::new();
    for _ in 0..300 {
        match host
            .run_frame()
            .expect("genesis core should continue running frames")
        {
            Some(frame) => {
                assert!(frame.width > 0, "frame width should be non-zero");
                assert!(frame.height > 0, "frame height should be non-zero");
                observed_sizes.insert((frame.width, frame.height));
                let non_zero_bytes = frame.data.iter().filter(|value| **value != 0).count();
                let visible_row_len = frame.width as usize * 2;
                let mut visible_non_zero_bytes = 0_usize;
                for row in 0..frame.height as usize {
                    let row_start = row.saturating_mul(frame.pitch);
                    let row_end = row_start
                        .saturating_add(visible_row_len)
                        .min(frame.data.len());
                    visible_non_zero_bytes += frame.data[row_start..row_end]
                        .iter()
                        .filter(|value| **value != 0)
                        .count();
                }
                if non_zero_bytes > 0 {
                    frames_with_non_zero += 1;
                }
                max_non_zero_bytes = max_non_zero_bytes.max(non_zero_bytes);
                max_visible_non_zero_bytes = max_visible_non_zero_bytes.max(visible_non_zero_bytes);
                if first_frame_info.is_none() {
                    first_frame_info = Some((
                        frame.width,
                        frame.height,
                        frame.pitch,
                        frame.pixel_format,
                        non_zero_bytes,
                    ));
                }
                if first_stride_non_zero_span.is_none() && frame.pitch >= 2 {
                    let stride_pixels = frame.pitch / 2;
                    let mut min_x = usize::MAX;
                    let mut max_x = 0_usize;
                    for y in 0..frame.height as usize {
                        let row_start = y.saturating_mul(frame.pitch);
                        for x in 0..stride_pixels {
                            let src = row_start + x * 2;
                            if src + 1 >= frame.data.len() {
                                break;
                            }
                            let value = u16::from_le_bytes([frame.data[src], frame.data[src + 1]]);
                            if value != 0 {
                                min_x = min_x.min(x);
                                max_x = max_x.max(x);
                            }
                        }
                    }
                    if min_x != usize::MAX {
                        first_stride_non_zero_span = Some((stride_pixels, min_x, max_x));
                    }
                }
                if first_320_span.is_none()
                    && frame.width == 320
                    && frame.height == 224
                    && frame.pitch >= 2
                {
                    let stride_pixels = frame.pitch / 2;
                    let mut min_x = usize::MAX;
                    let mut max_x = 0_usize;
                    for y in 0..frame.height as usize {
                        let row_start = y.saturating_mul(frame.pitch);
                        for x in 0..stride_pixels {
                            let src = row_start + x * 2;
                            if src + 1 >= frame.data.len() {
                                break;
                            }
                            let value = u16::from_le_bytes([frame.data[src], frame.data[src + 1]]);
                            if value != 0 {
                                min_x = min_x.min(x);
                                max_x = max_x.max(x);
                            }
                        }
                    }
                    if min_x != usize::MAX {
                        first_320_span = Some((stride_pixels, min_x, max_x));
                    }
                }
                cpu_frames += 1;
            }
            None => empty_frames += 1,
        }
    }

    host.unload().expect("core should unload cleanly");
    assert!(
        cpu_frames > 0,
        "genesis_plus_gx did not produce any CPU frames"
    );
    if let Some((width, height, pitch, pixel_format, non_zero_bytes)) = first_frame_info {
        eprintln!(
            "genesis smoke first_frame={}x{} pitch={} pixel_format={pixel_format:?} non_zero_bytes={non_zero_bytes}",
            width, height, pitch
        );
    }
    if let Some((stride_pixels, min_x, max_x)) = first_stride_non_zero_span {
        eprintln!(
            "genesis smoke first_stride_non_zero_span stride_pixels={} min_x={} max_x={}",
            stride_pixels, min_x, max_x
        );
    }
    if let Some((stride_pixels, min_x, max_x)) = first_320_span {
        eprintln!(
            "genesis smoke first_320_span stride_pixels={} min_x={} max_x={}",
            stride_pixels, min_x, max_x
        );
    }
    eprintln!(
        "genesis smoke cpu_frames={cpu_frames} empty_frames={empty_frames} frames_with_non_zero={frames_with_non_zero} max_non_zero_bytes={max_non_zero_bytes} max_visible_non_zero_bytes={max_visible_non_zero_bytes} observed_sizes={:?}",
        observed_sizes
    );
}
