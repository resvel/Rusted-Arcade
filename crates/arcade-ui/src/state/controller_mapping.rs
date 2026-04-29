use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use arcade_domain::{default_gamepad_mapping_for_system, MappingEntry, StoredGamepadMapping};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ControllerMappingCacheKey {
    pub(crate) system: String,
    pub(crate) mapping_key: String,
}

pub(crate) struct ControllerMappingState {
    pub(crate) expanded: bool,
    pub(crate) advanced_expanded: bool,
    pub(crate) show_system_hotspot_debug: bool,
    pub(crate) selected_device_key: Option<String>,
    pub(crate) pending_device_switch_key: Option<String>,
    pub(crate) pending_device_switch_label: Option<String>,
    pub(crate) loaded_system: String,
    pub(crate) loaded_key: String,
    pub(crate) loaded_actions: BTreeMap<String, Option<MappingEntry>>,
    pub(crate) loaded_threshold: f32,
    pub(crate) actions: BTreeMap<String, Option<MappingEntry>>,
    pub(crate) threshold: f32,
    pub(crate) selected_mapping_action: Option<String>,
    /// The system selected in the Input Settings panel (Settings view).
    pub(crate) input_system: String,
    mapping_cache: HashMap<ControllerMappingCacheKey, Arc<StoredGamepadMapping>>,
}

impl Default for ControllerMappingState {
    fn default() -> Self {
        let default_mapping = default_gamepad_mapping_for_system("NES");
        Self {
            expanded: true,
            advanced_expanded: false,
            show_system_hotspot_debug: false,
            selected_device_key: None,
            pending_device_switch_key: None,
            pending_device_switch_label: None,
            loaded_system: String::new(),
            loaded_key: String::new(),
            loaded_actions: default_mapping.actions.clone(),
            loaded_threshold: default_mapping.threshold,
            actions: default_mapping.actions,
            threshold: default_mapping.threshold,
            selected_mapping_action: None,
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

    pub(crate) fn toggle_system_hotspot_debug(&mut self) {
        self.show_system_hotspot_debug = !self.show_system_hotspot_debug;
    }

    pub(crate) fn selected_device_key(&self) -> Option<&str> {
        self.selected_device_key.as_deref()
    }

    pub(crate) fn set_selected_device_key(&mut self, device_key: Option<String>) {
        if self.selected_device_key == device_key {
            return;
        }
        self.selected_device_key = device_key;
        self.loaded_key.clear();
        self.selected_mapping_action = None;
        self.clear_pending_device_switch();
    }

    pub(crate) fn queue_pending_device_switch(&mut self, key: String, label: String) {
        self.pending_device_switch_key = Some(key);
        self.pending_device_switch_label = Some(label);
    }

    pub(crate) fn pending_device_switch(&self) -> Option<(&str, &str)> {
        match (
            self.pending_device_switch_key.as_deref(),
            self.pending_device_switch_label.as_deref(),
        ) {
            (Some(key), Some(label)) => Some((key, label)),
            _ => None,
        }
    }

    pub(crate) fn clear_pending_device_switch(&mut self) {
        self.pending_device_switch_key = None;
        self.pending_device_switch_label = None;
    }

    pub(crate) fn apply_pending_device_switch(&mut self) {
        let Some(key) = self.pending_device_switch_key.take() else {
            return;
        };
        self.selected_device_key = Some(key);
        self.loaded_key.clear();
        self.selected_mapping_action = None;
        self.pending_device_switch_label = None;
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
        self.selected_mapping_action = None;
    }

    pub(crate) fn reset_to_defaults(&mut self, system: &str, normalized_threshold: f32) {
        let mapping = default_gamepad_mapping_for_system(system);
        self.actions = mapping.actions;
        self.threshold = normalized_threshold;
        self.selected_mapping_action = None;
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
    ) -> Option<Arc<StoredGamepadMapping>> {
        self.mapping_cache.get(cache_key).cloned()
    }

    pub(crate) fn cache_put(
        &mut self,
        cache_key: ControllerMappingCacheKey,
        mapping: StoredGamepadMapping,
    ) {
        self.mapping_cache.insert(cache_key, Arc::new(mapping));
    }

    pub(crate) fn invalidate_cache_for_system(&mut self, system: &str) {
        self.mapping_cache.retain(|key, _| key.system != system);
    }

    pub(crate) fn select_mapping_action(&mut self, action: Option<String>) {
        self.selected_mapping_action = action;
    }
}

#[cfg(test)]
mod tests {
    use super::ControllerMappingState;

    #[test]
    fn pending_switch_tracks_confirm_and_cancel() {
        let mut state = ControllerMappingState::default();
        state.set_selected_device_key(Some(String::from("device-a")));
        state.queue_pending_device_switch(String::from("device-b"), String::from("Pad B"));
        assert_eq!(state.pending_device_switch(), Some(("device-b", "Pad B")));

        state.clear_pending_device_switch();
        assert!(state.pending_device_switch().is_none());
        assert_eq!(state.selected_device_key(), Some("device-a"));

        state.queue_pending_device_switch(String::from("device-c"), String::from("Pad C"));
        state.apply_pending_device_switch();
        assert_eq!(state.selected_device_key(), Some("device-c"));
        assert!(state.pending_device_switch().is_none());
    }
}
