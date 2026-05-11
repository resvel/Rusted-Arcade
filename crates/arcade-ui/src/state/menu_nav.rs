use std::time::{Duration, Instant};

use crate::app::{AppView, GridSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuFocusRegion {
    TopNav,
    LibraryManageButton,
    FiltersToggle,
    FiltersSystem,
    FiltersAlpha,
    ManageHeader,
    ManageScope,
    ManageActions,
    ManageSettings,
    ManageScrapeSystems,
    ManageScrapeActions,
    ManageList,
    SettingsSectionNav,
    SettingsRuntimeSetupSystem,
    SettingsRuntimeSetup,
    /// Core/system selector tabs in the settings panel.
    SettingsAppConfigCoreTab,
    /// A specific core variable row (index tracked separately).
    SettingsAppConfigCoreVariable,
    SettingsAppConfigSave,
    SettingsCoverSettings,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuNavDirection {
    Left,
    Right,
    Up,
    Down,
}

pub(crate) struct MenuNavState {
    pub(crate) focus_region: MenuFocusRegion,
    pub(crate) top_nav_index: usize,
    pub(crate) system_filter_index: usize,
    pub(crate) alpha_filter_index: usize,
    pub(crate) manage_scope_index: usize,
    pub(crate) manage_action_index: usize,
    pub(crate) manage_settings_index: usize,
    pub(crate) manage_scrape_system_index: usize,
    pub(crate) manage_scrape_action_index: usize,
    pub(crate) manage_list_index: usize,
    /// Which Settings subview is focused in the Settings section selector.
    pub(crate) settings_section_index: usize,
    /// Which core tab is focused in the system/core selector.
    pub(crate) settings_core_tab_index: usize,
    /// Which variable row is focused within the selected core.
    pub(crate) settings_core_variable_index: usize,
    /// Which option button is focused within the current variable row.
    pub(crate) settings_core_option_index: usize,
    pub(crate) settings_cover_action_index: usize,
    pub(crate) grid_index_home: usize,
    pub(crate) grid_index_library: usize,
    pub(crate) grid_columns_home: usize,
    pub(crate) grid_columns_library: usize,
    pub(crate) grid_visible_rows_home: usize,
    pub(crate) grid_visible_rows_library: usize,
    pub(crate) visible_row_start_home: usize,
    pub(crate) visible_row_end_home: usize,
    pub(crate) visible_row_start_library: usize,
    pub(crate) visible_row_end_library: usize,
    pub(crate) pending_scroll_to_index: Option<usize>,
    pub(crate) repeat_direction: Option<MenuNavDirection>,
    pub(crate) repeat_started_at: Option<Instant>,
    pub(crate) next_repeat_at: Option<Instant>,
    pub(crate) south_held: bool,
    pub(crate) east_held: bool,
    pub(crate) west_held: bool,
    pub(crate) left_trigger_held: bool,
    pub(crate) right_trigger_held: bool,
    pub(crate) left_shoulder_held: bool,
    pub(crate) right_shoulder_held: bool,
    pub(crate) near_end_load_armed: bool,
}

impl Default for MenuNavState {
    fn default() -> Self {
        Self {
            focus_region: MenuFocusRegion::Grid,
            top_nav_index: 0,
            system_filter_index: 0,
            alpha_filter_index: 0,
            manage_scope_index: 0,
            manage_action_index: 0,
            manage_settings_index: 0,
            manage_scrape_system_index: 0,
            manage_scrape_action_index: 0,
            manage_list_index: 0,
            settings_section_index: 1,
            settings_core_tab_index: 0,
            settings_core_variable_index: 0,
            settings_core_option_index: 0,
            settings_cover_action_index: 0,
            grid_index_home: 0,
            grid_index_library: 0,
            grid_columns_home: 1,
            grid_columns_library: 1,
            grid_visible_rows_home: 1,
            grid_visible_rows_library: 1,
            visible_row_start_home: 0,
            visible_row_end_home: 1,
            visible_row_start_library: 0,
            visible_row_end_library: 1,
            pending_scroll_to_index: None,
            repeat_direction: None,
            repeat_started_at: None,
            next_repeat_at: None,
            south_held: false,
            east_held: false,
            west_held: false,
            left_trigger_held: false,
            right_trigger_held: false,
            left_shoulder_held: false,
            right_shoulder_held: false,
            near_end_load_armed: true,
        }
    }
}

impl MenuNavState {
    pub(crate) fn clear_repeat_state(&mut self) {
        self.repeat_direction = None;
        self.repeat_started_at = None;
        self.next_repeat_at = None;
    }

    pub(crate) fn consume_direction(
        &mut self,
        now: Instant,
        direction: Option<MenuNavDirection>,
    ) -> Option<MenuNavDirection> {
        const HOLD_DELAY: Duration = Duration::from_millis(250);
        const REPEAT_INTERVAL: Duration = Duration::from_millis(100);

        let Some(direction) = direction else {
            self.clear_repeat_state();
            return None;
        };

        if self.repeat_direction != Some(direction) {
            self.repeat_direction = Some(direction);
            self.repeat_started_at = Some(now);
            self.next_repeat_at = Some(now + HOLD_DELAY);
            return Some(direction);
        }

        let Some(next_repeat_at) = self.next_repeat_at else {
            self.next_repeat_at = Some(now + HOLD_DELAY);
            return None;
        };

        if now >= next_repeat_at {
            self.next_repeat_at = Some(now + REPEAT_INTERVAL);
            return Some(direction);
        }

        None
    }

    pub(crate) fn sync_filter_indices(&mut self, system_index: usize, alpha_index: usize) {
        self.system_filter_index = system_index;
        self.alpha_filter_index = alpha_index;
    }

    pub(crate) fn normalize_filter_focus(
        &mut self,
        _current_view: AppView,
        filters_expanded: bool,
        _has_grid: bool,
    ) {
        if filters_expanded {
            return;
        }

        if matches!(
            self.focus_region,
            MenuFocusRegion::FiltersSystem | MenuFocusRegion::FiltersAlpha
        ) {
            self.focus_region = MenuFocusRegion::FiltersToggle;
        }
    }

    pub(crate) fn selected_top_nav_view(&self) -> AppView {
        match self.top_nav_index {
            0 => AppView::Home,
            1 => AppView::Library,
            _ => AppView::Settings,
        }
    }

    pub(crate) fn focus_top_nav_for_view(&mut self, view: AppView) {
        self.top_nav_index = match view {
            AppView::Home => 0,
            AppView::Library => 1,
            AppView::Settings => 2,
        };
        self.focus_region = MenuFocusRegion::TopNav;
    }

    pub(crate) fn move_top_nav(&mut self, direction: MenuNavDirection, current_view: AppView) {
        match direction {
            MenuNavDirection::Left => {
                self.top_nav_index = self.top_nav_index.saturating_sub(1);
            }
            MenuNavDirection::Right => {
                self.top_nav_index = (self.top_nav_index + 1).min(2);
            }
            MenuNavDirection::Down => {
                self.focus_region = if matches!(current_view, AppView::Home | AppView::Library) {
                    MenuFocusRegion::FiltersToggle
                } else {
                    MenuFocusRegion::Grid
                };
            }
            MenuNavDirection::Up => {}
        }
    }

    pub(crate) fn move_system_filter(
        &mut self,
        direction: MenuNavDirection,
        system_last_index: usize,
        alpha_last_index: usize,
    ) {
        match direction {
            MenuNavDirection::Left => {
                self.system_filter_index = self.system_filter_index.saturating_sub(1);
            }
            MenuNavDirection::Right => {
                self.system_filter_index = (self.system_filter_index + 1).min(system_last_index);
            }
            MenuNavDirection::Up => {}
            MenuNavDirection::Down => {
                self.alpha_filter_index = self.alpha_filter_index.min(alpha_last_index);
                self.focus_region = MenuFocusRegion::FiltersAlpha;
            }
        }
    }

    pub(crate) fn move_alpha_filter(&mut self, direction: MenuNavDirection, last_index: usize) {
        match direction {
            MenuNavDirection::Left => {
                self.alpha_filter_index = self.alpha_filter_index.saturating_sub(1);
            }
            MenuNavDirection::Right => {
                self.alpha_filter_index = (self.alpha_filter_index + 1).min(last_index);
            }
            MenuNavDirection::Up => {
                self.focus_region = MenuFocusRegion::FiltersSystem;
            }
            MenuNavDirection::Down => {
                self.focus_region = MenuFocusRegion::Grid;
            }
        }
    }

    pub(crate) fn step_back_focus(&mut self, current_view: AppView, filters_expanded: bool) {
        self.focus_region = match self.focus_region {
            MenuFocusRegion::TopNav => MenuFocusRegion::TopNav,
            MenuFocusRegion::LibraryManageButton => {
                self.top_nav_index = match current_view {
                    AppView::Home => 0,
                    AppView::Library => 1,
                    AppView::Settings => 2,
                };
                MenuFocusRegion::TopNav
            }
            MenuFocusRegion::FiltersToggle => {
                self.top_nav_index = match current_view {
                    AppView::Home => 0,
                    AppView::Library => 1,
                    AppView::Settings => 2,
                };
                MenuFocusRegion::TopNav
            }
            MenuFocusRegion::FiltersSystem => MenuFocusRegion::FiltersToggle,
            MenuFocusRegion::FiltersAlpha => MenuFocusRegion::FiltersSystem,
            MenuFocusRegion::ManageHeader => MenuFocusRegion::TopNav,
            MenuFocusRegion::ManageScope => MenuFocusRegion::ManageHeader,
            MenuFocusRegion::ManageActions => MenuFocusRegion::ManageScope,
            MenuFocusRegion::ManageSettings => MenuFocusRegion::ManageActions,
            MenuFocusRegion::ManageScrapeSystems => MenuFocusRegion::ManageSettings,
            MenuFocusRegion::ManageScrapeActions => MenuFocusRegion::ManageScrapeSystems,
            MenuFocusRegion::ManageList => MenuFocusRegion::ManageScrapeActions,
            MenuFocusRegion::SettingsSectionNav => MenuFocusRegion::TopNav,
            MenuFocusRegion::SettingsRuntimeSetupSystem => MenuFocusRegion::SettingsSectionNav,
            MenuFocusRegion::SettingsRuntimeSetup => MenuFocusRegion::SettingsRuntimeSetupSystem,
            MenuFocusRegion::SettingsAppConfigCoreTab => MenuFocusRegion::TopNav,
            MenuFocusRegion::SettingsAppConfigCoreVariable => {
                if self.settings_core_variable_index == 0 {
                    MenuFocusRegion::SettingsAppConfigCoreTab
                } else {
                    self.settings_core_variable_index =
                        self.settings_core_variable_index.saturating_sub(1);
                    MenuFocusRegion::SettingsAppConfigCoreVariable
                }
            }
            MenuFocusRegion::SettingsAppConfigSave => {
                MenuFocusRegion::SettingsAppConfigCoreVariable
            }
            MenuFocusRegion::SettingsCoverSettings => MenuFocusRegion::SettingsSectionNav,
            MenuFocusRegion::Grid => {
                if matches!(current_view, AppView::Home | AppView::Library) {
                    if filters_expanded {
                        MenuFocusRegion::FiltersAlpha
                    } else {
                        MenuFocusRegion::FiltersToggle
                    }
                } else {
                    self.top_nav_index = match current_view {
                        AppView::Home => 0,
                        AppView::Library => 1,
                        AppView::Settings => 2,
                    };
                    MenuFocusRegion::TopNav
                }
            }
        };
    }

    pub(crate) fn clear_pending_scroll(&mut self) {
        self.pending_scroll_to_index = None;
    }

    pub(crate) fn pending_scroll_index(&self, active_source: bool) -> Option<usize> {
        if active_source {
            self.pending_scroll_to_index
        } else {
            None
        }
    }

    pub(crate) fn request_scroll_to(&mut self, index: usize) {
        self.pending_scroll_to_index = Some(index);
    }

    pub(crate) fn active_grid_index(&self, source: GridSource) -> usize {
        match source {
            GridSource::Favorites => self.grid_index_home,
            GridSource::Library => self.grid_index_library,
        }
    }

    pub(crate) fn set_active_grid_index(&mut self, source: GridSource, index: usize) {
        match source {
            GridSource::Favorites => self.grid_index_home = index,
            GridSource::Library => self.grid_index_library = index,
        }
    }

    pub(crate) fn clamp_grid_index(&self, index: usize, len: usize) -> usize {
        if len == 0 {
            0
        } else {
            index.min(len.saturating_sub(1))
        }
    }

    pub(crate) fn grid_nav_dimensions(&self, source: GridSource) -> (usize, usize) {
        match source {
            GridSource::Favorites => (self.grid_columns_home, self.grid_visible_rows_home),
            GridSource::Library => (self.grid_columns_library, self.grid_visible_rows_library),
        }
    }

    pub(crate) fn record_visible_row_range(
        &mut self,
        source: GridSource,
        start: usize,
        end: usize,
    ) {
        let normalized_end = end.max(start.saturating_add(1));
        match source {
            GridSource::Favorites => {
                self.visible_row_start_home = start;
                self.visible_row_end_home = normalized_end;
            }
            GridSource::Library => {
                self.visible_row_start_library = start;
                self.visible_row_end_library = normalized_end;
            }
        }
    }

    pub(crate) fn should_scroll_to_index(&self, source: GridSource, index: usize) -> bool {
        let (columns, _) = self.grid_nav_dimensions(source);
        let row = index / columns.max(1);
        let (visible_start, visible_end) = match source {
            GridSource::Favorites => (self.visible_row_start_home, self.visible_row_end_home),
            GridSource::Library => (self.visible_row_start_library, self.visible_row_end_library),
        };

        row < visible_start || row >= visible_end
    }

    pub(crate) fn record_grid_metrics(
        &mut self,
        source: GridSource,
        columns: usize,
        visible_rows: usize,
    ) {
        match source {
            GridSource::Favorites => {
                self.grid_columns_home = columns.max(1);
                self.grid_visible_rows_home = visible_rows.max(1);
            }
            GridSource::Library => {
                self.grid_columns_library = columns.max(1);
                self.grid_visible_rows_library = visible_rows.max(1);
            }
        }
    }

    pub(crate) fn should_preload_near_end(
        &mut self,
        source: GridSource,
        len: usize,
        columns: usize,
    ) -> bool {
        if !matches!(source, GridSource::Library) {
            return false;
        }

        if len == 0 {
            self.near_end_load_armed = true;
            return false;
        }

        let near_end = self
            .active_grid_index(source)
            .saturating_add(columns.saturating_mul(2))
            >= len;
        if near_end {
            if self.near_end_load_armed {
                self.near_end_load_armed = false;
                true
            } else {
                false
            }
        } else {
            self.near_end_load_armed = true;
            false
        }
    }

    pub(crate) fn repair_grid_index(
        &mut self,
        source: GridSource,
        len: usize,
    ) -> Option<(usize, bool)> {
        let current_index = self.active_grid_index(source);

        if len == 0 {
            self.set_active_grid_index(source, 0);
            if self.focus_region == MenuFocusRegion::Grid {
                self.clear_pending_scroll();
            }
            return None;
        }

        let repaired_index = current_index.min(len.saturating_sub(1));
        self.set_active_grid_index(source, repaired_index);
        Some((repaired_index, repaired_index != current_index))
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuFocusRegion, MenuNavState};
    use crate::app::GridSource;

    #[test]
    fn repair_grid_index_clamps_selection_and_reports_change() {
        let mut state = MenuNavState {
            grid_index_library: 7,
            ..MenuNavState::default()
        };

        let repaired = state.repair_grid_index(GridSource::Library, 3);

        assert_eq!(repaired, Some((2, true)));
        assert_eq!(state.grid_index_library, 2);
    }

    #[test]
    fn repair_grid_index_clears_pending_scroll_when_grid_becomes_empty() {
        let mut state = MenuNavState {
            focus_region: MenuFocusRegion::Grid,
            pending_scroll_to_index: Some(4),
            grid_index_home: 4,
            ..MenuNavState::default()
        };

        let repaired = state.repair_grid_index(GridSource::Favorites, 0);

        assert_eq!(repaired, None);
        assert_eq!(state.grid_index_home, 0);
        assert_eq!(state.pending_scroll_to_index, None);
    }

    #[test]
    fn should_scroll_to_index_only_when_selection_leaves_visible_rows() {
        let mut state = MenuNavState {
            grid_columns_home: 4,
            ..MenuNavState::default()
        };
        state.record_visible_row_range(GridSource::Favorites, 1, 3);

        assert!(!state.should_scroll_to_index(GridSource::Favorites, 5));
        assert!(!state.should_scroll_to_index(GridSource::Favorites, 11));
        assert!(state.should_scroll_to_index(GridSource::Favorites, 0));
        assert!(state.should_scroll_to_index(GridSource::Favorites, 12));
    }
}
