use arcade_domain::RomQuery;

use crate::app::{AppView, GridSource, NativeArcadeUiApp};
use crate::state::LibraryCacheKey;
use crate::theme::SYSTEM_FILTERS;
use crate::views::rom_grid::grid_metrics_for;

use super::{LIBRARY_BUFFER_ROWS, MIN_LIBRARY_PAGE_SIZE};

impl NativeArcadeUiApp {
    pub(crate) fn refresh_favorites(&mut self) -> anyhow::Result<()> {
        let favorites = self.services.list_favorites()?;
        self.state.library.favorite_ids_all = self.upsert_rom_cards(favorites);
        self.apply_favorites_filters();
        Ok(())
    }

    pub(crate) fn apply_favorites_filters(&mut self) {
        self.state
            .library
            .apply_favorite_filters(&self.state.rom_catalog);
    }

    pub(crate) fn refresh_all(&mut self) -> anyhow::Result<()> {
        self.refresh_favorites()?;
        self.state.library.reset_cache();
        if self.state.library.loaded {
            self.refresh_library()?;
        }
        Ok(())
    }

    pub(crate) fn build_rom_query(&self) -> RomQuery {
        self.state.library.build_query(MIN_LIBRARY_PAGE_SIZE)
    }

    pub(crate) fn desired_library_page_size(
        &self,
        available_width: f32,
        available_height: f32,
    ) -> usize {
        let metrics = grid_metrics_for(GridSource::Library, available_width, available_height);
        let columns = metrics.columns.max(1);
        let visible_rows = metrics.visible_rows.max(1);

        columns
            .saturating_mul(visible_rows.saturating_add(LIBRARY_BUFFER_ROWS))
            .max(MIN_LIBRARY_PAGE_SIZE)
    }

    pub(crate) fn refresh_library(&mut self) -> anyhow::Result<()> {
        let query = self.build_rom_query();
        let cache_key = LibraryCacheKey::from_query(&query);

        if self
            .state
            .library
            .apply_cached_page(&cache_key, query.limit)
        {
            return Ok(());
        }

        let rom_ids = self.upsert_rom_cards(self.services.list_roms(&query)?);
        let exhausted = rom_ids.len() < query.limit;
        self.state
            .library
            .store_first_page(cache_key, rom_ids, exhausted);
        Ok(())
    }

    pub(crate) fn try_load_more_library(&mut self) {
        let Some((query, cache_key, chunk_size)) =
            self.state.library.next_page_query(MIN_LIBRARY_PAGE_SIZE)
        else {
            return;
        };
        let more_ids = match self.services.list_roms(&query) {
            Ok(roms) => self.upsert_rom_cards(roms),
            Err(err) => {
                self.state.status = format!("Failed to load more ROMs: {err}");
                return;
            }
        };

        if more_ids.is_empty() {
            self.state.library.mark_cache_exhausted(&cache_key);
            return;
        }
        self.state
            .library
            .append_page(&cache_key, more_ids, chunk_size);
    }

    pub(crate) fn sync_menu_nav_filter_indices(&mut self) {
        self.state.menu_nav.sync_filter_indices(
            self.current_system_filter_index(),
            self.current_alpha_filter_index(),
        );
    }

    pub(crate) fn apply_current_view_filter_change(
        &mut self,
        system_changed: bool,
        alpha_changed: bool,
        apply_filters: bool,
    ) {
        if !(system_changed || alpha_changed || apply_filters) {
            return;
        }

        self.sync_menu_nav_filter_indices();
        match self.state.current_view {
            AppView::Home => {
                self.state.library.loaded = false;
                self.apply_favorites_filters();
                self.repair_grid_selection(GridSource::Favorites);
            }
            AppView::Library => {
                self.try_refresh_roms();
                self.apply_favorites_filters();
                self.repair_grid_selection(GridSource::Library);
            }
            AppView::Settings => {}
        }

        self.normalize_filters_panel_focus();
    }

    pub(crate) fn cycle_system_filter(&mut self, direction: isize, current_view: AppView) {
        if SYSTEM_FILTERS.is_empty() {
            return;
        }

        let current = self.current_system_filter_index();
        let len = SYSTEM_FILTERS.len() as isize;
        let next = (current as isize + direction).rem_euclid(len) as usize;
        let changed = self.apply_system_filter_by_index(next);
        self.state.menu_nav.system_filter_index = next;
        self.state.current_view = current_view;
        self.apply_current_view_filter_change(changed, false, false);
    }

    pub(crate) fn toggle_selected_rom_favorite(&mut self) -> anyhow::Result<()> {
        let Some(rom) = self.current_selected_rom().cloned() else {
            anyhow::bail!("Select a ROM first.");
        };

        if rom.is_favorite {
            self.services.remove_favorite(&rom.rom.id)?;
            self.state.status = format!("Removed {} from favorites.", rom.display_title);
        } else {
            self.services.add_favorite(&rom.rom.id)?;
            self.state.status = format!("Added {} to favorites.", rom.display_title);
        }

        self.refresh_all()?;
        if let Some(source) = self.current_browse_grid_source() {
            self.repair_grid_selection(source);
        }
        Ok(())
    }

    pub(crate) fn try_refresh_roms(&mut self) {
        match self.refresh_library() {
            Ok(_) => {
                self.state.status.clear();
            }
            Err(err) => {
                self.state.status = format!("Failed to load ROM list: {err}");
            }
        }
    }
}
