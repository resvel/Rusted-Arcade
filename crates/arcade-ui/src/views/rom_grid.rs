use arcade_domain::RomCard;
use eframe::egui;
use egui::Color32;

use crate::app::{GridSource, NativeArcadeUiApp};
use crate::render::fit_size_to_aspect;
use crate::state::MenuFocusRegion;

const GRID_EDGE_PADDING: f32 = 8.0;
const GRID_SCROLLBAR_GUTTER: f32 = 18.0;
const GRID_RIGHT_SAFETY_INSET: f32 = 24.0;
const GRID_LEFT_SHIFT_BIAS: f32 = 80.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RomGridLayout {
    pub(crate) card_width: f32,
    pub(crate) card_height: f32,
    pub(crate) cover_height: f32,
    pub(crate) card_gap: f32,
    pub(crate) row_gap: f32,
    pub(crate) inner_margin: i8,
    pub(crate) title_max_chars: usize,
    pub(crate) meta_max_chars: usize,
    pub(crate) title_size: f32,
    pub(crate) line_size: f32,
    pub(crate) compact: bool,
    pub(crate) show_metadata: bool,
    pub(crate) show_preview_badge: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GridMetrics {
    pub(crate) layout: RomGridLayout,
    pub(crate) columns: usize,
    pub(crate) visible_rows: usize,
    pub(crate) side_padding: f32,
}

impl RomGridLayout {
    fn full() -> Self {
        Self {
            card_width: 204.0,
            card_height: 336.0,
            cover_height: 246.0,
            card_gap: 10.0,
            row_gap: 12.0,
            inner_margin: 8,
            title_max_chars: 36,
            meta_max_chars: 38,
            title_size: 14.0,
            line_size: 11.5,
            compact: false,
            show_metadata: true,
            show_preview_badge: true,
        }
    }

    fn compact() -> Self {
        Self {
            card_width: 170.0,
            card_height: 258.0,
            cover_height: 182.0,
            card_gap: 8.0,
            row_gap: 10.0,
            inner_margin: 7,
            title_max_chars: 28,
            meta_max_chars: 28,
            title_size: 13.0,
            line_size: 11.0,
            compact: true,
            show_metadata: true,
            show_preview_badge: false,
        }
    }

    fn dense() -> Self {
        Self {
            card_width: 152.0,
            card_height: 220.0,
            cover_height: 144.0,
            card_gap: 8.0,
            row_gap: 8.0,
            inner_margin: 6,
            title_max_chars: 24,
            meta_max_chars: 0,
            title_size: 12.0,
            line_size: 10.0,
            compact: true,
            show_metadata: false,
            show_preview_badge: false,
        }
    }
}

pub(crate) fn resolve_rom_grid_layout(
    _source: GridSource,
    available_width: f32,
    _available_height: f32,
) -> RomGridLayout {
    let full = RomGridLayout::full();
    let compact = RomGridLayout::compact();
    let dense = RomGridLayout::dense();

    let usable_width = (available_width
        - GRID_SCROLLBAR_GUTTER
        - GRID_RIGHT_SAFETY_INSET
        - GRID_EDGE_PADDING * 2.0)
        .max(0.0);

    let fits_width = |layout: RomGridLayout, columns: usize| {
        columns as f32 * layout.card_width + columns.saturating_sub(1) as f32 * layout.card_gap
            <= usable_width
    };

    if fits_width(full, 8) {
        full
    } else if fits_width(compact, 7) {
        compact
    } else {
        dense
    }
}

pub(crate) fn grid_metrics_for(
    source: GridSource,
    available_width: f32,
    available_height: f32,
) -> GridMetrics {
    let layout = resolve_rom_grid_layout(source, available_width, available_height);
    let usable_width = (available_width
        - GRID_SCROLLBAR_GUTTER
        - GRID_RIGHT_SAFETY_INSET
        - GRID_EDGE_PADDING * 2.0)
        .max(layout.card_width);
    let max_columns = if matches!(source, GridSource::Favorites | GridSource::Library) {
        9
    } else {
        usize::MAX
    };
    let columns = (((usable_width + layout.card_gap) / (layout.card_width + layout.card_gap))
        .floor()
        .max(1.0) as usize)
        .min(max_columns);
    let visible_rows = (((available_height + layout.row_gap)
        / (layout.card_height + layout.row_gap))
        .ceil()
        .max(1.0) as usize)
        .max(1);
    let content_width =
        columns as f32 * layout.card_width + columns.saturating_sub(1) as f32 * layout.card_gap;
    let side_padding = ((usable_width - content_width) * 0.5).max(0.0);

    GridMetrics {
        layout,
        columns,
        visible_rows,
        side_padding,
    }
}

impl NativeArcadeUiApp {
    pub(crate) fn repair_grid_selection(&mut self, source: GridSource) {
        let len = self.grid_len(source);
        let Some((repaired_index, index_changed)) =
            self.state.menu_nav.repair_grid_index(source, len)
        else {
            self.state.selection.clear();
            return;
        };

        let selection_changed = if let Some(rom) = self.grid_item(source, repaired_index) {
            self.state.selection.set(Some(rom.rom.id))
        } else {
            self.state.selection.clear()
        };

        if index_changed || selection_changed {
            self.state.menu_nav.request_scroll_to(repaired_index);
        }
    }

    pub(crate) fn active_grid_index(&self, source: GridSource) -> usize {
        self.state.menu_nav.active_grid_index(source)
    }

    pub(crate) fn set_active_grid_index(&mut self, source: GridSource, index: usize) {
        self.state.menu_nav.set_active_grid_index(source, index);
    }

    pub(crate) fn clamp_grid_index(&self, source: GridSource, index: usize) -> usize {
        self.state
            .menu_nav
            .clamp_grid_index(index, self.grid_len(source))
    }

    pub(crate) fn sync_selected_rom_from_grid_index(&mut self, source: GridSource) {
        let index = self.clamp_grid_index(source, self.active_grid_index(source));
        if let Some(rom) = self.grid_item(source, index) {
            self.state.selection.set(Some(rom.rom.id));
        } else {
            self.state.selection.clear();
        }
    }

    pub(crate) fn move_grid_selection_by_delta(
        &mut self,
        source: GridSource,
        delta: isize,
        metrics: GridMetrics,
    ) {
        self.state
            .menu_nav
            .record_grid_metrics(source, metrics.columns, metrics.visible_rows);
        let len = self.grid_len(source);
        if len == 0 {
            self.repair_grid_selection(source);
            return;
        }

        let current = self.active_grid_index(source) as isize;
        let last = len.saturating_sub(1) as isize;
        let next = (current + delta).clamp(0, last) as usize;
        if next != self.active_grid_index(source) {
            self.set_active_grid_index(source, next);
            self.sync_selected_rom_from_grid_index(source);
            if self.state.menu_nav.should_scroll_to_index(source, next) {
                self.state.menu_nav.request_scroll_to(next);
            }
        }
        self.maybe_preload_near_end_for_selection(source, metrics.columns);
    }

    pub(crate) fn page_grid_selection(
        &mut self,
        source: GridSource,
        direction: isize,
        metrics: GridMetrics,
    ) {
        let page = metrics.columns.saturating_mul(metrics.visible_rows.max(1)) as isize;
        self.move_grid_selection_by_delta(source, page.saturating_mul(direction), metrics);
    }

    pub(crate) fn maybe_preload_near_end_for_selection(
        &mut self,
        source: GridSource,
        columns: usize,
    ) {
        let len = self.grid_len(source);
        if self
            .state
            .menu_nav
            .should_preload_near_end(source, len, columns)
        {
            self.try_load_more_library();
        }
    }

    pub(crate) fn grid_nav_metrics(&self, source: GridSource) -> GridMetrics {
        let (columns, visible_rows) = self.state.menu_nav.grid_nav_dimensions(source);
        GridMetrics {
            layout: RomGridLayout::compact(),
            columns: columns.max(1),
            visible_rows: visible_rows.max(1),
            side_padding: 0.0,
        }
    }

    pub(crate) fn grid_len(&self, source: GridSource) -> usize {
        self.current_visible_grid_ids(source).len()
    }

    pub(crate) fn grid_item(&self, source: GridSource, index: usize) -> Option<RomCard> {
        let rom_id = self.current_visible_grid_ids(source).get(index)?;
        self.rom_by_id(rom_id).cloned()
    }

    pub(crate) fn draw_rom_grid(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        source: GridSource,
        grid_id: &str,
    ) {
        self.begin_cover_load_frame();
        let section_width = ui.available_width();
        let section_height = ui.available_height();
        let content_width = Self::content_band_width_for(section_width);
        let side_gutter = ((section_width - content_width) * 0.5).max(0.0);

        ui.horizontal(|ui| {
            if side_gutter > 0.0 {
                ui.add_space(side_gutter);
            }

            ui.allocate_ui_with_layout(
                egui::vec2(content_width, section_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let metrics =
                        grid_metrics_for(source, ui.available_width(), ui.available_height());
                    self.state.menu_nav.record_grid_metrics(
                        source,
                        metrics.columns,
                        metrics.visible_rows,
                    );
                    self.repair_grid_selection(source);
                    let layout = metrics.layout;
                    let columns = metrics.columns;
                    let total = self.grid_len(source);
                    let total_rows = total.div_ceil(columns);
                    let mut needs_refresh = false;
                    let mut min_visible_row_start = None;
                    let mut max_visible_row_end = 0usize;
                    let row_height = layout.card_height + layout.row_gap;
                    let pending_scroll_index = self
                        .state
                        .menu_nav
                        .pending_scroll_index(self.current_browse_grid_source() == Some(source));

                    let mut scroll_area = egui::ScrollArea::vertical().id_salt(grid_id);
                    if let Some(index) = pending_scroll_index {
                        scroll_area = scroll_area
                            .vertical_scroll_offset((index / columns) as f32 * row_height);
                    }

                    scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
                        min_visible_row_start.get_or_insert(row_range.start);
                        max_visible_row_end = max_visible_row_end.max(row_range.end);
                        let shift_left = metrics.side_padding.min(GRID_LEFT_SHIFT_BIAS);
                        let left_inset = GRID_EDGE_PADDING + metrics.side_padding - shift_left;
                        let right_inset = GRID_EDGE_PADDING + metrics.side_padding + shift_left;
                        for row in row_range {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add_space(left_inset);
                                for col in 0..columns {
                                    let index = row * columns + col;
                                    if let Some(rom) = self.grid_item(source, index) {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(layout.card_width, layout.card_height),
                                            egui::Layout::top_down(egui::Align::LEFT),
                                            |ui| {
                                                if self.draw_rom_card(
                                                    ctx, ui, source, index, &rom, layout,
                                                ) {
                                                    needs_refresh = true;
                                                }
                                            },
                                        );
                                    } else {
                                        ui.allocate_space(egui::vec2(
                                            layout.card_width,
                                            layout.card_height,
                                        ));
                                    }

                                    if col + 1 < columns {
                                        ui.add_space(layout.card_gap);
                                    }
                                }
                                ui.add_space(right_inset);
                            });
                            ui.add_space(layout.row_gap);
                        }
                    });
                    if pending_scroll_index.is_some() {
                        self.state.menu_nav.clear_pending_scroll();
                    }
                    let visible_row_start = min_visible_row_start.unwrap_or(0);
                    let visible_row_end = if total_rows == 0 {
                        0
                    } else {
                        max_visible_row_end.max(visible_row_start.saturating_add(1))
                    };
                    self.state.menu_nav.record_visible_row_range(
                        source,
                        visible_row_start,
                        visible_row_end,
                    );

                    if matches!(source, GridSource::Library) {
                        let near_end = max_visible_row_end.saturating_add(2) >= total_rows;
                        if near_end {
                            self.try_load_more_library();
                        }
                    }
                    self.maybe_preload_near_end_for_selection(source, columns);

                    if needs_refresh {
                        let _ = self.refresh_all();
                        self.repair_grid_selection(source);
                    }
                },
            );

            if side_gutter > 0.0 {
                ui.add_space(side_gutter);
            }
        });
    }

    pub(crate) fn draw_rom_card(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        source: GridSource,
        index: usize,
        rom: &RomCard,
        layout: RomGridLayout,
    ) -> bool {
        let palette = self.palette();
        let selected = self.state.selection.selected_rom_id() == Some(rom.rom.id.as_str());
        let focus_selected = selected
            && self.state.menu_nav.focus_region == MenuFocusRegion::Grid
            && self.current_browse_grid_source() == Some(source);
        let mut needs_refresh = false;
        let card_id = ui.id().with(("rom-card", rom.rom.id.as_str()));
        let card_response = ui.interact(ui.max_rect(), card_id, egui::Sense::click());
        let hovered = card_response.hovered();
        let interaction_t = ui.ctx().animate_bool(card_id, selected || hovered);
        let fill = blend_color(
            palette.panel,
            palette.panel_alt,
            if selected {
                if focus_selected {
                    0.84
                } else {
                    0.78
                }
            } else {
                interaction_t * 0.55
            },
        );
        let stroke_color = blend_color(
            palette.border,
            palette.accent,
            if selected {
                if focus_selected {
                    1.0
                } else {
                    0.92
                }
            } else {
                interaction_t * 0.7
            },
        );
        let mut favorite_consumed = false;

        let card_frame = egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(
                1.0 + interaction_t * 0.45
                    + if selected { 0.15 } else { 0.0 }
                    + if focus_selected { 0.4 } else { 0.0 },
                stroke_color,
            ))
            .corner_radius(egui::CornerRadius::same(if layout.compact {
                12
            } else {
                16
            }))
            .inner_margin(egui::Margin::same(layout.inner_margin))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().item_spacing.y = if layout.compact { 5.0 } else { 7.0 };
                ui.set_width(layout.card_width - 4.0);
                ui.set_min_height(layout.card_height - 4.0);

                let cover_slot_max_size = egui::vec2(
                    layout.card_width - (layout.inner_margin as f32 * 2.0 + 4.0),
                    layout.cover_height,
                );
                let cover_slot_size = fit_size_to_aspect(
                    cover_slot_max_size,
                    preferred_cover_aspect_for_system(&rom.rom.system),
                );
                let mut cover_rect = egui::Rect::NOTHING;
                ui.allocate_ui_with_layout(
                    cover_slot_max_size,
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    |ui| {
                        let cover_frame = egui::Frame::new()
                            .fill(blend_color(
                                Color32::from_rgba_premultiplied(6, 10, 18, 210),
                                palette.accent_soft,
                                interaction_t * 0.18,
                            ))
                            .corner_radius(egui::CornerRadius::same(12))
                            .show(ui, |ui| {
                                ui.allocate_ui_with_layout(
                                    cover_slot_size,
                                    egui::Layout::centered_and_justified(
                                        egui::Direction::LeftToRight,
                                    ),
                                    |ui| {
                                        if let Some(texture) = self.rom_cover_texture(ctx, rom) {
                                            let texture_size = texture.size_vec2();
                                            let aspect_ratio = if texture_size.y > 0.0 {
                                                texture_size.x / texture_size.y
                                            } else {
                                                1.0
                                            };
                                            let draw_size =
                                                fit_size_to_aspect(cover_slot_size, aspect_ratio);
                                            ui.add(egui::Image::new((texture.id(), draw_size)));
                                        } else {
                                            let rect = ui.max_rect();
                                            ui.painter().rect_filled(
                                                rect,
                                                egui::CornerRadius::same(10),
                                                Color32::from_rgba_premultiplied(10, 14, 22, 240),
                                            );
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "ART COMING SOON",
                                                egui::FontId::proportional(12.5),
                                                palette.text_muted,
                                            );
                                        }
                                    },
                                );
                            });
                        cover_rect = cover_frame.response.rect;
                    },
                );

                if layout.show_preview_badge && rom.rom.preview_video_path.is_some() {
                    let preview_size = egui::vec2(if layout.compact { 56.0 } else { 64.0 }, 20.0);
                    let preview_rect = egui::Rect::from_min_size(
                        cover_rect.min + egui::vec2(8.0, 8.0),
                        preview_size,
                    );
                    ui.painter().rect_filled(
                        preview_rect,
                        egui::CornerRadius::same(255),
                        blend_color(palette.panel_alt, palette.accent_soft, 0.55),
                    );
                    ui.painter().text(
                        preview_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Preview",
                        egui::FontId::proportional(layout.line_size),
                        palette.text,
                    );
                }

                let favorite_size = if layout.compact {
                    egui::vec2(28.0, 24.0)
                } else {
                    egui::vec2(30.0, 26.0)
                };
                let favorite_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        cover_rect.right() - favorite_size.x - 8.0,
                        cover_rect.top() + 8.0,
                    ),
                    favorite_size,
                );
                let favorite_response = ui
                    .interact(
                        favorite_rect,
                        ui.id().with(("favorite-chip", rom.rom.id.as_str())),
                        egui::Sense::click(),
                    )
                    .on_hover_text(if rom.is_favorite {
                        "Remove Favorite"
                    } else {
                        "Add Favorite"
                    });
                let favorite_hover_t = ui.ctx().animate_bool(
                    ui.id().with(("favorite-chip-hover", rom.rom.id.as_str())),
                    favorite_response.hovered(),
                );
                let favorite_fill = if rom.is_favorite {
                    blend_color(
                        blend_color(palette.accent_soft, palette.accent, 0.18),
                        palette.accent,
                        favorite_hover_t * 0.16,
                    )
                } else {
                    blend_color(
                        blend_color(palette.panel, palette.panel_alt, 0.45),
                        palette.accent_soft,
                        favorite_hover_t * 0.4,
                    )
                };
                let favorite_stroke = if rom.is_favorite {
                    palette.accent
                } else {
                    blend_color(
                        palette.border,
                        palette.accent,
                        interaction_t * 0.35 + favorite_hover_t * 0.25,
                    )
                };
                ui.painter().rect_filled(
                    favorite_rect,
                    egui::CornerRadius::same(255),
                    favorite_fill,
                );
                ui.painter().rect_stroke(
                    favorite_rect,
                    egui::CornerRadius::same(255),
                    egui::Stroke::new(1.0, favorite_stroke),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    favorite_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if rom.is_favorite { "★" } else { "☆" },
                    egui::FontId::proportional(if layout.compact { 13.0 } else { 14.0 }),
                    palette.text,
                );
                if favorite_response.clicked() {
                    favorite_consumed = true;
                    self.state.menu_nav.focus_region = MenuFocusRegion::Grid;
                    self.set_active_grid_index(source, index);
                    self.state.selection.set(Some(rom.rom.id.clone()));
                    let result = if rom.is_favorite {
                        self.services.remove_favorite(&rom.rom.id)
                    } else {
                        self.services.add_favorite(&rom.rom.id)
                    };
                    match result {
                        Ok(_) => needs_refresh = true,
                        Err(err) => {
                            self.state.status = if rom.is_favorite {
                                format!("Failed to remove favorite: {err}")
                            } else {
                                format!("Failed to favorite ROM: {err}")
                            };
                        }
                    }
                }

                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(6, 10, 18, 212))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(
                        8,
                        if layout.compact { 5 } else { 6 },
                    ))
                    .show(ui, |ui| {
                        ui.set_width(cover_slot_max_size.x);
                        ui.label(
                            egui::RichText::new(truncate_text(
                                &rom.display_title,
                                layout.title_max_chars,
                            ))
                            .size(layout.title_size)
                            .strong()
                            .color(palette.text),
                        );
                    });

                egui::Frame::new()
                    .fill(Color32::from_rgba_premultiplied(6, 10, 18, 220))
                    .stroke(egui::Stroke::new(
                        1.0,
                        blend_color(palette.border, palette.accent, 0.18),
                    ))
                    .corner_radius(egui::CornerRadius::same(255))
                    .inner_margin(egui::Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(rom.rom.system.as_str())
                                .size((layout.line_size - 1.0).max(9.0))
                                .strong()
                                .color(palette.accent),
                        );
                    });

                if layout.show_metadata {
                    if let Some(meta_line) = rom_metadata_line(rom) {
                        let meta_visible = selected || hovered;
                        let meta_t = ui.ctx().animate_bool(
                            ui.id().with(("rom-meta", rom.rom.id.as_str())),
                            meta_visible,
                        );
                        let max_chars = if layout.meta_max_chars == 0 {
                            layout.title_max_chars
                        } else {
                            layout.meta_max_chars
                        };
                        if meta_t > 0.0 {
                            ui.label(
                                egui::RichText::new(truncate_text(&meta_line, max_chars))
                                    .size(layout.line_size)
                                    .color(scale_color_alpha(palette.text_muted, meta_t)),
                            );
                        }
                    }
                }
            });
        let card_rect = card_frame.response.rect;
        let glow_t = if focus_selected { 1.0 } else { 0.0 };
        if glow_t > 0.0 {
            Self::paint_selection_glow(
                ui,
                card_rect,
                if layout.compact { 12 } else { 16 },
                palette.accent,
                0.78 + glow_t * 0.22,
            );
        }

        if !favorite_consumed && card_response.clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::Grid;
            self.set_active_grid_index(source, index);
            self.state.selection.set(Some(rom.rom.id.clone()));
        }
        if !favorite_consumed && card_response.double_clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::Grid;
            self.set_active_grid_index(source, index);
            self.state.selection.set(Some(rom.rom.id.clone()));
            self.launch_selected_rom();
        }

        needs_refresh
    }
}

fn rom_metadata_line(rom: &RomCard) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(year) = rom.release_year {
        parts.push(year.to_string());
    }
    if let Some(value) = rom.manufacturer.as_deref().map(str::trim) {
        if !value.is_empty() {
            parts.push(value.to_string());
        }
    }
    if let Some(value) = rom.genre.as_deref().map(str::trim) {
        if !value.is_empty() {
            parts.push(value.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" • "))
    }
}

fn preferred_cover_aspect_for_system(system: &str) -> f32 {
    match system {
        "NES" => 0.72,
        "SNES" => 1.28,
        "GENESIS" => 0.8,
        "GB" => 0.74,
        "GBA" => 1.22,
        "N64" => 0.86,
        "ARCADE" => 0.78,
        "SATURN" => 0.8,
        "PCECD" => 0.8,
        _ => 0.8,
    }
}

fn blend_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };

    Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
}

fn scale_color_alpha(color: Color32, factor: f32) -> Color32 {
    let alpha = (color.a() as f32 * factor.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha)
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        if let Some(ch) = chars.next() {
            out.push(ch);
        } else {
            return out;
        }
    }
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}
