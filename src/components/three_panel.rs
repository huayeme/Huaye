use crate::core::events::AppEvent;
use eframe::egui;

pub struct SplitterLabels {
    pub a_min: &'static str,
    pub a_max: &'static str,
    pub c_min: &'static str,
    pub c_max: &'static str,
}

pub struct ThreePanelLayout {
    pub left_ratio: f32,
    pub bottom_ratio: f32,
    a_min_toast_shown: bool,
    a_max_toast_shown: bool,
    c_min_toast_shown: bool,
    c_max_toast_shown: bool,
    labels: SplitterLabels,
}

pub struct PanelRects {
    pub a: egui::Rect,
    pub b: egui::Rect,
    pub c: egui::Rect,
    pub total: egui::Rect,
    bc: egui::Rect,
    safe_spacing: f32,
    max_a_width: f32,
    min_a: f32,
    a_max_allowed: f32,
    a_width: f32,
    max_c_height: f32,
    min_c: f32,
    c_max_allowed: f32,
    c_height: f32,
}

impl ThreePanelLayout {
    pub fn new(left_ratio: f32, bottom_ratio: f32, labels: SplitterLabels) -> Self {
        Self {
            left_ratio,
            bottom_ratio,
            a_min_toast_shown: false,
            a_max_toast_shown: false,
            c_min_toast_shown: false,
            c_max_toast_shown: false,
            labels,
        }
    }

    pub fn compute(&self, total_rect: egui::Rect, spacing: f32) -> PanelRects {
        let safe_spacing = spacing
            .min(total_rect.width() / 3.0)
            .min(total_rect.height() / 3.0);

        let max_a_width = (total_rect.width() - safe_spacing).max(0.0);
        let mut a_width = max_a_width * self.left_ratio;
        let min_a = max_a_width * 0.2;
        let min_bc = max_a_width * 0.3;
        let a_max_allowed = (max_a_width - min_bc).max(min_a);
        a_width = a_width.clamp(min_a, a_max_allowed);

        let max_c_height = (total_rect.height() - safe_spacing).max(0.0);
        let mut c_height = max_c_height * self.bottom_ratio;
        let min_c = max_c_height * 0.2;
        let min_b = max_c_height * 0.3;
        let c_max_allowed = (max_c_height - min_b).max(min_c);
        c_height = c_height.clamp(min_c, c_max_allowed);

        if total_rect.height() < c_height + safe_spacing + min_b {
            c_height = (total_rect.height() - safe_spacing - min_b).max(min_c);
        }
        let b_height = (total_rect.height() - c_height - safe_spacing).max(0.0);

        let a = egui::Rect::from_min_size(
            total_rect.min,
            egui::vec2(a_width, total_rect.height()),
        );
        let bc = egui::Rect::from_min_size(
            egui::pos2(total_rect.min.x + a_width + safe_spacing, total_rect.min.y),
            egui::vec2(
                (total_rect.width() - a_width - safe_spacing).max(0.0),
                total_rect.height(),
            ),
        );
        let b = egui::Rect::from_min_size(bc.min, egui::vec2(bc.width(), b_height));
        let c = egui::Rect::from_min_size(
            egui::pos2(bc.min.x, b.max.y + safe_spacing),
            egui::vec2(bc.width(), (bc.height() - b_height - safe_spacing).max(0.0)),
        );

        PanelRects {
            a, b, c, total: total_rect,
            bc, safe_spacing, max_a_width, min_a, a_max_allowed, a_width,
            max_c_height, min_c, c_max_allowed, c_height,
        }
    }

    pub fn handle_a_splitter(
        &mut self,
        ui: &mut egui::Ui,
        rects: &PanelRects,
        tx: &flume::Sender<AppEvent>,
    ) {
        let splitter_rect = egui::Rect::from_min_size(
            egui::pos2(rects.a.max.x, rects.total.min.y),
            egui::vec2(rects.safe_spacing, rects.total.height()),
        );
        let resp = ui.interact(
            splitter_rect,
            ui.id().with("a_splitter"),
            egui::Sense::drag(),
        );
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.dragged() {
            let delta = resp.drag_delta().x;
            if rects.max_a_width > 0.0 {
                let target_width = rects.a_width + delta;

                if target_width <= rects.min_a + 1.0 && delta < 0.0 {
                    if !self.a_min_toast_shown {
                        let _ = tx.try_send(AppEvent::ToastRequest {
                            text: self.labels.a_min.to_string(),
                            is_error: false,
                        });
                        self.a_min_toast_shown = true;
                    }
                } else if target_width >= rects.a_max_allowed - 1.0 && delta > 0.0 {
                    if !self.a_max_toast_shown {
                        let _ = tx.try_send(AppEvent::ToastRequest {
                            text: self.labels.a_max.to_string(),
                            is_error: false,
                        });
                        self.a_max_toast_shown = true;
                    }
                } else {
                    if target_width > rects.min_a + 5.0 {
                        self.a_min_toast_shown = false;
                    }
                    if target_width < rects.a_max_allowed - 5.0 {
                        self.a_max_toast_shown = false;
                    }
                }

                let clamped_width = target_width.clamp(rects.min_a, rects.a_max_allowed);
                self.left_ratio = clamped_width / rects.max_a_width;
            }
        }
    }

    pub fn handle_c_splitter(
        &mut self,
        ui: &mut egui::Ui,
        rects: &PanelRects,
        tx: &flume::Sender<AppEvent>,
    ) {
        let splitter_rect = egui::Rect::from_min_size(
            egui::pos2(rects.bc.min.x, rects.b.max.y),
            egui::vec2(rects.bc.width(), rects.safe_spacing),
        );
        let resp = ui.interact(
            splitter_rect,
            ui.id().with("c_splitter"),
            egui::Sense::drag(),
        );
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.dragged() {
            let delta = resp.drag_delta().y;
            if rects.max_c_height > 0.0 {
                let target_height = rects.c_height - delta;

                if target_height <= rects.min_c + 1.0 && delta > 0.0 {
                    if !self.c_min_toast_shown {
                        let _ = tx.try_send(AppEvent::ToastRequest {
                            text: self.labels.c_min.to_string(),
                            is_error: false,
                        });
                        self.c_min_toast_shown = true;
                    }
                } else if target_height >= rects.c_max_allowed - 1.0 && delta < 0.0 {
                    if !self.c_max_toast_shown {
                        let _ = tx.try_send(AppEvent::ToastRequest {
                            text: self.labels.c_max.to_string(),
                            is_error: false,
                        });
                        self.c_max_toast_shown = true;
                    }
                } else {
                    if target_height > rects.min_c + 5.0 {
                        self.c_min_toast_shown = false;
                    }
                    if target_height < rects.c_max_allowed - 5.0 {
                        self.c_max_toast_shown = false;
                    }
                }

                let clamped_height = target_height.clamp(rects.min_c, rects.c_max_allowed);
                self.bottom_ratio = clamped_height / rects.max_c_height;
            }
        }
    }

    pub fn allocate(&self, ui: &mut egui::Ui, rects: &PanelRects) {
        ui.allocate_rect(rects.total, egui::Sense::hover());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_panel_layout_new() {
        let layout = ThreePanelLayout::new(
            0.25, 0.20,
            SplitterLabels { a_min: "a", a_max: "b", c_min: "c", c_max: "d" },
        );
        assert_eq!(layout.left_ratio, 0.25);
        assert_eq!(layout.bottom_ratio, 0.20);
        assert!(!layout.a_min_toast_shown);
    }

    #[test]
    fn test_compute_rects_positive() {
        let layout = ThreePanelLayout::new(
            0.25, 0.20,
            SplitterLabels { a_min: "", a_max: "", c_min: "", c_max: "" },
        );
        let total = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let rects = layout.compute(total, 10.0);
        assert!(rects.a.is_positive());
        assert!(rects.b.is_positive());
        assert!(rects.c.is_positive());
    }

    #[test]
    fn test_compute_rects_ratios_clamped() {
        let layout = ThreePanelLayout::new(
            0.05, 0.05,
            SplitterLabels { a_min: "", a_max: "", c_min: "", c_max: "" },
        );
        let total = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let rects = layout.compute(total, 10.0);
        // min_a = 0.2 * max_a_width, so a_width should be clamped up
        assert!(rects.a.width() >= (total.width() - 10.0) * 0.2 - 1.0);
    }
}
