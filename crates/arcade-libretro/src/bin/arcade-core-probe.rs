use std::ffi::{c_char, c_void, CStr};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use arcade_domain::{
    display_name_for_core, infer_core_name_from_library_path, infer_system_for_core,
    parse_legacy_core_variable, DynamicCoreProfile, DynamicCoreVariableDefinition,
    DynamicCoreVariableOption,
};
use libloading::{Library, Symbol};

const RETRO_ENVIRONMENT_SET_VARIABLES: u32 = 16;
const RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION: u32 = 52;
const RETRO_ENVIRONMENT_SET_CORE_OPTIONS: u32 = 53;
const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL: u32 = 54;
const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2: u32 = 67;
const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL: u32 = 68;

type RetroSetEnvironment = unsafe extern "C" fn(RetroEnvironmentFn);
type RetroInit = unsafe extern "C" fn();
type RetroDeinit = unsafe extern "C" fn();
type RetroGetSystemInfo = unsafe extern "C" fn(*mut RetroSystemInfo);
type RetroEnvironmentFn = unsafe extern "C" fn(u32, *mut c_void) -> bool;

#[repr(C)]
struct RetroVariable {
    key: *const c_char,
    value: *const c_char,
}

#[repr(C)]
struct RetroCoreOptionValue {
    value: *const c_char,
    label: *const c_char,
}

#[repr(C)]
struct RetroCoreOptionDefinition {
    key: *const c_char,
    desc: *const c_char,
    info: *const c_char,
    values: *const RetroCoreOptionValue,
    default_value: *const c_char,
}

#[repr(C)]
struct RetroCoreOptionsIntl {
    us: *const RetroCoreOptionDefinition,
    local: *const RetroCoreOptionDefinition,
}

#[repr(C)]
struct RetroCoreOptionV2Category {
    key: *const c_char,
    desc: *const c_char,
    info: *const c_char,
}

#[repr(C)]
struct RetroCoreOptionV2Definition {
    key: *const c_char,
    desc: *const c_char,
    info: *const c_char,
    info_categorized: *const c_char,
    category_key: *const c_char,
    values: *const RetroCoreOptionValue,
    default_value: *const c_char,
}

#[repr(C)]
struct RetroCoreOptionsV2 {
    categories: *const RetroCoreOptionV2Category,
    definitions: *const RetroCoreOptionV2Definition,
}

#[repr(C)]
struct RetroCoreOptionsV2Intl {
    us: *const RetroCoreOptionsV2,
    local: *const RetroCoreOptionsV2,
}

#[repr(C)]
#[derive(Default)]
struct RetroSystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[derive(Default)]
struct ProbeState {
    core_name: String,
    system: String,
    variables: Vec<DynamicCoreVariableDefinition>,
}

static PROBE_STATE: OnceLock<Mutex<ProbeState>> = OnceLock::new();

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let core_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("usage: arcade-core-probe <core-path> [core-name] [system]"))?;
    let core_name = args
        .next()
        .or_else(|| infer_core_name_from_library_path(&core_path))
        .ok_or_else(|| anyhow!("could not infer core name from {}", core_path.display()))?;
    let system = args
        .next()
        .unwrap_or_else(|| infer_system_for_core(&core_name));

    let metadata = std::fs::metadata(&core_path)
        .with_context(|| format!("failed to stat core {}", core_path.display()))?;
    let modified_unix = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    let state = PROBE_STATE.get_or_init(|| Mutex::new(ProbeState::default()));
    {
        let mut guard = state.lock().expect("probe state lock");
        *guard = ProbeState {
            core_name: core_name.clone(),
            system: system.clone(),
            variables: Vec::new(),
        };
    }

    let library = unsafe { Library::new(&core_path) }
        .with_context(|| format!("failed to load core {}", core_path.display()))?;
    let set_environment =
        unsafe { load_symbol::<RetroSetEnvironment>(&library, b"retro_set_environment")? };
    let init = unsafe { load_symbol::<RetroInit>(&library, b"retro_init")? };
    let deinit = unsafe { load_symbol::<RetroDeinit>(&library, b"retro_deinit")? };
    let get_system_info =
        unsafe { load_symbol::<RetroGetSystemInfo>(&library, b"retro_get_system_info")? };

    let mut info = RetroSystemInfo::default();
    unsafe {
        get_system_info(&mut info as *mut RetroSystemInfo);
        set_environment(probe_environment);
        init();
        deinit();
    }

    let display_name = unsafe { c_string_to_string(info.library_name) }
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| display_name_for_core(&core_name));

    let variables = {
        let guard = state.lock().expect("probe state lock");
        guard.variables.clone()
    };

    let profile = DynamicCoreProfile {
        core_name,
        display_name,
        system,
        source_path: Some(core_path.display().to_string()),
        source_modified_unix: modified_unix,
        source_len: Some(metadata.len()),
        variables,
    };

    println!("{}", serde_json::to_string_pretty(&profile)?);
    Ok(())
}

unsafe fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T> {
    let symbol: Symbol<T> = library.get(name).with_context(|| {
        format!(
            "missing required libretro symbol {}",
            String::from_utf8_lossy(name)
        )
    })?;
    Ok(*symbol)
}

unsafe extern "C" fn probe_environment(cmd: u32, data: *mut c_void) -> bool {
    match cmd {
        RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION => {
            if !data.is_null() {
                unsafe {
                    *(data as *mut u32) = 2;
                }
            }
            true
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS => {
            if data.is_null() {
                return false;
            }
            unsafe {
                collect_core_option_definitions(data as *const RetroCoreOptionDefinition, None)
            }
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL => {
            if data.is_null() {
                return false;
            }
            let intl = unsafe { &*(data as *const RetroCoreOptionsIntl) };
            let defs = if !intl.local.is_null() {
                intl.local
            } else {
                intl.us
            };
            unsafe { collect_core_option_definitions(defs, None) }
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2 => {
            if data.is_null() {
                return false;
            }
            let options = unsafe { &*(data as *const RetroCoreOptionsV2) };
            unsafe { collect_core_option_v2_definitions(options.definitions) }
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL => {
            if data.is_null() {
                return false;
            }
            let intl = unsafe { &*(data as *const RetroCoreOptionsV2Intl) };
            let options = if !intl.local.is_null() {
                intl.local
            } else {
                intl.us
            };
            if options.is_null() {
                return false;
            }
            let options = unsafe { &*options };
            unsafe { collect_core_option_v2_definitions(options.definitions) }
        }
        RETRO_ENVIRONMENT_SET_VARIABLES => {
            if data.is_null() {
                return false;
            }
            let Some(state) = PROBE_STATE.get() else {
                return false;
            };
            let mut guard = state.lock().expect("probe state lock");
            let mut variable = data as *const RetroVariable;
            while !variable.is_null() {
                let current = unsafe { &*variable };
                if current.key.is_null() {
                    break;
                }
                let Ok(key) = (unsafe { CStr::from_ptr(current.key) }).to_str() else {
                    break;
                };
                let spec = if current.value.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(current.value) }
                        .to_string_lossy()
                        .to_string()
                };
                if let Some(definition) =
                    parse_legacy_core_variable(key, &spec, &guard.core_name, &guard.system)
                {
                    if !guard
                        .variables
                        .iter()
                        .any(|existing| existing.key == definition.key)
                    {
                        guard.variables.push(definition);
                    }
                }
                variable = unsafe { variable.add(1) };
            }
            true
        }
        _ => false,
    }
}

unsafe fn collect_core_option_definitions(
    mut definition: *const RetroCoreOptionDefinition,
    group: Option<String>,
) -> bool {
    if definition.is_null() {
        return false;
    }
    while !definition.is_null() {
        let current = unsafe { &*definition };
        if current.key.is_null() {
            break;
        }
        if let Some(variable) = unsafe {
            dynamic_variable_from_core_option(
                current.key,
                current.desc,
                current.values,
                current.default_value,
                group.clone().unwrap_or_else(|| String::from("Discovered")),
            )
        } {
            push_discovered_variable(variable);
        }
        definition = unsafe { definition.add(1) };
    }
    true
}

unsafe fn collect_core_option_v2_definitions(
    mut definition: *const RetroCoreOptionV2Definition,
) -> bool {
    if definition.is_null() {
        return false;
    }
    while !definition.is_null() {
        let current = unsafe { &*definition };
        if current.key.is_null() {
            break;
        }
        let group = unsafe { c_string_to_string(current.category_key) }
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("Discovered"));
        if let Some(variable) = unsafe {
            dynamic_variable_from_core_option(
                current.key,
                current.desc,
                current.values,
                current.default_value,
                group,
            )
        } {
            push_discovered_variable(variable);
        }
        definition = unsafe { definition.add(1) };
    }
    true
}

unsafe fn dynamic_variable_from_core_option(
    key: *const c_char,
    desc: *const c_char,
    values: *const RetroCoreOptionValue,
    default_value: *const c_char,
    group: String,
) -> Option<DynamicCoreVariableDefinition> {
    let key = unsafe { c_string_to_string(key) }?;
    let label = unsafe { c_string_to_string(desc) }.unwrap_or_else(|| key.clone());
    let mut options = unsafe { collect_option_values(values) };
    if options.is_empty() {
        return None;
    }
    if let Some(default) = unsafe { c_string_to_string(default_value) } {
        if let Some(index) = options.iter().position(|option| option.value == default) {
            let default_option = options.remove(index);
            options.insert(0, default_option);
        }
    }
    Some(DynamicCoreVariableDefinition {
        key,
        label,
        group,
        options,
    })
}

unsafe fn collect_option_values(
    mut value: *const RetroCoreOptionValue,
) -> Vec<DynamicCoreVariableOption> {
    let mut values = Vec::new();
    if value.is_null() {
        return values;
    }
    while !value.is_null() {
        let current = unsafe { &*value };
        if current.value.is_null() {
            break;
        }
        let Some(raw_value) = (unsafe { c_string_to_string(current.value) }) else {
            break;
        };
        let display = unsafe { c_string_to_string(current.label) };
        values.push(DynamicCoreVariableOption {
            value: raw_value,
            display,
        });
        value = unsafe { value.add(1) };
    }
    values
}

fn push_discovered_variable(variable: DynamicCoreVariableDefinition) {
    let Some(state) = PROBE_STATE.get() else {
        return;
    };
    let mut guard = state.lock().expect("probe state lock");
    if !guard
        .variables
        .iter()
        .any(|existing| existing.key == variable.key)
    {
        guard.variables.push(variable);
    }
}

unsafe fn c_string_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string())
    }
}
