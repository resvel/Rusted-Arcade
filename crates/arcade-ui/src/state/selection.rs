#[derive(Default)]
pub(crate) struct RomSelectionState {
    selected_rom_id: Option<String>,
}

impl RomSelectionState {
    pub(crate) fn selected_rom_id(&self) -> Option<&str> {
        self.selected_rom_id.as_deref()
    }

    pub(crate) fn selected_rom_id_cloned(&self) -> Option<String> {
        self.selected_rom_id.clone()
    }

    pub(crate) fn clear(&mut self) -> bool {
        self.set(None)
    }

    pub(crate) fn set(&mut self, next: Option<String>) -> bool {
        if self.selected_rom_id == next {
            return false;
        }

        self.selected_rom_id = next;
        true
    }
}
