use arcade_domain::RomCard;

use crate::app::{GridSource, NativeArcadeUiApp};

impl NativeArcadeUiApp {
    pub(crate) fn upsert_rom_cards<I>(&mut self, cards: I) -> Vec<String>
    where
        I: IntoIterator<Item = RomCard>,
    {
        let mut ids = Vec::new();
        for card in cards {
            let id = card.rom.id.clone();
            self.state.rom_catalog.insert(id.clone(), card);
            ids.push(id);
        }
        ids
    }

    pub(crate) fn rom_by_id(&self, id: &str) -> Option<&RomCard> {
        self.state.rom_catalog.get(id)
    }

    pub(crate) fn current_visible_grid_ids(&self, source: GridSource) -> &[String] {
        match source {
            GridSource::Favorites => &self.state.library.favorite_ids_filtered,
            GridSource::Library => &self.state.library.visible_rom_ids,
        }
    }

    pub(crate) fn current_browse_grid_source(&self) -> Option<GridSource> {
        match self.state.current_view {
            crate::app::AppView::Home => Some(GridSource::Favorites),
            crate::app::AppView::Library => {
                if self.state.manage.open {
                    None
                } else {
                    Some(GridSource::Library)
                }
            }
            crate::app::AppView::Settings => None,
        }
    }

    pub(crate) fn current_selected_rom(&self) -> Option<&RomCard> {
        let id = self.state.selection.selected_rom_id()?;
        self.rom_by_id(id)
    }
}
