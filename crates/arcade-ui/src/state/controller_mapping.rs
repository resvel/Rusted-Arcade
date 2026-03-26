use std::collections::{BTreeMap, HashMap};

use arcade_domain::{
    default_gamepad_mapping_for_system, MappingEntry, StoredGamepadMapping,
    SYSTEM_DEFAULT_MAPPING_KEY,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ControllerMappingCacheKey {
    pub(crate) system: String,
    pub(crate) mapping_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MappingEditorScope {
    SystemDefault,
    ActiveDevice,
}

pub(crate) struct ControllerMappingState {
    pub(crate) expanded: bool,
    pub(crate) advanced_expanded: bool,
    pub(crate) selected_scope: MappingEditorScope,
    pub(crate) loaded_system: String,
    pub(crate) loaded_key: String,
    pub(crate) loaded_actions: BTreeMap<String, Option<MappingEntry>>,
    pub(crate) loaded_threshold: f32,
    pub(crate) actions: BTreeMap<String, Option<MappingEntry>>,
    pub(crate) threshold: f32,
    /// The system selected in the Input Settings panel (Settings view).
    pub(crate) input_system: String,
    mapping_cache: HashMap<ControllerMappingCacheKey, StoredGamepadMapping>,
}

impl Default for ControllerMappingState {
    fn default() -> Self {
        let default_mapping = default_gamepad_mapping_for_system("NES");
        Self {
            expanded: true,
            advanced_expanded: false,
            selected_scope: MappingEditorScope::SystemDefault,
            loaded_system: String::new(),
            loaded_key: String::new(),
            loaded_actions: default_mapping.actions.clone(),
            loaded_threshold: default_mapping.threshold,
            actions: default_mapping.actions,
            threshold: default_mapping.threshold,
            input_system: String::from("NES"),
            mapping_cache: HashMap::new(),
        }
    }
}

impl ControllerMappingState {
    pub(crate) fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
    }

    pub(crate) fn toggle_advanced_expanded(&mut self) {
        self.advanced_expanded = !self.advanced_expanded;
    }

    pub(crate) fn select_scope(&mut self, scope: MappingEditorScope) {
        if self.selected_scope == scope {
            return;
        }

        self.selected_scope = scope;
        self.loaded_key.clear();
    }

    pub(crate) fn normalize_scope(&mut self, has_active_device: bool) {
        if self.selected_scope == MappingEditorScope::ActiveDevice && !has_active_device {
            self.select_scope(MappingEditorScope::SystemDefault);
        }
    }

    pub(crate) fn desired_mapping_key(&self, active_device_key: Option<&str>) -> String {
        if self.selected_scope == MappingEditorScope::ActiveDevice {
            active_device_key
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| SYSTEM_DEFAULT_MAPPING_KEY.to_string())
        } else {
            SYSTEM_DEFAULT_MAPPING_KEY.to_string()
        }
    }

    pub(crate) fn needs_reload(&self, system: &str, desired_key: &str) -> bool {
        self.loaded_system != system || self.loaded_key != desired_key
    }

    pub(crate) fn load_from_mapping(
        &mut self,
        system: String,
        desired_key: String,
        mapping: StoredGamepadMapping,
        normalized_threshold: f32,
    ) {
        self.loaded_system = system;
        self.loaded_key = desired_key;
        self.loaded_actions = mapping.actions.clone();
        self.loaded_threshold = normalized_threshold;
        self.actions = mapping.actions;
        self.threshold = normalized_threshold;
    }

    pub(crate) fn reset_to_defaults(&mut self, system: &str, normalized_threshold: f32) {
        let mapping = default_gamepad_mapping_for_system(system);
        self.actions = mapping.actions;
        self.threshold = normalized_threshold;
    }

    pub(crate) fn mark_saved(&mut self, normalized_threshold: f32) {
        self.threshold = normalized_threshold;
        self.loaded_actions = self.actions.clone();
        self.loaded_threshold = normalized_threshold;
    }

    pub(crate) fn is_dirty(&self, normalized_threshold: f32) -> bool {
        self.actions != self.loaded_actions || normalized_threshold != self.loaded_threshold
    }

    pub(crate) fn cache_get(
        &self,
        cache_key: &ControllerMappingCacheKey,
    ) -> Option<StoredGamepadMapping> {
        self.mapping_cache.get(cache_key).cloned()
    }

    pub(crate) fn cache_put(
        &mut self,
        cache_key: ControllerMappingCacheKey,
        mapping: StoredGamepadMapping,
    ) {
        self.mapping_cache.insert(cache_key, mapping);
    }

    pub(crate) fn invalidate_cache_for_system(&mut self, system: &str) {
        self.mapping_cache.retain(|key, _| key.system != system);
    }

    pub(crate) fn set_action(&mut self, key: String, selected: Option<MappingEntry>) {
        self.actions.insert(key, selected);
    }
}
