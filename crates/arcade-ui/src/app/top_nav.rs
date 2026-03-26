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

    /// Paint the title image as a decorative overlay anchored to the left of
    /// the header.  `image_height` controls the rendered image size;
    /// `layout_width` reserves horizontal space so nav buttons don't overlap.
    /// The image is vertically centred on `anchor_rect` and the panel clips
    /// the transparent padding, keeping the header compact.
    fn paint_header_title(
        &mut self,
        ui: &mut egui::Ui,
        anchor_rect: egui::Rect,
        image_height: f32,
        layout_width: f32,
    ) {
        if let Some(texture) = self.header_title_texture(ui.ctx()) {
            let tex_size = texture.size_vec2();
            let aspect = tex_size.x / tex_size.y;
            let draw_width = image_height * aspect;
            let draw_rect = egui::Rect::from_min_size(
                egui::pos2(
                    anchor_rect.left(),
                    anchor_rect.top() - image_height * 0.38,
                ),
                egui::vec2(draw_width, image_height),
            );
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter()
                .image(texture.id(), draw_rect, uv, egui::Color32::WHITE);
            // Reserve horizontal space only (height stays at layout row height)
            ui.allocate_space(egui::vec2(layout_width, 0.0));
        } else {
            let palette = self.palette();
            ui.label(
                egui::RichText::new("Rusted Arcade")
                    .size(18.0)
                    .strong()
                    .color(palette.text),
            );
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
                            ui.set_min_height(if compact_layout { 36.0 } else { 40.0 });
                            ui.spacing_mut().item_spacing = egui::vec2(14.0, 4.0);

                            if compact_layout {
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
                                    self.paint_header_title(
                                        ui, panel_rect, 264.0, 160.0,
                                    );
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
                                            egui::vec2(82.0, 28.0),
                                            12.5,
                                        );
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    self.paint_header_title(
                                        ui, panel_rect, 312.0, 200.0,
                                    );
                                    ui.add_space(12.0);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(10.0, 4.0);
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
                                                    egui::vec2(88.0, 30.0),
                                                    13.0,
                                                );
                                            }
                                        },
                                    );
                                });
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
