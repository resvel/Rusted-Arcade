use tracing::{info, warn};

use crate::app::NativeArcadeUiApp;

impl NativeArcadeUiApp {
    pub(crate) fn launch_selected_rom(&mut self) {
        let Some(rom_id) = self.state.selection.selected_rom_id_cloned() else {
            self.state.play.set_status("Select a ROM first.");
            return;
        };

        match self.services.prepare_launch(&rom_id) {
            Ok(plan) => match self.host.load_for_rom(
                &plan.system,
                plan.effective_core.as_deref(),
                &plan.rom_path,
            ) {
                Ok(core_name) => {
                    info!(
                        system = %plan.system,
                        rom_id = %plan.rom_id,
                        rom_path = %plan.rom_path.display(),
                        core = %core_name,
                        "Started play session"
                    );
                    self.state.play.begin_session(
                        plan.status_message,
                        plan.rom_id,
                        plan.system,
                        core_name,
                    );
                    self.assets.last_frame_texture = None;
                    self.show_play_bar();
                }
                Err(err) => {
                    warn!(
                        system = %plan.system,
                        rom_id = %plan.rom_id,
                        rom_path = %plan.rom_path.display(),
                        requested_core = %plan.effective_core.as_deref().unwrap_or("auto"),
                        error = %err,
                        "Failed to start play session"
                    );
                    if !self.host.is_loaded() {
                        self.state.play.clear_active_launch();
                    }
                    self.state
                        .play
                        .set_status(format!("Failed to start core: {err}"));
                }
            },
            Err(err) => {
                warn!(rom_id = %rom_id, error = %err, "Could not prepare play session");
                self.state
                    .play
                    .set_status(format!("Could not start play session: {err}"));
            }
        }
    }
}
