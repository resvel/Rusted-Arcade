use arcade_domain::RomCard;
use eframe::egui;
use egui::Color32;

use crate::app::{GridSource, NativeArcadeUiApp};
use crate::state::MenuFocusRegion;
use crate::theme::ThemePalette;

const GRID_EDGE_PADDING: f32 = 0.0;
const GRID_SCROLLBAR_GUTTER: f32 = 18.0;
const GRID_RIGHT_SAFETY_INSET: f32 = 8.0;
const GRID_LEFT_SHIFT_BIAS: f32 = 0.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RomGridLayout {
    pub(crate) card_width: f32,
    pub(crate) card_height: f32,
    pub(crate) card_gap: f32,
    pub(crate) row_gap: f32,
    pub(crate) title_max_chars: usize,
    pub(crate) title_size: f32,
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
            card_width: 224.0,
            card_height: 224.0,
            card_gap: 8.0,
            row_gap: 8.0,
            title_max_chars: 44,
            title_size: 15.5,
        }
    }

    fn compact() -> Self {
        Self {
            card_width: 208.0,
            card_height: 208.0,
            card_gap: 7.0,
            row_gap: 7.0,
            title_max_chars: 40,
            title_size: 15.0,
        }
    }

    fn dense() -> Self {
        Self {
            card_width: 192.0,
            card_height: 192.0,
            card_gap: 6.0,
            row_gap: 6.0,
            title_max_chars: 36,
            title_size: 14.5,
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

    if fits_width(full, 10) {
        full
    } else if fits_width(compact, 8) {
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
    let columns = ((usable_width + layout.card_gap) / (layout.card_width + layout.card_gap))
        .floor()
        .max(1.0) as usize;
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
        ui.allocate_ui_with_layout(
            egui::vec2(section_width, section_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let metrics = grid_metrics_for(source, ui.available_width(), ui.available_height());
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
                    scroll_area =
                        scroll_area.vertical_scroll_offset((index / columns) as f32 * row_height);
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
                                    if self.draw_rom_tile(ctx, ui, source, index, &rom, layout) {
                                        needs_refresh = true;
                                    }
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
    }

    pub(crate) fn draw_rom_tile(
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
        let (tile_rect, tile_response) = ui.allocate_exact_size(
            egui::vec2(layout.card_width, layout.card_height),
            egui::Sense::click(),
        );
        let tile_id = ui.id().with(("rom-tile", rom.rom.id.as_str()));
        let hovered = tile_response.hovered();
        let interaction_t = ui.ctx().animate_bool(tile_id, selected || hovered);
        let cover_texture = self.rom_cover_texture(ctx, rom);
        let has_art = cover_texture.is_some();
        let title_t = ui.ctx().animate_bool(
            ui.id().with(("rom-tile-title", rom.rom.id.as_str())),
            has_art && (hovered || focus_selected),
        );
        let mut favorite_consumed = false;
        let corner_radius = egui::CornerRadius::same(4);

        ui.painter().rect_filled(
            tile_rect,
            corner_radius,
            blend_color(palette.panel, palette.panel_alt, interaction_t * 0.45),
        );

        if let Some(texture) = cover_texture {
            let uv = center_crop_uv(texture.size_vec2(), tile_rect.size());
            ui.painter()
                .image(texture.id(), tile_rect, uv, Color32::WHITE);
        } else {
            paint_missing_art_tile(ui, tile_rect, &rom.display_title, layout, palette);
        }

        if title_t > 0.0 {
            let scrim_height = (layout.card_height * 0.32).clamp(34.0, 46.0);
            let scrim_rect = egui::Rect::from_min_max(
                egui::pos2(tile_rect.left(), tile_rect.bottom() - scrim_height),
                tile_rect.right_bottom(),
            );
            ui.painter().rect_filled(
                scrim_rect,
                egui::CornerRadius::same(0),
                Color32::from_rgba_premultiplied(5, 8, 14, (198.0 * title_t) as u8),
            );
            ui.painter().text(
                scrim_rect.shrink2(egui::vec2(9.0, 0.0)).center(),
                egui::Align2::CENTER_CENTER,
                truncate_text(&rom.display_title, layout.title_max_chars),
                egui::FontId::proportional(layout.title_size),
                scale_color_alpha(palette.text, title_t),
            );
        }

        let stroke_t = if selected {
            if focus_selected {
                1.0
            } else {
                0.76
            }
        } else {
            interaction_t * 0.52
        };
        ui.painter().rect_stroke(
            tile_rect,
            corner_radius,
            egui::Stroke::new(
                1.0 + stroke_t * 0.9,
                blend_color(palette.border, palette.accent, stroke_t),
            ),
            egui::StrokeKind::Inside,
        );

        if focus_selected {
            Self::paint_selection_glow(ui, tile_rect, 4, palette.accent, 0.95);
        }

        let favorite_size = (layout.card_width * 0.2).clamp(23.0, 28.0);
        let favorite_rect = egui::Rect::from_min_size(
            egui::pos2(
                tile_rect.right() - favorite_size - 7.0,
                tile_rect.top() + 7.0,
            ),
            egui::vec2(favorite_size, favorite_size),
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
                Color32::from_rgba_premultiplied(5, 8, 14, 202),
                palette.accent,
                0.34 + favorite_hover_t * 0.18,
            )
        } else {
            blend_color(
                Color32::from_rgba_premultiplied(5, 8, 14, 188),
                palette.panel_alt,
                favorite_hover_t * 0.32,
            )
        };
        let favorite_stroke = if rom.is_favorite {
            palette.accent
        } else {
            blend_color(palette.border, palette.text, 0.32 + favorite_hover_t * 0.24)
        };
        ui.painter()
            .rect_filled(favorite_rect, egui::CornerRadius::same(255), favorite_fill);
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
            egui::FontId::proportional((layout.title_size + 1.0).min(14.0)),
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

        if !favorite_consumed && tile_response.clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::Grid;
            self.set_active_grid_index(source, index);
            self.state.selection.set(Some(rom.rom.id.clone()));
        }
        if !favorite_consumed && tile_response.double_clicked() {
            self.state.menu_nav.focus_region = MenuFocusRegion::Grid;
            self.set_active_grid_index(source, index);
            self.state.selection.set(Some(rom.rom.id.clone()));
            self.launch_selected_rom();
        }

        needs_refresh
    }
}

fn center_crop_uv(texture_size: egui::Vec2, draw_size: egui::Vec2) -> egui::Rect {
    let full = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    if texture_size.x <= 0.0 || texture_size.y <= 0.0 || draw_size.x <= 0.0 || draw_size.y <= 0.0 {
        return full;
    }

    let texture_aspect = texture_size.x / texture_size.y;
    let draw_aspect = draw_size.x / draw_size.y;
    if texture_aspect > draw_aspect {
        let visible_width = (draw_aspect / texture_aspect).clamp(0.0, 1.0);
        let left = (1.0 - visible_width) * 0.5;
        egui::Rect::from_min_max(egui::pos2(left, 0.0), egui::pos2(left + visible_width, 1.0))
    } else {
        let visible_height = (texture_aspect / draw_aspect).clamp(0.0, 1.0);
        let top = (1.0 - visible_height) * 0.5;
        egui::Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(1.0, top + visible_height))
    }
}

fn paint_missing_art_tile(
    ui: &egui::Ui,
    tile_rect: egui::Rect,
    title: &str,
    layout: RomGridLayout,
    palette: ThemePalette,
) {
    let inner = tile_rect.shrink(12.0);
    ui.painter().rect_filled(
        tile_rect,
        egui::CornerRadius::same(4),
        Color32::from_rgba_premultiplied(4, 5, 8, 245),
    );
    ui.painter().rect_filled(
        tile_rect.shrink(1.0),
        egui::CornerRadius::same(3),
        Color32::from_rgba_premultiplied(0, 0, 0, 226),
    );

    let lines = title_lines(title, layout.title_max_chars, 3);
    let line_height = layout.title_size + 3.0;
    let total_height = line_height * lines.len() as f32;
    let first_y = inner.center().y - total_height * 0.5 + line_height * 0.5;
    for (index, line) in lines.iter().enumerate() {
        ui.painter().text(
            egui::pos2(inner.center().x, first_y + index as f32 * line_height),
            egui::Align2::CENTER_CENTER,
            line,
            egui::FontId::proportional(layout.title_size),
            palette.text,
        );
    }
}

fn title_lines(value: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let words = value.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() || max_lines == 0 {
        return vec![truncate_text(value, max_chars)];
    }

    let chars_per_line = (max_chars / max_lines.max(1)).clamp(8, 14);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let needed = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if needed > chars_per_line && !current.is_empty() {
            lines.push(current);
            current = String::new();
            if lines.len() + 1 == max_lines {
                break;
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(value.to_string());
    }
    let mut trimmed = lines.into_iter().take(max_lines).collect::<Vec<_>>();
    if let Some(last) = trimmed.last_mut() {
        *last = truncate_text(last, chars_per_line);
    }
    trimmed
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
