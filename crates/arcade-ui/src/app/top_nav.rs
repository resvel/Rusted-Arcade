use eframe::egui;

use crate::state::MenuFocusRegion;

use super::{AppView, NativeArcadeUiApp};

impl NativeArcadeUiApp {
    pub(crate) fn navigate_to_view(&mut self, view: AppView) {
        if view != AppView::Library {
            self.state.manage.open = false;
            self.state.manage.remove_confirmation_armed = false;
        }
        self.state.current_view = view;
        self.state.menu_nav.focus_top_nav_for_view(view);
        if view == AppView::Settings {
            self.sync_manage_settings_from_services();
            self.sync_settings_navigation_indices();
        }
        if let Some(source) = self.current_browse_grid_source() {
            self.repair_grid_selection(source);
        }
    }

    fn draw_top_nav_button(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        view: AppView,
        logical_index: usize,
        min_size: egui::Vec2,
        font_size: f32,
    ) {
        let palette = self.palette();
        let selected = self.state.current_view == view;
        let focused = self.state.menu_nav.focus_region == MenuFocusRegion::TopNav
            && self.state.menu_nav.top_nav_index == logical_index;
        let focus_t = ui
            .ctx()
            .animate_bool(ui.id().with(("top-nav-focus", logical_index)), focused);
        let button = egui::Button::new(egui::RichText::new(label).size(font_size).strong())
            .min_size(min_size)
            .fill(if selected {
                palette.accent_soft
            } else if focused {
                egui::Color32::from_rgba_premultiplied(
                    palette.accent.r(),
                    palette.accent.g(),
                    palette.accent.b(),
                    22,
                )
            } else {
                palette.panel
            })
            .stroke(egui::Stroke::new(
                if focused { 1.6 } else { 1.0 },
                if selected || focused {
                    palette.accent
                } else {
                    palette.border
                },
            ))
            .corner_radius(egui::CornerRadius::same(255));
        let response = ui.add(button);
        let glow_t = if focused { 0.58 + focus_t * 0.42 } else { 0.0 };
        Self::paint_selection_glow(ui, response.rect, 255, palette.accent, glow_t);
        if response.clicked() {
            self.navigate_to_view(view);
        }
    }

    pub(super) fn draw_top_nav(&mut self, ctx: &egui::Context) {
        let width = Self::viewport_width(ctx);
        let chrome_margin = Self::chrome_margin_for_width(width);
        let palette = self.palette();
        egui::TopBottomPanel::top("top-nav")
            .frame(
                egui::Frame::new()
                    .fill(palette.panel_alt)
                    .inner_margin(egui::Margin::same(chrome_margin)),
            )
            .show(ctx, |ui| {
                let panel_rect = ui.max_rect();
                if let Some(texture) = self.header_background_texture(ctx) {
                    let size = texture.size_vec2();
                    if size.x > 0.0 && size.y > 0.0 {
                        let width_scale = panel_rect.width() / size.x;
                        let height_scale = panel_rect.height() / size.y;
                        let scale = width_scale.max(height_scale);
                        let draw_size = egui::vec2(size.x * scale, size.y * scale);
                        let draw_rect = egui::Rect::from_center_size(panel_rect.center(), draw_size);
                        let uv =
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                        ui.painter().image(
                            texture.id(),
                            draw_rect,
                            uv,
                            egui::Color32::from_rgba_premultiplied(255, 255, 255, 52),
                        );
                    }
                }

                let panel_width = ui.available_width();
                let content_width = Self::content_band_width_for(panel_width);
                let side_gutter = ((panel_width - content_width) * 0.5).max(0.0);
                let compact_layout = content_width < 760.0;

                ui.horizontal(|ui| {
                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }

                    ui.allocate_ui_with_layout(
                        egui::vec2(content_width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let show_all_logos = self.active_system() == "ALL";
                            ui.set_min_height(if compact_layout {
                                if show_all_logos {
                                    118.0
                                } else {
                                    88.0
                                }
                            } else if show_all_logos {
                                106.0
                            } else {
                                72.0
                            });
                            ui.spacing_mut().item_spacing = egui::vec2(14.0, 8.0);

                            if compact_layout {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("Rusted Arcade")
                                            .heading()
                                            .strong()
                                            .color(palette.text),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "Default in Game Controls: hold 'ESC' to Exit. Controller Exit, Reset, Save, Load, and slot shortcuts can be customized per system in Library.",
                                        )
                                        .small()
                                        .color(palette.text_muted),
                                    );
                                });
                                ui.add_space(10.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
                                    for (label, view, logical_index) in [
                                        ("Favorites", AppView::Home, 0usize),
                                        ("Library", AppView::Library, 1usize),
                                        ("Settings", AppView::Settings, 2usize),
                                    ] {
                                        self.draw_top_nav_button(
                                            ui,
                                            label,
                                            view,
                                            logical_index,
                                            egui::vec2(92.0, 34.0),
                                            13.0,
                                        );
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Rusted Arcade")
                                                .heading()
                                                .strong()
                                                .color(palette.text),
                                        );
                                        ui.label(
                                            egui::RichText::new(
                                                "Default in Game Controls: hold 'ESC' to Exit. Controller Exit, Reset, Save, Load, and slot shortcuts can be customized per system in Library.",
                                            )
                                            .small()
                                            .color(palette.text_muted),
                                        );
                                    });
                                    ui.add_space(18.0);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
                                            for (label, view, logical_index) in [
                                                ("Settings", AppView::Settings, 2usize),
                                                ("Library", AppView::Library, 1usize),
                                                ("Favorites", AppView::Home, 0usize),
                                            ] {
                                                self.draw_top_nav_button(
                                                    ui,
                                                    label,
                                                    view,
                                                    logical_index,
                                                    egui::vec2(98.0, 36.0),
                                                    13.5,
                                                );
                                            }
                                        },
                                    );
                                });
                            }

                            if show_all_logos {
                                ui.add_space(if compact_layout { 4.0 } else { 6.0 });
                                let banner_height = if compact_layout { 24.0 } else { 28.0 };
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), banner_height),
                                    egui::Sense::hover(),
                                );
                                self.draw_all_systems_header_logos(ui, ctx, rect);
                            }
                        },
                    );

                    if side_gutter > 0.0 {
                        ui.add_space(side_gutter);
                    }
                });
            });
    }
}
