use crate::components::three_panel::{SplitterLabels, ThreePanelLayout};
use crate::core::events::AppEvent;
use crate::core::module::AppModule;
use crate::core::state::GlobalState;
use eframe::egui;

pub struct DashboardModule {
    pub layout: ThreePanelLayout,
}

impl DashboardModule {
    pub fn new() -> Self {
        Self {
            layout: ThreePanelLayout::new(
                0.20,
                0.20,
                SplitterLabels {
                    a_min: "控制面板(区域A) 已收缩至最小宽度极限",
                    a_max: "数据显示(区域B) 已收缩至最小宽度极限",
                    c_min: "数据输入(区域C) 已收缩至最小高度极限",
                    c_max: "数据显示(区域B) 已收缩至最小高度极限",
                },
            ),
        }
    }
}

impl AppModule for DashboardModule {
    fn id(&self) -> &str {
        "dashboard"
    }
    fn name(&self) -> &str {
        "仪表盘"
    }
    fn icon(&self) -> &str {
        "📊"
    }

    fn show_content(
        &mut self,
        ui: &mut egui::Ui,
        state: &GlobalState,
        tx: &flume::Sender<AppEvent>,
    ) {
        let theme = &state.theme;
        let heading_f32 = theme.heading_font_size as f32;
        let body_f32 = theme.body_font_size as f32;
        let spacing: f32 = theme.ui_spacing;

        let bg_c = theme.title_bg_color;
        let panel_bg = egui::Color32::from_rgba_unmultiplied(
            bg_c.r(),
            bg_c.g(),
            bg_c.b(),
            (bg_c.a() as f32 * theme.bg_opacity) as u8,
        );
        let r = theme.ui_rounding;
        let margin = spacing.max(r as f32).max(4.0);
        let radius = egui::CornerRadius::same(r);

        let total_rect = ui.available_rect_before_wrap();
        let rects = self.layout.compute(total_rect, spacing);

        let render_panel = |ui: &mut egui::Ui,
                            rect: egui::Rect,
                            title: &str,
                            content: &mut dyn FnMut(&mut egui::Ui)| {
            if !rect.is_positive() {
                return;
            }
            ui.painter().rect_filled(rect, radius, panel_bg);
            let inner_rect = rect.shrink(margin);
            if inner_rect.is_positive() {
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
                    ui.set_clip_rect(inner_rect);
                    ui.heading(
                        egui::RichText::new(title)
                            .size(heading_f32)
                            .color(theme.text_color),
                    );
                    ui.add_space(15.0);
                    egui::ScrollArea::vertical()
                        .id_salt(title)
                        .auto_shrink([false, false])
                        .show(ui, content);
                });
            }
        };

        render_panel(ui, rects.a, "控制面板 (区域 A)", &mut |ui| {
            ui.label(
                egui::RichText::new("串口选择、波特率设置、校验位等控制选项...")
                    .size(body_f32)
                    .color(egui::Color32::GRAY),
            );
        });

        self.layout.handle_a_splitter(ui, &rects, tx);

        render_panel(ui, rects.b, "数据显示 (区域 B)", &mut |ui| {
            ui.label(
                egui::RichText::new("串口接收到的主数据区域，占据最大的一块空间。")
                    .size(body_f32)
                    .color(egui::Color32::GRAY),
            );
        });

        self.layout.handle_c_splitter(ui, &rects, tx);

        render_panel(ui, rects.c, "数据输入 (区域 C)", &mut |ui| {
            ui.label(
                egui::RichText::new("用于发送串口数据，支持快捷指令和回车发送...")
                    .size(body_f32)
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(10.0);
            if ui.button("发送数据 🚀").clicked() {}
        });

        self.layout.allocate(ui, &rects);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_module() {
        let module = DashboardModule::new();
        assert_eq!(module.name(), "仪表盘");
        assert_eq!(module.icon(), "📊");
    }

    #[test]
    fn test_dashboard_default_ratios() {
        let module = DashboardModule::new();
        assert_eq!(module.layout.left_ratio, 0.20);
        assert_eq!(module.layout.bottom_ratio, 0.20);
    }

    #[test]
    fn test_dashboard_status_bar_hint() {
        let module = DashboardModule::new();
        assert_eq!(module.status_bar_hint(), "就绪");
    }

    #[test]
    fn test_dashboard_public_fields_mutability() {
        let mut module = DashboardModule::new();
        module.layout.left_ratio = 0.30;
        module.layout.bottom_ratio = 0.25;
        assert_eq!(module.layout.left_ratio, 0.30);
        assert_eq!(module.layout.bottom_ratio, 0.25);
    }
}
