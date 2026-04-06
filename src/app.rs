use crate::components::decorations::*;
use crate::core::events::AppEvent;
use crate::core::module::AppModule;
use crate::core::state::GlobalState;
use crate::core::theme::*;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use sysinfo::{Pid, System};

pub struct ToastMessage {
    pub text: String,
    pub is_error: bool,
    pub expire_time: Option<f64>,
}

pub struct MyApp {
    modules: Vec<Box<dyn AppModule>>,
    current_index: usize,

    event_tx: flume::Sender<AppEvent>,
    event_rx: flume::Receiver<AppEvent>,
    state: GlobalState,

    cpu_usage: f32,
    mem_usage: u64,

    init_frames: u8,
    startup_frame_count: u8,
    last_theme: ThemeConfig,
    is_dragging_window: bool,
    config_dirty: bool,
    config_dirty_since: Option<std::time::Instant>,
    config_saving: Arc<AtomicBool>,
    sysinfo_handle: Option<std::thread::JoinHandle<()>>,
    toasts: Vec<ToastMessage>,
    window_min_x_toast_shown: bool,
    window_min_y_toast_shown: bool,
    ignore_min_size_toast_frames: u8,
    sysinfo_stop: Arc<AtomicBool>,
}

impl MyApp {
    pub fn new(event_rx: flume::Receiver<AppEvent>) -> Self {
        let modules = crate::modules::build_app_modules();

        let config = crate::core::config::AppConfig::load();

        let state = GlobalState {
            theme: config.theme.clone(),
            drag_transparent_enabled: config.drag_transparent_enabled,
            ..Default::default()
        };

        // 获取已初始化的全局事件发送器 (在 logger::init() 中设置)
        let event_tx = crate::core::events::GLOBAL_EVENT_TX
            .get()
            .expect("GLOBAL_EVENT_TX not initialized")
            .clone();

        let sysinfo_stop = Arc::new(AtomicBool::new(false));

        Self {
            modules,
            current_index: 0,

            event_tx,
            event_rx,

            last_theme: ThemeConfig {
                name: String::new(),
                ..ThemeConfig::default()
            },
            state,

            cpu_usage: 0.0,
            mem_usage: 0,

            init_frames: 0,
            startup_frame_count: 0,
            is_dragging_window: false,
            config_dirty: false,
            config_dirty_since: None,
            config_saving: Arc::new(AtomicBool::new(false)),
            sysinfo_handle: None, // 延迟到首次 update() 启动，需要 egui::Context 触发 repaint
            toasts: Vec::new(),
            window_min_x_toast_shown: false,
            window_min_y_toast_shown: false,
            ignore_min_size_toast_frames: 10,
            sysinfo_stop,
        }
    }

    pub fn add_toast(
        toasts: &mut Vec<ToastMessage>,
        text: String,
        is_error: bool,
        current_time: f64,
    ) {
        // 去重：相同文本的 toast 刷新过期时间而不是重复添加
        if let Some(existing) = toasts.iter_mut().find(|t| t.text == text) {
            existing.expire_time = Some(current_time + 3.0);
            return;
        }
        // 上限 5 条，满了移除最早的
        if toasts.len() >= 5 {
            toasts.remove(0);
        }
        toasts.push(ToastMessage {
            text,
            is_error,
            expire_time: Some(current_time + 3.0),
        });
    }

    pub fn show_toast(&mut self, text: String, is_error: bool, current_time: f64) {
        Self::add_toast(&mut self.toasts, text, is_error, current_time);
    }

    fn handle_event(&mut self, event: AppEvent, current_time: f64) {
        match event {
            AppEvent::LogMessage(msg) => {
                tracing::debug!("收到异步消息: {}", msg);
            }
            AppEvent::UpdateTheme(new_theme) => {
                let old_name = self.state.theme.name.clone();
                self.state.theme = new_theme;
                self.mark_config_dirty();
                tracing::info!(
                    target: "config",
                    action = "theme_changed",
                    old_theme = %old_name,
                    new_theme = %self.state.theme.name,
                    bg_opacity = self.state.theme.bg_opacity,
                    corner_radius = self.state.theme.ui_rounding,
                );
            }
            AppEvent::UpdateDragTransparent(enabled) => {
                self.state.drag_transparent_enabled = enabled;
                self.mark_config_dirty();
                tracing::debug!(action = "drag_transparent_changed", enabled = enabled,);
            }
            AppEvent::ToastRequest { text, is_error } => {
                tracing::debug!(
                    action = "toast_request",
                    is_error = is_error,
                    message = %text,
                );
                self.show_toast(text, is_error, current_time);
            }
            AppEvent::FatalError(msg) => {
                tracing::error!(
                    action = "fatal_error",
                    message = %msg,
                );
                self.show_toast(format!("致命错误: {}", msg), true, current_time);
            }
            AppEvent::SysInfoUpdate {
                cpu_usage,
                mem_usage,
            } => {
                self.cpu_usage = cpu_usage;
                self.mem_usage = mem_usage;
            }
            AppEvent::DataReady => {}
        }
    }

    fn mark_config_dirty(&mut self) {
        self.config_dirty = true;
        if self.config_dirty_since.is_none() {
            self.config_dirty_since = Some(std::time::Instant::now());
        }
    }

    fn flush_config_if_ready(&mut self) {
        if !self.config_dirty {
            return;
        }
        if self.config_saving.load(Ordering::Relaxed) {
            return;
        }
        if let Some(since) = self.config_dirty_since {
            if since.elapsed() >= std::time::Duration::from_millis(500) {
                let config = crate::core::config::AppConfig {
                    theme: self.state.theme.clone(),
                    drag_transparent_enabled: self.state.drag_transparent_enabled,
                };
                let tx = self.event_tx.clone();
                let saving = self.config_saving.clone();
                saving.store(true, Ordering::Relaxed);
                std::thread::spawn(move || {
                    if let Err(msg) = config.save() {
                        let _ = tx.try_send(AppEvent::ToastRequest {
                            text: msg,
                            is_error: true,
                        });
                    }
                    saving.store(false, Ordering::Relaxed);
                });
                self.config_dirty = false;
                self.config_dirty_since = None;
            }
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl eframe::App for MyApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 延迟启动 sysinfo 后台线程（首次 update 时拿到 ctx 用于触发 repaint）
        if self.sysinfo_handle.is_none() {
            let stop_flag = self.sysinfo_stop.clone();
            let tx_clone = self.event_tx.clone();
            let ctx_clone = ctx.clone();
            self.sysinfo_handle = Some(std::thread::spawn(move || {
                let pid = Pid::from_u32(std::process::id());
                let mut sys = System::new();
                sys.refresh_cpu_all();
                let num_cpus = sys.cpus().len().max(1) as f32;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
                    let (cpu_usage, mem_usage) = sys
                        .process(pid)
                        .map(|p| (p.cpu_usage() / num_cpus, p.memory()))
                        .unwrap_or((0.0, 0));
                    // try_send: 通道满时丢弃本次数据而非阻塞，系统监控数据丢一帧无害
                    if tx_clone
                        .try_send(AppEvent::SysInfoUpdate {
                            cpu_usage,
                            mem_usage,
                        })
                        .is_err()
                        && tx_clone.is_disconnected()
                    {
                        break;
                    }
                    ctx_clone.request_repaint();
                }
            }));
        }

        let current_time = ctx.input(|i| i.time);

        // 1. 处理通知消息过期
        self.toasts.retain(|t| {
            if let Some(expire) = t.expire_time {
                current_time < expire
            } else {
                true // 还没开始计时的保留
            }
        });

        // 2. 消费所有后台事件 (基于时间的时间片节流)
        let loop_start = std::time::Instant::now();
        let time_limit = std::time::Duration::from_millis(6); // 保证 120Hz 下有余量给 UI 渲染

        while let Ok(event) = self.event_rx.try_recv() {
            self.handle_event(event, current_time);
            if loop_start.elapsed() > time_limit {
                // 时间片耗尽，保证下一帧继续处理未处理完的事件
                ctx.request_repaint();
                break;
            }
        }

        // 如果消费完毕且由于没有时间耗尽被打断，不需要强制 request_repaint。
        // 因为 eframe 会自动等待新的事件或输入，避免 CPU 满载。

        // Debounce 配置保存：停止操作 500ms 后才真正写盘
        self.flush_config_if_ready();

        let theme = &self.state.theme;
        let is_peeking = (self.state.drag_transparent_enabled && self.is_dragging_window)
            || ctx.input(|i| i.modifiers.alt);
        let peek_factor =
            ctx.animate_bool_with_time(egui::Id::new("peek_anim"), is_peeking, ANIM_DURATION);

        let current_render_opacity = lerp(theme.bg_opacity, 0.0, peek_factor);
        let ui_alpha_factor = 1.0 - peek_factor;

        if &self.last_theme != theme {
            let mut visuals = if theme.is_dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            visuals.panel_fill = egui::Color32::TRANSPARENT;

            let theme_rounding = egui::CornerRadius::same(theme.ui_rounding);
            visuals.widgets.noninteractive.corner_radius = theme_rounding;
            visuals.widgets.inactive.corner_radius = theme_rounding;
            visuals.widgets.hovered.corner_radius = theme_rounding;
            visuals.widgets.active.corner_radius = theme_rounding;
            visuals.widgets.open.corner_radius = theme_rounding;

            let widget_bg = theme.widget_bg_color;
            visuals.extreme_bg_color = widget_bg;
            visuals.widgets.inactive.bg_fill = widget_bg;
            // Removed the global bg_fill overrides for hovered/active as it breaks general button styles

            ctx.set_visuals(visuals);

            // Global scrollbar fix for egui 0.33, ensuring handles don't fall back to foreground text color
            let mut style = (*ctx.style()).clone();
            style.spacing.scroll.foreground_color = false;
            ctx.set_style(style);

            self.last_theme = theme.clone();
        }

        let bg_c = theme.bg_color;
        let bg_color = egui::Color32::from_rgba_unmultiplied(
            bg_c.r(),
            bg_c.g(),
            bg_c.b(),
            (current_render_opacity * 255.0) as u8,
        );

        let title_bg_c = theme.title_bg_color;
        let title_bg_color = egui::Color32::from_rgba_unmultiplied(
            title_bg_c.r(),
            title_bg_c.g(),
            title_bg_c.b(),
            (current_render_opacity * 255.0) as u8,
        );

        let fade_color = |c: egui::Color32| -> egui::Color32 {
            egui::Color32::from_rgba_unmultiplied(
                c.r(),
                c.g(),
                c.b(),
                (c.a() as f32 * ui_alpha_factor) as u8,
            )
        };
        let text_color = fade_color(theme.text_color);

        let current_shadow_color = theme.shadow_color;
        let base_shadow_a = current_shadow_color.a() as f32;
        let normal_shadow_alpha = base_shadow_a * theme.shadow_intensity * theme.bg_opacity;
        let target_peek_shadow_alpha = if theme.shadow_intensity > 0.0 {
            (base_shadow_a * theme.shadow_intensity * PEEK_SHADOW_MULTIPLIER)
                .max(PEEK_SHADOW_MIN_ALPHA)
        } else {
            0.0
        };
        let final_shadow_alpha = lerp(normal_shadow_alpha, target_peek_shadow_alpha, peek_factor)
            .clamp(0.0, 255.0) as u8;

        let final_shadow_color = egui::Color32::from_rgba_unmultiplied(
            current_shadow_color.r(),
            current_shadow_color.g(),
            current_shadow_color.b(),
            final_shadow_alpha,
        );

        let current_border_color = theme.border_color;
        let base_border_a = current_border_color.a() as f32;
        let normal_border_alpha =
            base_border_a + (255.0 - base_border_a) * (1.0 - theme.bg_opacity);
        let target_peek_border_alpha = 255.0;
        let current_border_alpha = lerp(normal_border_alpha, target_peek_border_alpha, peek_factor)
            .clamp(0.0, 255.0) as u8;

        let final_border_color = egui::Color32::from_rgba_unmultiplied(
            current_border_color.r(),
            current_border_color.g(),
            current_border_color.b(),
            current_border_alpha,
        );

        if self.init_frames < 2 {
            self.startup_frame_count = self.startup_frame_count.saturating_add(1);
            // 获取显示器尺寸，若 5 帧后仍无法获取（例如某些 Linux 桌面环境异常），则降级使用默认大小强制显示
            let monitor_size = ctx.input(|i| i.viewport().monitor_size);

            if let Some(size) = monitor_size {
                if self.init_frames == 0 {
                    let visual_target = size / 2.0;
                    let os_target =
                        visual_target + egui::vec2(SHADOW_MARGIN * 2.0, SHADOW_MARGIN * 2.0);
                    let center_pos = (size - os_target) / 2.0;

                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(os_target));
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                        center_pos.to_pos2(),
                    ));

                    self.init_frames = 1;
                    ctx.request_repaint();
                } else if self.init_frames == 1 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    self.init_frames = 2;
                    ctx.request_repaint();
                }
            } else if self.startup_frame_count > 5 {
                // 防御性降级：放弃等待，直接显示
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                self.init_frames = 2;
                ctx.request_repaint();
            } else {
                ctx.request_repaint(); // 继续重试
            }

            // 在前两帧等待 OS 响应尺寸和位置变化，不进行 UI 渲染，避免窗口内容闪烁
            return;
        }

        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        // 我们完全摒弃了原生窗口配置，所有的渲染都在自定义无边框模式下进行
        let is_fullscreen_mode = is_maximized;
        let current_margin = if is_fullscreen_mode {
            0.0
        } else {
            SHADOW_MARGIN
        };

        let screen_rect = ctx.content_rect();
        let absolute_window_rect = screen_rect.shrink(current_margin);
        let visual_size = absolute_window_rect.size();

        // 窗口尺寸达到最小极限时的 Toast 提示
        if self.ignore_min_size_toast_frames > 0 {
            self.ignore_min_size_toast_frames -= 1;
        } else {
            let hit_x = visual_size.x <= MIN_VISUAL_WIDTH + 1.0;
            let hit_y = visual_size.y <= MIN_VISUAL_HEIGHT + 1.0;

            let mut just_hit_x = false;
            let mut just_hit_y = false;

            if hit_x && !self.window_min_x_toast_shown {
                self.window_min_x_toast_shown = true;
                just_hit_x = true;
            } else if visual_size.x > MIN_VISUAL_WIDTH + 5.0 {
                self.window_min_x_toast_shown = false;
            }

            if hit_y && !self.window_min_y_toast_shown {
                self.window_min_y_toast_shown = true;
                just_hit_y = true;
            } else if visual_size.y > MIN_VISUAL_HEIGHT + 5.0 {
                self.window_min_y_toast_shown = false;
            }

            let min_size_text = if just_hit_x && just_hit_y {
                Some("主窗口已缩放至最小极限！")
            } else if just_hit_x {
                Some("主窗口横向已缩放至最小极限！")
            } else if just_hit_y {
                Some("主窗口竖向已缩放至最小极限！")
            } else {
                None
            };
            if let Some(text) = min_size_text {
                Self::add_toast(&mut self.toasts, text.to_string(), false, current_time);
            }
        }

        let title_h_f32 = theme.title_bar_height as f32;
        let max_allowed_radius = title_h_f32 / 2.0;
        let actual_corner_radius = if is_fullscreen_mode {
            0.0
        } else {
            max_allowed_radius * theme.corner_proportion
        };
        let actual_corner_radius_u8 = actual_corner_radius.round() as u8;

        let safe_spread = theme.shadow_spread.min(SHADOW_MARGIN as u32);
        let safe_blur = theme
            .shadow_blur
            .min((SHADOW_MARGIN as u32).saturating_sub(safe_spread));

        let normal_border_w = theme.border_thickness as f32;
        let peek_border_w = normal_border_w.max(PEEK_BORDER_MIN_WIDTH);
        let active_border_w = if is_fullscreen_mode {
            0.0
        } else {
            lerp(normal_border_w, peek_border_w, peek_factor)
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let painter = ui.painter();

                if !is_fullscreen_mode
                    && (safe_blur > 0 || safe_spread > 0)
                    && theme.shadow_intensity > 0.0
                {
                    let shadow = eframe::epaint::Shadow {
                        offset: [0, 0],
                        blur: safe_blur.min(255) as u8,
                        spread: safe_spread.min(255) as u8,
                        color: final_shadow_color,
                    };
                    painter.add(shadow.as_shape(
                        absolute_window_rect,
                        egui::CornerRadius::same(actual_corner_radius_u8),
                    ));
                }

                painter.rect(
                    absolute_window_rect,
                    actual_corner_radius_u8,
                    bg_color,
                    egui::Stroke::new(active_border_w, final_border_color),
                    egui::StrokeKind::Inside,
                );

                let title_bar_rect = egui::Rect::from_min_size(
                    absolute_window_rect.min,
                    egui::vec2(absolute_window_rect.width(), title_h_f32),
                );
                let mut is_hovering_buttons = false;
                #[allow(unused_assignments)]
                let mut title_interact = ui.interact(
                    egui::Rect::NOTHING,
                    ui.id().with("dummy_title_drag"),
                    egui::Sense::click_and_drag(),
                );

                // 自定义标题栏渲染
                {
                    let inner_radius =
                        (actual_corner_radius - active_border_w).max(0.0).round() as u8;
                    let title_rounding = egui::CornerRadius {
                        nw: inner_radius,
                        ne: inner_radius,
                        sw: 0,
                        se: 0,
                    };
                    let inner_title_rect = title_bar_rect.shrink(active_border_w);
                    painter.rect_filled(inner_title_rect, title_rounding, title_bg_color);

                    if active_border_w > 0.0 {
                        let line_y = title_bar_rect.max.y - (active_border_w / 2.0);
                        let separator_color = fade_color(final_border_color);
                        if separator_color.a() > 0 {
                            painter.line_segment(
                                [
                                    egui::pos2(
                                        absolute_window_rect.min.x + active_border_w,
                                        line_y,
                                    ),
                                    egui::pos2(
                                        absolute_window_rect.max.x - active_border_w,
                                        line_y,
                                    ),
                                ],
                                egui::Stroke::new(active_border_w, separator_color),
                            );
                        }
                    }

                    title_interact = ui.interact(
                        title_bar_rect,
                        ui.id().with("title_drag"),
                        egui::Sense::click_and_drag(),
                    );

                    if title_interact.drag_stopped() {
                        self.is_dragging_window = false;
                    }

                    let center_y = title_bar_rect.center().y;
                    let base_btn_radius = title_h_f32 * BTN_RADIUS_RATIO;

                    if ui_alpha_factor > UI_FADE_THRESHOLD {
                        let spacing_px = title_h_f32 * TITLE_SPACING_RATIO;
                        let logo_outer_r = title_h_f32 * LOGO_OUTER_RATIO;
                        let logo_center =
                            egui::pos2(title_bar_rect.min.x + spacing_px + logo_outer_r, center_y);

                        let logo_color = fade_color(theme.logo_color);
                        draw_flower_logo(painter, logo_center, title_h_f32, logo_color);

                        let dynamic_title_font_size =
                            (title_h_f32 * LOGO_OUTER_RATIO * TITLE_FONT_RATIO).round();
                        let text_start_x = logo_center.x + logo_outer_r + spacing_px;

                        painter.text(
                            egui::pos2(text_start_x, center_y),
                            egui::Align2::LEFT_CENTER,
                            "花也",
                            egui::FontId::proportional(dynamic_title_font_size),
                            text_color,
                        );

                        let btn_spacing = title_h_f32 * BTN_SPACING_RATIO;
                        let close_x = title_bar_rect.max.x - (title_h_f32 / 2.0);
                        let max_x = close_x - (base_btn_radius * 2.0 + btn_spacing);
                        let min_x = max_x - (base_btn_radius * 2.0 + btn_spacing);
                        let settings_x = min_x - (base_btn_radius * 2.0 + btn_spacing);
                        let ai_x = settings_x - (base_btn_radius * 2.0 + btn_spacing);

                        let buttons = [
                            (ai_x, theme.btn_ai_bg, theme.btn_ai_icon, WindowButton::AI),
                            (
                                settings_x,
                                theme.btn_set_bg,
                                theme.btn_set_icon,
                                WindowButton::Settings,
                            ),
                            (
                                min_x,
                                theme.btn_min_bg,
                                theme.btn_min_icon,
                                WindowButton::Minimize,
                            ),
                            (
                                max_x,
                                theme.btn_max_bg,
                                theme.btn_max_icon,
                                WindowButton::Maximize,
                            ),
                            (
                                close_x,
                                theme.btn_close_bg,
                                theme.btn_close_icon,
                                WindowButton::Close,
                            ),
                        ];

                        for (btn_x, bg_col, icon_col, btn_type) in buttons.iter() {
                            let btn_center = egui::pos2(*btn_x, center_y);

                            let mut is_truly_hovered = false;
                            if let Some(mouse_pos) = ctx.pointer_latest_pos() {
                                if mouse_pos.distance(btn_center) <= base_btn_radius {
                                    is_truly_hovered = true;
                                }
                            }

                            let hover_scale = ctx.animate_bool_with_time(
                                ui.id().with("btn_anim").with(btn_type),
                                is_truly_hovered,
                                0.2,
                            );
                            let current_btn_radius =
                                base_btn_radius * (1.0 + BTN_HOVER_SCALE * hover_scale);

                            let btn_rect = egui::Rect::from_center_size(
                                btn_center,
                                egui::vec2(current_btn_radius * 2.0, current_btn_radius * 2.0),
                            );
                            let response =
                                ui.interact(btn_rect, ui.id().with(btn_type), egui::Sense::click());

                            if is_truly_hovered {
                                is_hovering_buttons = true;
                            }

                            let faded_bg = fade_color(*bg_col);
                            let faded_icon = fade_color(*icon_col);
                            let faded_stroke =
                                fade_color(egui::Color32::from_black_alpha(BTN_STROKE_ALPHA));

                            painter.circle_filled(btn_center, current_btn_radius, faded_bg);
                            painter.circle_stroke(
                                btn_center,
                                current_btn_radius,
                                egui::Stroke::new(BTN_STROKE_WIDTH, faded_stroke),
                            );

                            if is_truly_hovered {
                                draw_window_control_icon(
                                    painter,
                                    btn_center,
                                    current_btn_radius,
                                    *btn_type,
                                    faded_icon,
                                    is_maximized,
                                );
                            }

                            if response.clicked() && is_truly_hovered {
                                match btn_type {
                                    WindowButton::Close => {
                                        tracing::info!(target: "window", action = "close_requested");
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Close)
                                    }
                                    WindowButton::Minimize => {
                                        tracing::debug!(target: "window", action = "minimize_requested");
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                                    }
                                    WindowButton::Maximize => {
                                        tracing::debug!(
                                            target: "window",
                                            action = "maximize_toggled",
                                            currently_maximized = is_maximized,
                                        );
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                                    }
                                    WindowButton::Settings => {
                                        let old_module = self
                                            .modules
                                            .get(self.current_index)
                                            .map(|m| m.name())
                                            .unwrap_or("unknown");
                                        let settings_index = self.modules.iter().position(|m| m.id() == "settings");
                                        if let Some(si) = settings_index {
                                            if self.current_index == si {
                                                self.current_index = 0;
                                            } else {
                                                self.current_index = si;
                                            }
                                        }
                                        let new_module = self
                                            .modules
                                            .get(self.current_index)
                                            .map(|m| m.name())
                                            .unwrap_or("unknown");
                                        tracing::info!(
                                            target: "navigation",
                                            action = "module_switched",
                                            from = %old_module,
                                            to = %new_module,
                                        );
                                    }
                                    WindowButton::AI => {
                                        let _ = self.event_tx.try_send(AppEvent::ToastRequest {
                                            text: "AI 助手模块还在开发中哦！✨".to_string(),
                                            is_error: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                } // 结束自定义标题栏渲染

                let is_in_ghost_corner = |pos: egui::Pos2| -> bool {
                    if is_fullscreen_mode {
                        return false;
                    }
                    if !absolute_window_rect.contains(pos) {
                        return true;
                    }
                    let r = actual_corner_radius;
                    let corners = [
                        egui::pos2(
                            absolute_window_rect.min.x + r,
                            absolute_window_rect.min.y + r,
                        ),
                        egui::pos2(
                            absolute_window_rect.max.x - r,
                            absolute_window_rect.min.y + r,
                        ),
                        egui::pos2(
                            absolute_window_rect.min.x + r,
                            absolute_window_rect.max.y - r,
                        ),
                        egui::pos2(
                            absolute_window_rect.max.x - r,
                            absolute_window_rect.max.y - r,
                        ),
                    ];
                    if pos.x < corners[0].x && pos.y < corners[0].y {
                        return pos.distance(corners[0]) > r;
                    }
                    if pos.x > corners[1].x && pos.y < corners[1].y {
                        return pos.distance(corners[1]) > r;
                    }
                    if pos.x < corners[2].x && pos.y > corners[2].y {
                        return pos.distance(corners[2]) > r;
                    }
                    if pos.x > corners[3].x && pos.y > corners[3].y {
                        return pos.distance(corners[3]) > r;
                    }
                    false
                };

                let edge_w = RESIZE_EDGE_WIDTH;
                let is_hovering_resize_edge = if let Some(pointer_pos) = ctx.pointer_latest_pos() {
                    !is_fullscreen_mode
                        && (pointer_pos.x < absolute_window_rect.min.x + edge_w
                            || pointer_pos.x > absolute_window_rect.max.x - edge_w
                            || pointer_pos.y < absolute_window_rect.min.y + edge_w
                            || pointer_pos.y > absolute_window_rect.max.y - edge_w)
                } else {
                    false
                };

                if title_interact.double_clicked() && !is_hovering_buttons {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                }

                if title_interact.drag_started() && !is_hovering_buttons && !is_hovering_resize_edge
                {
                    self.is_dragging_window = true;
                }

                if title_interact.dragged() && !is_hovering_buttons && !is_hovering_resize_edge {
                    if let Some(pointer_pos) = ctx.pointer_interact_pos() {
                        if !is_in_ghost_corner(pointer_pos) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                    }
                }

                let base_btn_radius_for_corner = title_h_f32 * BTN_RADIUS_RATIO;
                if !is_fullscreen_mode {
                    let mut corner_w = RESIZE_CORNER_SIZE;
                    let safe_corner_w = (title_h_f32 / 2.0) - base_btn_radius_for_corner - 2.0;
                    if corner_w > safe_corner_w {
                        corner_w = safe_corner_w.max(edge_w);
                    }

                    let rect = absolute_window_rect;
                    let resize_zones = [
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.min.x + corner_w, rect.min.y),
                                egui::pos2(rect.max.x - corner_w, rect.min.y + edge_w),
                            ),
                            egui::ResizeDirection::North,
                            egui::CursorIcon::ResizeVertical,
                        ),
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.min.x + corner_w, rect.max.y - edge_w),
                                egui::pos2(rect.max.x - corner_w, rect.max.y),
                            ),
                            egui::ResizeDirection::South,
                            egui::CursorIcon::ResizeVertical,
                        ),
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, rect.min.y + corner_w),
                                egui::pos2(rect.min.x + edge_w, rect.max.y - corner_w),
                            ),
                            egui::ResizeDirection::West,
                            egui::CursorIcon::ResizeHorizontal,
                        ),
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.max.x - edge_w, rect.min.y + corner_w),
                                egui::pos2(rect.max.x, rect.max.y - corner_w),
                            ),
                            egui::ResizeDirection::East,
                            egui::CursorIcon::ResizeHorizontal,
                        ),
                        (
                            egui::Rect::from_min_max(
                                rect.min,
                                egui::pos2(rect.min.x + corner_w, rect.min.y + corner_w),
                            ),
                            egui::ResizeDirection::NorthWest,
                            egui::CursorIcon::ResizeNwSe,
                        ),
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.max.x - corner_w, rect.min.y),
                                egui::pos2(rect.max.x, rect.min.y + corner_w),
                            ),
                            egui::ResizeDirection::NorthEast,
                            egui::CursorIcon::ResizeNeSw,
                        ),
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, rect.max.y - corner_w),
                                egui::pos2(rect.min.x + corner_w, rect.max.y),
                            ),
                            egui::ResizeDirection::SouthWest,
                            egui::CursorIcon::ResizeNeSw,
                        ),
                        (
                            egui::Rect::from_min_max(
                                egui::pos2(rect.max.x - corner_w, rect.max.y - corner_w),
                                rect.max,
                            ),
                            egui::ResizeDirection::SouthEast,
                            egui::CursorIcon::ResizeNwSe,
                        ),
                    ];

                    for (i, (zone, dir, cursor)) in resize_zones.into_iter().enumerate() {
                        let response = ui.interact(
                            zone,
                            ui.id().with("resize_edge").with(i),
                            egui::Sense::drag(),
                        );
                        if response.hovered() || response.dragged() {
                            if let Some(pointer_pos) = ctx.pointer_latest_pos() {
                                if !is_in_ghost_corner(pointer_pos) {
                                    if response.hovered() {
                                        ctx.set_cursor_icon(cursor);
                                    }
                                    if response.dragged() {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                                            dir,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                let content_rect = egui::Rect::from_min_max(
                    egui::pos2(absolute_window_rect.min.x, title_bar_rect.max.y),
                    absolute_window_rect.max,
                );

                // 状态栏和标题栏高度一致，取消原本的 30% 限高，确保视觉统一
                let status_margin = theme.ui_spacing;
                let status_bar_height = title_h_f32;
                let safe_status_area_height = status_bar_height + status_margin;
                let actual_status_bar_height = status_bar_height;

                let module_area_rect = egui::Rect::from_min_max(
                    content_rect.min,
                    egui::pos2(
                        content_rect.max.x,
                        (content_rect.max.y - safe_status_area_height).max(content_rect.min.y),
                    ),
                );

                let mut safe_content_rect = module_area_rect.shrink(theme.ui_spacing);
                if !safe_content_rect.is_positive() {
                    safe_content_rect = egui::Rect::from_center_size(
                        module_area_rect.center(),
                        egui::vec2(0.0, 0.0),
                    );
                }

                if safe_content_rect.is_positive() {
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(safe_content_rect),
                        |sandbox_ui| {
                            sandbox_ui.set_clip_rect(safe_content_rect);
                            sandbox_ui.set_opacity(ui_alpha_factor);

                            // 🟩 Layer 1: 业务内容层
                            if let Some(module) = self.modules.get_mut(self.current_index) {
                                module.show_content(sandbox_ui, &self.state, &self.event_tx);
                            } else {
                                sandbox_ui.centered_and_justified(|ui| {
                                    ui.label(
                                        egui::RichText::new("没有任何业务模块可用")
                                            .size(24.0)
                                            .color(text_color),
                                    );
                                });
                            }
                        },
                    );
                }

                // 底部状态监控栏绘制
                let status_bar_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        content_rect.min.x + status_margin,
                        content_rect.max.y - safe_status_area_height,
                    ),
                    egui::pos2(
                        content_rect.max.x - status_margin,
                        content_rect.max.y - safe_status_area_height + actual_status_bar_height,
                    ),
                );

                if status_bar_rect.is_positive() {
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(status_bar_rect),
                        |status_ui| {
                            // 绘制长圆形组件背景
                            let status_bg_color = title_bg_color; // 状态栏使用处理过透明度的标题栏背景色
                            status_ui.painter().rect_filled(
                                status_bar_rect,
                                egui::CornerRadius::same(theme.ui_rounding),
                                status_bg_color,
                            );

                            status_ui.scope_builder(
                                egui::UiBuilder::new().max_rect(status_bar_rect.shrink(8.0)),
                                |ui| {
                                    ui.horizontal_centered(|ui| {
                                        // 左侧：显示就绪提示
                                        let hint = if let Some(module) =
                                            self.modules.get(self.current_index)
                                        {
                                            module.status_bar_hint()
                                        } else {
                                            "就绪"
                                        };
                                        ui.label(
                                            egui::RichText::new(hint).color(text_color).size(14.0),
                                        );

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                // 右侧：资源监控
                                                let mem_mb =
                                                    self.mem_usage as f32 / 1024.0 / 1024.0;
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "💾 Mem: {:.1} MB",
                                                        mem_mb
                                                    ))
                                                    .color(text_color)
                                                    .size(13.0),
                                                );
                                                ui.add_space(8.0);

                                                // 如果 CPU 使用率较高，变成警告色
                                                let cpu_color = if self.cpu_usage > 80.0 {
                                                    fade_color(egui::Color32::from_rgb(255, 100, 100))
                                                } else {
                                                    text_color
                                                };
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "⚡ CPU: {:.1}%",
                                                        self.cpu_usage
                                                    ))
                                                    .color(cpu_color)
                                                    .size(13.0),
                                                );
                                                ui.add_space(12.0);

                                                // 分隔符
                                                ui.label(egui::RichText::new("|").color(text_color).size(13.0));
                                                ui.add_space(12.0);

                                                // 导航按钮
                                                let nav_btn = |ui: &mut egui::Ui, label: &str, is_active: bool, base: egui::Color32| -> bool {
                                                    let color = if is_active { text_color } else { fade_color(egui::Color32::GRAY) };
                                                    let fill = if is_active { fade_color(base) } else { egui::Color32::TRANSPARENT };
                                                    let hover_fill = fade_color(egui::Color32::from_rgba_unmultiplied(
                                                        base.r().saturating_add(30),
                                                        base.g().saturating_add(30),
                                                        base.b().saturating_add(30),
                                                        255,
                                                    ));
                                                    let mut text = egui::RichText::new(label).color(color).size(13.0);
                                                    if is_active { text = text.strong(); }
                                                    let mut clicked = false;
                                                    ui.scope(|ui| {
                                                        let v = &mut ui.style_mut().visuals;
                                                        v.widgets.inactive.weak_bg_fill = fill;
                                                        v.widgets.inactive.bg_fill = fill;
                                                        v.widgets.hovered.weak_bg_fill = hover_fill;
                                                        v.widgets.hovered.bg_fill = hover_fill;
                                                        v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                                                        v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
                                                        v.widgets.active.bg_stroke = egui::Stroke::NONE;
                                                        let btn = egui::Button::new(text)
                                                            .corner_radius(theme.ui_rounding)
                                                            .min_size(egui::vec2(0.0, 24.0));
                                                        if ui.add(btn).clicked() {
                                                            clicked = true;
                                                        }
                                                    });
                                                    clicked
                                                };

                                                if nav_btn(ui, "🏠 主页", self.current_index == 0, theme.btn_max_bg) {
                                                    self.current_index = 0;
                                                }
                                                ui.add_space(4.0);

                                                let is_term = self.modules.get(self.current_index).map(|m| m.id()) == Some("terminal");
                                                if nav_btn(ui, "🔌 串口助手", is_term, theme.btn_ai_bg) {
                                                    if let Some(index) = self.modules.iter().position(|m| m.id() == "terminal") {
                                                        self.current_index = index;
                                                    }
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                        },
                    );
                }

                // 🟥 Layer 4: 全局强打断层 (Toast)
                if !self.toasts.is_empty() {
                    let fade_duration = 0.5;

                    // 容器背景用所有 toast 中最小的 alpha（最快过期的那条）
                    let min_remaining = self
                        .toasts
                        .iter()
                        .filter_map(|t| t.expire_time.map(|e| e - current_time))
                        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .unwrap_or(f64::MAX);

                    let container_alpha = if min_remaining < fade_duration {
                        (min_remaining / fade_duration).max(0.0) as f32
                    } else {
                        1.0
                    };

                    let base_alpha = (240.0 * container_alpha) as u8;
                    let toast_bg = egui::Color32::from_rgba_unmultiplied(
                        theme.title_bg_color.r(),
                        theme.title_bg_color.g(),
                        theme.title_bg_color.b(),
                        base_alpha,
                    );
                    let stroke_alpha = (theme.border_color.a() as f32 * container_alpha) as u8;
                    let toast_stroke = egui::Stroke::new(
                        theme.border_thickness as f32,
                        egui::Color32::from_rgba_unmultiplied(
                            theme.border_color.r(),
                            theme.border_color.g(),
                            theme.border_color.b(),
                            stroke_alpha,
                        ),
                    );

                    let toast_max_width = screen_rect.width() * 0.8;

                    egui::Area::new(egui::Id::new("global_toasts"))
                        .order(egui::Order::Tooltip)
                        .anchor(egui::Align2::CENTER_TOP, [0.0, title_h_f32 + 20.0])
                        .show(ctx, |ui| {
                            ui.set_max_width(toast_max_width);
                            egui::Frame::NONE
                                .inner_margin(egui::Margin::symmetric(20, 12))
                                .corner_radius(theme.ui_rounding)
                                .fill(toast_bg)
                                .shadow(eframe::epaint::Shadow {
                                    offset: [0, 4],
                                    blur: theme.shadow_blur as u8,
                                    spread: 0,
                                    color: egui::Color32::from_rgba_unmultiplied(
                                        final_shadow_color.r(),
                                        final_shadow_color.g(),
                                        final_shadow_color.b(),
                                        (final_shadow_color.a() as f32 * container_alpha) as u8,
                                    ),
                                })
                                .stroke(toast_stroke)
                                .show(ui, |ui| {
                                    for toast in &self.toasts {
                                        // 每条 toast 独立计算淡出 alpha
                                        let remaining = toast.expire_time
                                            .map(|e| e - current_time)
                                            .unwrap_or(f64::MAX);
                                        let item_alpha = if remaining < fade_duration {
                                            (remaining / fade_duration).max(0.0) as f32
                                        } else {
                                            1.0
                                        };

                                        let base_color = if toast.is_error {
                                            egui::Color32::from_rgb(255, 100, 100)
                                        } else {
                                            theme.text_color
                                        };
                                        let color = egui::Color32::from_rgba_unmultiplied(
                                            base_color.r(),
                                            base_color.g(),
                                            base_color.b(),
                                            (base_color.a() as f32 * item_alpha) as u8,
                                        );
                                        let icon = if toast.is_error { "❌" } else { "✅" };
                                        ui.label(
                                            egui::RichText::new(format!("{} {}", icon, toast.text))
                                                .color(color)
                                                .size(theme.body_font_size as f32 + 2.0),
                                        );
                                    }
                                });
                        });

                    // 淡出期间需要持续 repaint
                    if min_remaining > 0.0 && min_remaining <= fade_duration {
                        ctx.request_repaint();
                    } else if min_remaining > fade_duration {
                        ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                            min_remaining - fade_duration,
                        ));
                    } else {
                        ctx.request_repaint();
                    }
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.sysinfo_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.sysinfo_handle.take() {
            let _ = handle.join();
        }

        // 等待进行中的异步保存完成，避免与下面的同步写产生文件写冲突
        let start_wait = std::time::Instant::now();
        while self.config_saving.load(Ordering::Relaxed) {
            if start_wait.elapsed() > std::time::Duration::from_secs(2) {
                tracing::warn!("等待配置保存超时，强行退出");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 退出前强制写入未保存的配置（退出时同步写，确保不丢失）
        if self.config_dirty {
            let config = crate::core::config::AppConfig {
                theme: self.state.theme.clone(),
                drag_transparent_enabled: self.state.drag_transparent_enabled,
            };
            if let Err(msg) = config.save() {
                tracing::error!(action = "config_save_on_exit_failed", message = %msg);
            }
        }
        for module in &mut self.modules {
            module.on_exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lerp_basic() {
        assert_eq!(lerp(0.0, 100.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 100.0, 1.0), 100.0);
        assert_eq!(lerp(0.0, 100.0, 0.5), 50.0);
    }

    #[test]
    fn test_lerp_with_negative() {
        assert_eq!(lerp(-50.0, 50.0, 0.5), 0.0);
        assert_eq!(lerp(-100.0, 100.0, 0.25), -50.0);
    }

    #[test]
    fn test_lerp_extrapolation() {
        assert_eq!(lerp(0.0, 100.0, -0.5), -50.0);
        assert_eq!(lerp(0.0, 100.0, 1.5), 150.0);
    }

    #[test]
    fn test_lerp_identity() {
        assert_eq!(lerp(42.0, 42.0, 0.5), 42.0);
        assert_eq!(lerp(42.0, 42.0, 0.0), 42.0);
        assert_eq!(lerp(42.0, 42.0, 1.0), 42.0);
    }

    #[test]
    fn test_toast_message_creation() {
        let toast = ToastMessage {
            text: "Test message".to_string(),
            is_error: false,
            expire_time: Some(100.0),
        };
        assert_eq!(toast.text, "Test message");
        assert!(!toast.is_error);
        assert_eq!(toast.expire_time, Some(100.0));
    }

    #[test]
    fn test_toast_message_error_variant() {
        let toast = ToastMessage {
            text: "Error occurred".to_string(),
            is_error: true,
            expire_time: Some(200.0),
        };
        assert!(toast.is_error);
    }

    #[test]
    fn test_toast_message_no_expiry() {
        let toast = ToastMessage {
            text: "Permanent toast".to_string(),
            is_error: false,
            expire_time: None,
        };
        assert!(toast.expire_time.is_none());
    }

    #[test]
    fn test_toast_message_equality() {
        let toast1 = ToastMessage {
            text: "Test".to_string(),
            is_error: false,
            expire_time: Some(100.0),
        };
        let toast2 = ToastMessage {
            text: "Test".to_string(),
            is_error: false,
            expire_time: Some(100.0),
        };
        assert!(toast1.text == toast2.text);
        assert!(toast1.is_error == toast2.is_error);
        assert!(toast1.expire_time == toast2.expire_time);
    }
}
