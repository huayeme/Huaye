use crate::core::events::AppEvent;
use crate::core::module::AppModule;
use crate::core::state::GlobalState;
use crate::core::theme::{ThemeConfig, SHADOW_MARGIN};
use eframe::egui;
use tracing;

pub struct SettingsModule {
    theme_receiver: Option<flume::Receiver<Result<String, String>>>,
    theme_load_handle: Option<std::thread::JoinHandle<()>>,
    local_theme: ThemeConfig,
    local_drag_transparent_enabled: bool,
    needs_sync: bool,
}

impl SettingsModule {
    pub fn new() -> Self {
        Self {
            theme_receiver: None,
            theme_load_handle: None,
            local_theme: ThemeConfig::default(),
            local_drag_transparent_enabled: true,
            needs_sync: true,
        }
    }
}

impl AppModule for SettingsModule {
    fn id(&self) -> &str {
        "settings"
    }
    fn name(&self) -> &str {
        "系统设置"
    }
    fn icon(&self) -> &str {
        "⚙"
    }

    fn show_content(
        &mut self,
        ui: &mut egui::Ui,
        state: &GlobalState,
        tx: &flume::Sender<AppEvent>,
    ) {
        if self.needs_sync {
            self.local_theme = state.theme.clone();
            self.local_drag_transparent_enabled = state.drag_transparent_enabled;
            self.needs_sync = false;
        }

        if let Some(rx) = &self.theme_receiver {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(json_str) => {
                        if !json_str.is_empty() {
                            match serde_json::from_str::<ThemeConfig>(&json_str) {
                                Ok(new_theme) => {
                                    self.local_theme = new_theme.clone();
                                    let _ = tx.try_send(AppEvent::UpdateTheme(new_theme));
                                    let _ = tx.try_send(AppEvent::ToastRequest {
                                        text: "主题加载成功！".to_string(),
                                        is_error: false,
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.try_send(AppEvent::ToastRequest {
                                        text: format!("主题解析失败: {}", e),
                                        is_error: true,
                                    });
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.try_send(AppEvent::ToastRequest {
                            text: format!("文件读取失败: {}", err),
                            is_error: true,
                        });
                    }
                }
                self.theme_receiver = None;
                if let Some(handle) = self.theme_load_handle.take() {
                    let _ = handle.join();
                }
            }
        }

        let heading_f32 = self.local_theme.heading_font_size as f32;
        let body_f32 = self.local_theme.body_font_size as f32;
        let spacing = self.local_theme.ui_spacing;
        let text_color = self.local_theme.text_color;
        let card_heading_size = body_f32 * 1.2;
        let grid_col_spacing = spacing * 1.3;
        let grid_row_spacing = spacing;

        let mut theme_changed = false;
        let mut drag_changed = false;

        macro_rules! add_row {
            ($ui:expr, $label:expr, $content:expr) => {
                $ui.label(egui::RichText::new($label).size(body_f32).color(text_color));
                if $content.changed() {
                    theme_changed = true;
                }
                $ui.end_row();
            };
        }

        // 标签文字的便捷闭包
        let label = |text: &str| egui::RichText::new(text).size(body_f32).color(text_color);

        ui.scope(|ui| {
            // 面板背景色需要跟随全局透明度进行乘算
            let bg_c = self.local_theme.title_bg_color;
            let panel_bg = egui::Color32::from_rgba_unmultiplied(
                bg_c.r(),
                bg_c.g(),
                bg_c.b(),
                (bg_c.a() as f32 * self.local_theme.bg_opacity) as u8,
            );
            let rounding = egui::CornerRadius::same(self.local_theme.ui_rounding);
            let panel_margin = 20.0f32.max(self.local_theme.ui_rounding as f32);

            egui::Frame::NONE
                .fill(panel_bg)
                .corner_radius(rounding)
                .inner_margin(panel_margin)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.heading(egui::RichText::new("⚙ 系统与外观设置").size(heading_f32).color(text_color).strong());
                    ui.add_space(spacing);

                    let original_style = ui.style().clone();
                    let scroll_color = self.local_theme.scrollbar_color;

                    ui.scope(|ui| {
                        let visuals = &mut ui.style_mut().visuals;
                        for w in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
                            w.bg_fill = scroll_color;
                            w.weak_bg_fill = scroll_color;
                            w.fg_stroke.color = scroll_color;
                            w.bg_stroke = egui::Stroke::NONE;
                            w.corner_radius = rounding;
                        }

                        ui.style_mut().spacing.scroll.foreground_color = false;
                        ui.style_mut().spacing.scroll.bar_width = 10.0;

                        egui::ScrollArea::both().id_salt("settings_scroll").auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_style(original_style);

                            let card_bg = {
                                let c = self.local_theme.widget_bg_color;
                                egui::Color32::from_rgba_unmultiplied(
                                    c.r(), 
                                    c.g(), 
                                    c.b(), 
                                    (180.0 * self.local_theme.bg_opacity) as u8
                                )
                            };
                            let card_frame = egui::Frame::default()
                                .fill(card_bg)
                                .inner_margin(spacing)
                                .corner_radius(rounding);

                            let make_grid = |id: &str| {
                                egui::Grid::new(id)
                                    .num_columns(2)
                                    .spacing([grid_col_spacing, grid_row_spacing])
                                    .min_col_width(160.0)
                            };

                            ui.vertical(|ui| {
                                // ── 卡片1：常规设置（数据/主题 + 窗口行为）──
                                card_frame.show(ui, |ui| {
                                    ui.heading(egui::RichText::new("🛠 常规设置").size(card_heading_size).color(text_color).strong());
                                    ui.add_space(spacing * 0.6);
                                    ui.separator();
                                    ui.add_space(spacing * 0.6);

                                    make_grid("grid_general").show(ui, |ui| {
                                        ui.label(label("全局重置:"));
                                        if ui.button("✨ 一键恢复默认设置").clicked() {
                                            tracing::info!(target: "settings", action = "reset_to_defaults");
                                            self.local_theme = ThemeConfig::default();
                                            self.local_drag_transparent_enabled = true;
                                            theme_changed = true;
                                            drag_changed = true;
                                            let _ = tx.try_send(AppEvent::ToastRequest { text: "已恢复所有默认设置".to_string(), is_error: false });
                                        }
                                        ui.end_row();

                                        ui.label(label("主题管理:"));
                                        ui.horizontal(|ui| {
                                            if ui.button("📂 从文件加载...").clicked() && self.theme_receiver.is_none() {
                                                tracing::info!(target: "settings", action = "theme_file_dialog_opened");
                                                if let Some(path) = rfd::FileDialog::new()
                                                    .add_filter("JSON Theme", &["json"])
                                                    .set_title("选择花也主题文件")
                                                    .pick_file()
                                                {
                                                    tracing::info!(target: "settings", action = "theme_file_selected", path = %path.display());
                                                    let (tx_file, rx_file) = flume::bounded(1);
                                                    self.theme_receiver = Some(rx_file);
                                                    let ctx_clone = ui.ctx().clone();
                                                    self.theme_load_handle = Some(std::thread::spawn(move || {
                                                        const MAX_THEME_FILE_SIZE: u64 = 1024 * 1024; // 1 MB
                                                        let result = match std::fs::metadata(&path) {
                                                            Ok(meta) if meta.len() > MAX_THEME_FILE_SIZE => {
                                                                Err(format!("文件过大 ({:.1} MB)，主题文件不应超过 1 MB", meta.len() as f64 / 1024.0 / 1024.0))
                                                            }
                                                            Err(e) => Err(e.to_string()),
                                                            _ => std::fs::read_to_string(&path).map_err(|e| e.to_string()),
                                                        };
                                                        let _ = tx_file.send(result);
                                                        ctx_clone.request_repaint();
                                                    }));
                                                }
                                            }
                                            if self.theme_receiver.is_some() {
                                                ui.spinner();
                                            }
                                        });
                                        ui.end_row();

                                        ui.label(label("窗口透明度:"));
                                        let opacity_resp = ui.add(egui::Slider::new(&mut self.local_theme.bg_opacity, 0.0..=1.0));
                                        if opacity_resp.changed() { theme_changed = true; }
                                        opacity_resp.on_hover_text("透明度过低时，背景颜色相近可能会看不清");
                                        ui.end_row();

                                        ui.label(label("拖动时全透明:"));
                                        let drag_resp = ui.checkbox(&mut self.local_drag_transparent_enabled, label("开启"));
                                        if drag_resp.changed() { drag_changed = true; }
                                        drag_resp.on_hover_text("长按 Alt 键可临时全透明窗口");
                                        ui.end_row();
                                    });
                                });
                                ui.add_space(spacing);

                                // ── 卡片2：外观与排版（尺寸 + 色彩/阴影）──
                                card_frame.show(ui, |ui| {
                                    ui.heading(egui::RichText::new("🎨 外观与排版").size(card_heading_size).color(text_color).strong());
                                    ui.add_space(spacing * 0.6);
                                    ui.separator();
                                    ui.add_space(spacing * 0.6);

                                    make_grid("grid_appearance").show(ui, |ui| {
                                        add_row!(ui, "标题栏高度:", ui.add(egui::Slider::new(&mut self.local_theme.title_bar_height, 32..=60)));
                                        add_row!(ui, "圆角比例 (0~1):", ui.add(egui::Slider::new(&mut self.local_theme.corner_proportion, 0.0..=1.0)));
                                        add_row!(ui, "标题文字大小:", ui.add(egui::Slider::new(&mut self.local_theme.heading_font_size, 12..=36)));
                                        add_row!(ui, "正文文字大小:", ui.add(egui::Slider::new(&mut self.local_theme.body_font_size, 10..=24)));
                                        add_row!(ui, "控件圆角:", ui.add(egui::Slider::new(&mut self.local_theme.ui_rounding, 0..=20)));
                                        add_row!(ui, "全局间距:", ui.add(egui::Slider::new(&mut self.local_theme.ui_spacing, 0.0..=40.0)));

                                        add_row!(ui, "边框粗细:", ui.add(egui::Slider::new(&mut self.local_theme.border_thickness, 0..=5)));
                                        add_row!(ui, "边框颜色:", ui.color_edit_button_srgba(&mut self.local_theme.border_color));

                                        let max_blur = (SHADOW_MARGIN as u32).saturating_sub(self.local_theme.shadow_spread);
                                        add_row!(ui, "阴影模糊:", ui.add(egui::Slider::new(&mut self.local_theme.shadow_blur, 0..=max_blur)));

                                        let max_spread = (SHADOW_MARGIN as u32).saturating_sub(self.local_theme.shadow_blur);
                                        add_row!(ui, "阴影扩散:", ui.add(egui::Slider::new(&mut self.local_theme.shadow_spread, 0..=max_spread)));

                                        add_row!(ui, "阴影强度:", ui.add(egui::Slider::new(&mut self.local_theme.shadow_intensity, 0.0..=2.0)));
                                        add_row!(ui, "阴影底色:", ui.color_edit_button_srgba(&mut self.local_theme.shadow_color));
                                    });
                                });
                            });
                        });
                    });
                });
        });

        if theme_changed {
            tracing::info!(
                target: "settings",
                action = "theme_change_requested",
                theme_name = %self.local_theme.name,
                is_dark = self.local_theme.is_dark,
            );
            let _ = tx.try_send(AppEvent::UpdateTheme(self.local_theme.clone()));
        }
        if drag_changed {
            tracing::debug!(
                target: "settings",
                action = "drag_transparent_change_requested",
                enabled = self.local_drag_transparent_enabled,
            );
            let _ = tx.try_send(AppEvent::UpdateDragTransparent(
                self.local_drag_transparent_enabled,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_module_initialization() {
        let module = SettingsModule::new();
        assert_eq!(module.name(), "系统设置");
        assert_eq!(module.icon(), "⚙");
        assert!(module.needs_sync);
        assert_eq!(module.local_drag_transparent_enabled, true);
    }

    #[test]
    fn test_settings_local_theme_is_default() {
        let module = SettingsModule::new();
        assert_eq!(module.local_theme.name, "极简纯白");
        assert!(!module.local_theme.is_dark);
        assert_eq!(module.local_theme.title_bar_height, 50);
    }

    #[test]
    fn test_settings_theme_receiver_initially_none() {
        let module = SettingsModule::new();
        assert!(module.theme_receiver.is_none());
    }

    #[test]
    fn test_settings_status_bar_hint() {
        let module = SettingsModule::new();
        assert_eq!(module.status_bar_hint(), "就绪");
    }

    #[test]
    fn test_settings_on_exit_default() {
        let mut module = SettingsModule::new();
        module.on_exit();
    }

    #[test]
    fn test_settings_field_mutability() {
        let mut module = SettingsModule::new();
        assert!(module.needs_sync);
        module.needs_sync = false;
        assert!(!module.needs_sync);
    }

    #[test]
    fn test_settings_local_theme_mutability() {
        let mut module = SettingsModule::new();
        let original_name = module.local_theme.name.clone();
        module.local_theme.name = "自定义主题".to_string();
        assert_eq!(module.local_theme.name, "自定义主题");
        assert_ne!(module.local_theme.name, original_name);
    }

    #[test]
    fn test_settings_drag_transparent_mutability() {
        let mut module = SettingsModule::new();
        assert!(module.local_drag_transparent_enabled);
        module.local_drag_transparent_enabled = false;
        assert!(!module.local_drag_transparent_enabled);
    }
}
