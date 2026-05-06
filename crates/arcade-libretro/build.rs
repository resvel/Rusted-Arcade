fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=OpenGL");
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=AppKit");
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=IOSurface");
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=CoreFoundation");

    cc::Build::new()
        .file("src/libretro_log_shim.c")
        .warnings(false)
        .compile("arcade_libretro_log_shim");

    compile_vulkan_present_shader(
        "vulkan_present.vert.spv",
        naga::ShaderStage::Vertex,
        r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0, 3.0),
        vec2<f32>(3.0, -1.0),
    );
    var output: VertexOutput;
    let position = positions[vertex_index];
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = position * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    return output;
}
"#,
    );
    compile_vulkan_present_shader(
        "vulkan_present.frag.spv",
        naga::ShaderStage::Fragment,
        r#"
@group(0) @binding(0)
var present_sampler: sampler;

@group(0) @binding(1)
var present_texture: texture_2d<f32>;

@fragment
fn main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    return textureSampleLevel(present_texture, present_sampler, uv, 0.0);
}
"#,
    );
}

fn compile_vulkan_present_shader(filename: &str, stage: naga::ShaderStage, source: &str) {
    let module = naga::front::wgsl::parse_str(source).expect("failed to parse WGSL shader");
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let info = validator
        .validate(&module)
        .expect("failed to validate WGSL shader");
    let options = naga::back::spv::Options::default();
    let pipeline_options = naga::back::spv::PipelineOptions {
        shader_stage: stage,
        entry_point: String::from("main"),
    };
    let words = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline_options))
        .expect("failed to compile WGSL shader to SPIR-V");
    let output_path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR missing"))
            .join(filename);
    let mut bytes = Vec::with_capacity(words.len() * std::mem::size_of::<u32>());
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    std::fs::write(output_path, bytes).expect("failed to write compiled SPIR-V shader");
}
