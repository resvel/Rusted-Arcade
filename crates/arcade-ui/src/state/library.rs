use std::collections::HashMap;

use arcade_domain::{RomCard, RomQuery};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LibraryCacheKey {
    pub(crate) system: Option<String>,
    pub(crate) search: Option<String>,
    pub(crate) alpha: Option<String>,
}

impl LibraryCacheKey {
    pub(crate) fn from_query(query: &RomQuery) -> Self {
        Self {
            system: query.system.clone(),
            search: query.search.clone(),
            alpha: query.alpha.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LibraryCacheEntry {
    pub(crate) rom_ids: Vec<String>,
    pub(crate) exhausted: bool,
}

pub(crate) struct LibraryState {
    pub(crate) favorite_ids_all: Vec<String>,
    pub(crate) favorite_ids_filtered: Vec<String>,
    pub(crate) visible_rom_ids: Vec<String>,
    pub(crate) cache: HashMap<LibraryCacheKey, LibraryCacheEntry>,
    pub(crate) search: String,
    pub(crate) system_filter: String,
    pub(crate) alpha_filter: String,
    pub(crate) filters_panel_collapsed: bool,
    pub(crate) loaded: bool,
    pub(crate) page_size: usize,
    pub(crate) launch_options_open: bool,
    pub(crate) launch_core_choices: HashMap<String, String>,
}

impl LibraryState {
    pub(crate) fn apply_favorite_filters(&mut self, rom_catalog: &HashMap<String, RomCard>) {
        let system_filter = self.system_filter.trim().to_ascii_uppercase();
        let alpha_filter = self.alpha_filter.trim().to_ascii_uppercase();
        let search_filter = self.search.trim().to_ascii_lowercase();

        self.favorite_ids_filtered = self
            .favorite_ids_all
            .iter()
            .filter_map(|rom_id| rom_catalog.get(rom_id))
            .filter(|rom| {
                if system_filter != "ALL"
                    && !system_filter.is_empty()
                    && !rom.rom.system.eq_ignore_ascii_case(&system_filter)
                {
                    return false;
                }

                if alpha_filter != "ALL" && !alpha_filter.is_empty() {
                    let first = rom
                        .display_title
                        .trim_start()
                        .chars()
                        .next()
                        .unwrap_or_default();
                    if alpha_filter == "0-9" {
                        if !first.is_ascii_digit() {
                            return false;
                        }
                    } else if alpha_filter.len() == 1 {
                        let desired = alpha_filter.chars().next().unwrap_or_default();
                        if !first.eq_ignore_ascii_case(&desired) {
                            return false;
                        }
                    }
                }

                if !search_filter.is_empty() {
                    let display = rom.display_title.to_ascii_lowercase();
                    let title = rom.rom.title.to_ascii_lowercase();
                    if !display.contains(&search_filter) && !title.contains(&search_filter) {
                        return false;
                    }
                }

                true
            })
            .map(|rom| rom.rom.id.clone())
            .collect();
    }

    pub(crate) fn reset_cache(&mut self) {
        self.cache.clear();
    }

    pub(crate) fn set_page_size(&mut self, next_page_size: usize) -> bool {
        if self.page_size == next_page_size {
            return false;
        }

        self.page_size = next_page_size;
        true
    }

    pub(crate) fn build_query(&self, minimum_page_size: usize) -> RomQuery {
        RomQuery {
            system: if self.system_filter == "ALL" {
                None
            } else {
                Some(self.system_filter.clone())
            },
            search: if self.search.trim().is_empty() {
                None
            } else {
                Some(self.search.trim().to_string())
            },
            alpha: if self.alpha_filter == "ALL" {
                None
            } else {
                Some(self.alpha_filter.clone())
            },
            favorites_only: false,
            limit: self.page_size.max(minimum_page_size),
            offset: 0,
        }
    }

    pub(crate) fn apply_cached_page(&mut self, cache_key: &LibraryCacheKey, limit: usize) -> bool {
        let Some(cached) = self.cache.get(cache_key) else {
            return false;
        };

        if cached.rom_ids.len() < limit && !cached.exhausted {
            return false;
        }

        self.visible_rom_ids = cached.rom_ids.clone();
        self.loaded = true;
        true
    }

    pub(crate) fn next_page_query(
        &self,
        minimum_page_size: usize,
    ) -> Option<(RomQuery, LibraryCacheKey, usize)> {
        if !self.loaded {
            return None;
        }

        let mut query = self.build_query(minimum_page_size);
        let cache_key = LibraryCacheKey::from_query(&query);
        let chunk_size = query.limit.max(minimum_page_size);
        let entry = self.cache.get(&cache_key)?;
        if entry.exhausted {
            return None;
        }

        query.offset = entry.rom_ids.len();
        query.limit = chunk_size;
        Some((query, cache_key, chunk_size))
    }

    pub(crate) fn store_first_page(
        &mut self,
        cache_key: LibraryCacheKey,
        rom_ids: Vec<String>,
        exhausted: bool,
    ) {
        self.cache.insert(
            cache_key,
            LibraryCacheEntry {
                rom_ids: rom_ids.clone(),
                exhausted,
            },
        );
        self.visible_rom_ids = rom_ids;
        self.loaded = true;
    }

    pub(crate) fn mark_cache_exhausted(&mut self, cache_key: &LibraryCacheKey) {
        if let Some(entry) = self.cache.get_mut(cache_key) {
            entry.exhausted = true;
        }
    }

    pub(crate) fn append_page(
        &mut self,
        cache_key: &LibraryCacheKey,
        more_ids: Vec<String>,
        chunk_size: usize,
    ) {
        let Some(entry) = self.cache.get_mut(cache_key) else {
            return;
        };

        if more_ids.len() < chunk_size {
            entry.exhausted = true;
        }

        entry.rom_ids.extend(more_ids);
        self.visible_rom_ids = entry.rom_ids.clone();
    }
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            favorite_ids_all: Vec::new(),
            favorite_ids_filtered: Vec::new(),
            visible_rom_ids: Vec::new(),
            cache: HashMap::new(),
            search: String::new(),
            system_filter: String::from("ALL"),
            alpha_filter: String::from("ALL"),
            filters_panel_collapsed: false,
            loaded: false,
            page_size: 0,
            launch_options_open: false,
            launch_core_choices: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arcade_domain::{Rom, RomCard};

    use super::{LibraryCacheKey, LibraryState};

    #[test]
    fn favorite_filters_apply_system_alpha_and_search_together() {
        let mut state = LibraryState {
            favorite_ids_all: vec!["1".into(), "2".into(), "3".into()],
            system_filter: "NES".into(),
            alpha_filter: "A".into(),
            search: "adven".into(),
            ..LibraryState::default()
        };
        let rom_catalog = HashMap::from([
            ("1".into(), rom_card("1", "NES", "Alpha Adventure")),
            ("2".into(), rom_card("2", "NES", "Beta Battle")),
            ("3".into(), rom_card("3", "SNES", "Alpha Adventure")),
        ]);

        state.apply_favorite_filters(&rom_catalog);

        assert_eq!(state.favorite_ids_filtered, vec![String::from("1")]);
    }

    #[test]
    fn apply_cached_page_requires_full_limit_unless_cache_is_exhausted() {
        let cache_key = LibraryCacheKey {
            system: Some("NES".into()),
            search: None,
            alpha: None,
        };
        let mut state = LibraryState::default();
        state.cache.insert(
            cache_key.clone(),
            super::LibraryCacheEntry {
                rom_ids: vec!["1".into(), "2".into()],
                exhausted: false,
            },
        );

        assert!(!state.apply_cached_page(&cache_key, 3));
        assert!(state.visible_rom_ids.is_empty());

        state.mark_cache_exhausted(&cache_key);

        assert!(state.apply_cached_page(&cache_key, 3));
        assert_eq!(
            state.visible_rom_ids,
            vec![String::from("1"), String::from("2")]
        );
        assert!(state.loaded);
    }

    #[test]
    fn next_page_query_uses_cached_length_as_offset_and_page_limit() {
        let mut state = LibraryState {
            loaded: true,
            page_size: 5,
            system_filter: "NES".into(),
            ..LibraryState::default()
        };
        let query = state.build_query(3);
        let cache_key = LibraryCacheKey::from_query(&query);
        state.store_first_page(cache_key, vec!["1".into(), "2".into()], false);

        let (next_query, _, chunk_size) = state.next_page_query(3).expect("next page query");

        assert_eq!(next_query.offset, 2);
        assert_eq!(next_query.limit, 5);
        assert_eq!(chunk_size, 5);
        assert_eq!(next_query.system.as_deref(), Some("NES"));
    }

    #[test]
    fn append_page_marks_cache_exhausted_when_result_is_short() {
        let mut state = LibraryState {
            loaded: true,
            ..LibraryState::default()
        };
        let cache_key = LibraryCacheKey {
            system: None,
            search: None,
            alpha: None,
        };
        state.store_first_page(cache_key.clone(), vec!["1".into(), "2".into()], false);

        state.append_page(&cache_key, vec!["3".into()], 4);

        assert_eq!(
            state.visible_rom_ids,
            vec![String::from("1"), String::from("2"), String::from("3")]
        );
        assert!(state.cache.get(&cache_key).expect("cache entry").exhausted);
    }

    fn rom_card(id: &str, system: &str, title: &str) -> RomCard {
        RomCard {
            rom: Rom {
                id: id.into(),
                system: system.into(),
                slug: id.into(),
                title: title.into(),
                file_path: format!("{id}.rom"),
                emulator_core: None,
                cover_path: None,
                preview_video_path: None,
                preview_poster_path: None,
                preview_duration_sec: None,
                preview_updated_at: None,
                added_at: None,
                updated_at: None,
            },
            display_title: title.into(),
            release_year: None,
            manufacturer: None,
            genre: None,
            is_favorite: true,
        }
    }
}
