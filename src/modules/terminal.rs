use crate::components::three_panel::{SplitterLabels, ThreePanelLayout};
use crate::core::data_pipeline::RingBuffer;
use crate::core::events::AppEvent;
use crate::core::module::AppModule;
use crate::core::state::GlobalState;
use eframe::egui;
use flume::{Receiver, Sender};
use serialport::SerialPort;
use std::io::Read;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayMode {
    Ascii,
    Hex,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SendMode {
    Single,
    Multi,
}

enum BackgroundCommand {
    Open {
        port: String,
        baud_rate: u32,
        data_bits: serialport::DataBits,
        parity: serialport::Parity,
        stop_bits: serialport::StopBits,
    },
    Close,
    Send(Vec<u8>),
}

enum BackgroundEvent {
    Opened(String),
    Closed,
    DataReceived(Vec<u8>),
    Error(String),
}

pub struct Terminal {
    layout: ThreePanelLayout,

    // A区 - 配置状态
    available_ports: Vec<String>,
    selected_port: String,
    baud_rate_str: String,
    data_bits: serialport::DataBits,
    parity: serialport::Parity,
    stop_bits: serialport::StopBits,

    // A区 - 发送接收控制状态
    rx_display_mode: DisplayMode,
    show_timestamp: bool,
    tx_display_mode: DisplayMode,
    add_crlf: bool,

    // B区 - 接收/发送数据缓冲 (third field: true = TX, false = RX)
    rx_buffer: RingBuffer<(chrono::DateTime<chrono::Local>, Vec<u8>, bool)>,
    rx_dirty: bool,
    cached_rx_lines: Vec<String>,
    is_auto_scroll: bool,

    // C区 - 发送区状态
    tx_text: String,
    send_mode: SendMode,

    // 后台控制 (每次 toggle_port 重建通道，避免 MPMC 竞态)
    is_open: bool,
    is_connecting: bool,
    bg_tx: Sender<BackgroundCommand>,
    bg_rx: Receiver<BackgroundEvent>,

    // 全局错误提示锁
    last_error_time: f64,
}

impl Default for Terminal {
    fn default() -> Self {
        let (bg_tx, _) = flume::bounded(100);
        let (_, bg_rx) = flume::bounded(100);

        Self {
            layout: ThreePanelLayout::new(
                0.25,
                0.20,
                SplitterLabels {
                    a_min: "配置区(区域A) 已收缩至最小宽度极限",
                    a_max: "数据展示区(区域B/C) 已收缩至最小宽度极限",
                    c_min: "发送区(区域C) 已收缩至最小高度极限",
                    c_max: "接收数据区(区域B) 已收缩至最小高度极限",
                },
            ),

            available_ports: Vec::new(),
            selected_port: String::new(),
            baud_rate_str: "115200".to_string(),
            data_bits: serialport::DataBits::Eight,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,

            rx_display_mode: DisplayMode::Ascii,
            show_timestamp: false,
            tx_display_mode: DisplayMode::Ascii,
            add_crlf: false,

            rx_buffer: RingBuffer::new(5000),
            rx_dirty: false,
            cached_rx_lines: Vec::new(),
            is_auto_scroll: true,

            tx_text: String::new(),
            send_mode: SendMode::Single,

            is_open: false,
            is_connecting: false,
            bg_tx,
            bg_rx,

            last_error_time: 0.0,
        }
    }
}

impl Terminal {
    pub fn new() -> Self {
        let mut terminal = Self::default();
        terminal.refresh_ports();
        terminal
    }

    fn refresh_ports(&mut self) {
        self.available_ports.clear();
        if let Ok(ports) = serialport::available_ports() {
            for port in ports {
                self.available_ports.push(port.port_name);
            }
        }
        if !self.available_ports.contains(&self.selected_port) {
            self.selected_port = self.available_ports.first().cloned().unwrap_or_default();
        }
    }

    fn toggle_port(&mut self, app_tx: &Sender<AppEvent>, ctx: egui::Context) {
        if self.is_open {
            self.is_open = false;
            let _ = self.bg_tx.try_send(BackgroundCommand::Close);
            let _ = app_tx.try_send(AppEvent::ToastRequest {
                text: "串口已关闭".to_string(),
                is_error: false,
            });
        } else if !self.is_connecting {
            self.is_connecting = true;
            let baud_rate = self.baud_rate_str.parse::<u32>().unwrap_or(115200);

            // 重建通道，确保新旧线程完全隔离，避免 MPMC 竞态
            let (new_bg_tx, bg_cmd_rx) = flume::bounded(100);
            let (bg_event_tx, new_bg_rx) = flume::bounded(100);
            self.bg_tx = new_bg_tx;
            self.bg_rx = new_bg_rx;

            let port_name = self.selected_port.clone();
            let data_bits = self.data_bits;
            let parity = self.parity;
            let stop_bits = self.stop_bits;

            let ctx_clone = ctx.clone();

            std::thread::spawn(move || {
                let mut active_port: Option<Box<dyn SerialPort>> = None;

                loop {
                    if let Some(p) = &mut active_port {
                        // 已打开，非阻塞地消费指令
                        while let Ok(cmd) = bg_cmd_rx.try_recv() {
                            match cmd {
                                BackgroundCommand::Close => {
                                    let _ = bg_event_tx.try_send(BackgroundEvent::Closed);
                                    ctx_clone.request_repaint();
                                    return; // 线程正常退出
                                }
                                BackgroundCommand::Send(data) => {
                                    if let Err(e) = p.write_all(&data) {
                                        let _ = bg_event_tx.try_send(BackgroundEvent::Error(
                                            format!("写入错误: {}", e),
                                        ));
                                        let _ = bg_event_tx.try_send(BackgroundEvent::Closed);
                                        ctx_clone.request_repaint();
                                        return;
                                    } else {
                                        let _ = p.flush();
                                    }
                                }
                                BackgroundCommand::Open { .. } => {
                                    tracing::warn!(target: "terminal", "收到重复的 Open 指令，串口已处于打开状态，忽略");
                                }
                            }
                        }

                        // 阻塞式读取串口数据 (依赖 timeout 设置)
                        let mut buf = [0u8; 1024];
                        match p.read(&mut buf) {
                            Ok(t) if t > 0 => {
                                let _ = bg_event_tx
                                    .try_send(BackgroundEvent::DataReceived(buf[..t].to_vec()));
                                ctx_clone.request_repaint(); // 收到数据后立即唤醒UI重绘
                            }
                            Ok(_) => {} // EOF
                            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                            Err(e) => {
                                let _ = bg_event_tx
                                    .try_send(BackgroundEvent::Error(format!("读取错误: {}", e)));
                                let _ = bg_event_tx.try_send(BackgroundEvent::Closed);
                                ctx_clone.request_repaint();
                                return; // 致命错误，退出线程
                            }
                        }
                    } else {
                        // 未打开，阻塞等待 Open 指令，避免忙等待消耗 CPU
                        match bg_cmd_rx.recv() {
                            Ok(BackgroundCommand::Open {
                                port,
                                baud_rate,
                                data_bits,
                                parity,
                                stop_bits,
                            }) => {
                                let builder = serialport::new(&port, baud_rate)
                                    .data_bits(data_bits)
                                    .parity(parity)
                                    .stop_bits(stop_bits)
                                    .timeout(Duration::from_millis(10));

                                match builder.open() {
                                    Ok(p) => {
                                        active_port = Some(p);
                                        let _ = bg_event_tx.try_send(BackgroundEvent::Opened(port));
                                        ctx_clone.request_repaint();
                                    }
                                    Err(e) => {
                                        let _ = bg_event_tx.try_send(BackgroundEvent::Error(
                                            format!("无法打开串口 {}: {}", port, e),
                                        ));
                                        ctx_clone.request_repaint();
                                        return; // 打开失败，直接退出线程等待下一次重试
                                    }
                                }
                            }
                            Ok(BackgroundCommand::Close) => return,
                            Err(_) => return, // 通道断开
                            _ => {}
                        }
                    }
                }
            });

            let _ = self.bg_tx.try_send(BackgroundCommand::Open {
                port: port_name,
                baud_rate,
                data_bits,
                parity,
                stop_bits,
            });
        }
    }

    fn handle_background_events(&mut self, app_tx: &Sender<AppEvent>) {
        while let Ok(event) = self.bg_rx.try_recv() {
            match event {
                BackgroundEvent::Opened(port) => {
                    self.is_open = true;
                    self.is_connecting = false;
                    let _ = app_tx.try_send(AppEvent::ToastRequest {
                        text: format!("串口 {} 已打开", port),
                        is_error: false,
                    });
                }
                BackgroundEvent::Closed => {
                    self.is_open = false;
                    self.is_connecting = false;
                }
                BackgroundEvent::DataReceived(data) => {
                    self.rx_buffer.push((chrono::Local::now(), data, false));
                    self.rx_dirty = true;
                }
                BackgroundEvent::Error(msg) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64();
                    if now - self.last_error_time > 2.0 {
                        let _ = app_tx.try_send(AppEvent::ToastRequest {
                            text: msg.clone(),
                            is_error: true,
                        });
                        self.last_error_time = now;
                    }
                    self.is_open = false;
                    self.is_connecting = false;
                }
            }
        }
    }

    fn rebuild_rx_cache(&mut self) {
        self.cached_rx_lines.clear();
        let (s1, s2) = self.rx_buffer.as_slices();
        let mut current_line = String::new();

        for (time, data, is_tx) in s1.iter().chain(s2.iter()) {
            // TX 数据独立成行，先 flush 未完成的 RX 行
            if *is_tx {
                if !current_line.is_empty() {
                    self.cached_rx_lines.push(current_line.clone());
                    current_line.clear();
                }
                let mut tx_line = String::new();
                if self.show_timestamp {
                    tx_line.push_str(&format!("[{}] ", time.format("%H:%M:%S.%3f")));
                }
                tx_line.push_str(">>> ");
                match self.rx_display_mode {
                    DisplayMode::Ascii => {
                        let text = String::from_utf8_lossy(data).to_string();
                        let trimmed = text.trim_end_matches(&['\r', '\n'][..]);
                        tx_line.push_str(trimmed);
                    }
                    DisplayMode::Hex => {
                        for byte in data {
                            tx_line.push_str(&format!("{:02X} ", byte));
                        }
                    }
                }
                self.cached_rx_lines.push(tx_line);
                continue;
            }

            match self.rx_display_mode {
                DisplayMode::Ascii => {
                    let formatted_data = String::from_utf8_lossy(data).to_string();
                    for chunk in formatted_data.split_inclusive('\n') {
                        if self.show_timestamp && current_line.is_empty() {
                            current_line.push_str(&format!("[{}] ", time.format("%H:%M:%S.%3f")));
                        }
                        current_line.push_str(chunk);

                        if chunk.ends_with('\n') {
                            if current_line.ends_with('\n') {
                                current_line.pop();
                            }
                            if current_line.ends_with('\r') {
                                current_line.pop();
                            }
                            self.cached_rx_lines.push(current_line.clone());
                            current_line.clear();
                        }
                    }
                }
                DisplayMode::Hex => {
                    if self.show_timestamp {
                        current_line.push_str(&format!("[{}] ", time.format("%H:%M:%S.%3f")));
                    }
                    for byte in data {
                        current_line.push_str(&format!("{:02X} ", byte));
                    }
                    self.cached_rx_lines.push(current_line.clone());
                    current_line.clear();
                }
            }
        }
        if !current_line.is_empty() {
            self.cached_rx_lines.push(current_line);
        }
    }

    fn send_data(&mut self, app_tx: &Sender<AppEvent>) {
        if !self.is_open || self.tx_text.is_empty() {
            return;
        }

        let mut data_to_send = Vec::new();

        match self.tx_display_mode {
            DisplayMode::Ascii => {
                data_to_send.extend_from_slice(self.tx_text.as_bytes());
            }
            DisplayMode::Hex => {
                let clean_hex: String = self
                    .tx_text
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if !clean_hex.is_ascii() {
                    let _ = app_tx.try_send(AppEvent::ToastRequest {
                        text: "包含无效的HEX字符".to_string(),
                        is_error: true,
                    });
                    return;
                }
                if !clean_hex.len().is_multiple_of(2) {
                    let _ = app_tx.try_send(AppEvent::ToastRequest {
                        text: "HEX模式下字符数必须为偶数".to_string(),
                        is_error: true,
                    });
                    return;
                }
                for i in (0..clean_hex.len()).step_by(2) {
                    if let Ok(byte) = u8::from_str_radix(&clean_hex[i..i + 2], 16) {
                        data_to_send.push(byte);
                    } else {
                        let _ = app_tx.try_send(AppEvent::ToastRequest {
                            text: "包含无效的HEX字符".to_string(),
                            is_error: true,
                        });
                        return;
                    }
                }
            }
        }

        if self.add_crlf {
            data_to_send.push(b'\r');
            data_to_send.push(b'\n');
        }

        self.rx_buffer
            .push((chrono::Local::now(), data_to_send.clone(), true));
        self.rx_dirty = true;
        let _ = self.bg_tx.try_send(BackgroundCommand::Send(data_to_send));
        self.tx_text.clear();
    }
}

impl AppModule for Terminal {
    fn id(&self) -> &str {
        "terminal"
    }

    fn name(&self) -> &str {
        "串口终端"
    }

    fn icon(&self) -> &str {
        "🔌"
    }

    fn status_bar_hint(&self) -> &str {
        if self.is_connecting {
            "正在连接..."
        } else if self.is_open {
            "正在通信"
        } else {
            "串口关闭"
        }
    }

    fn on_exit(&mut self) {
        if self.is_open {
            let _ = self.bg_tx.try_send(BackgroundCommand::Close);
        }
    }

    fn show_content(&mut self, ui: &mut egui::Ui, state: &GlobalState, tx: &Sender<AppEvent>) {
        self.handle_background_events(tx);

        if self.rx_dirty {
            self.rebuild_rx_cache();
            self.rx_dirty = false;
        }

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
                            with_scroll: bool,
                            content: &mut dyn FnMut(&mut egui::Ui)| {
            if !rect.is_positive() {
                return;
            }
            ui.painter().rect_filled(rect, radius, panel_bg);
            let inner_rect = rect.shrink(margin);
            if inner_rect.is_positive() {
                // 将剪裁区域向外扩展3个像素，专门留给边框(Stroke)渲染，防止最外圈边框被截断
                let clip_rect = inner_rect.expand(3.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
                    ui.set_clip_rect(clip_rect);
                    ui.heading(
                        egui::RichText::new(title)
                            .size(heading_f32)
                            .color(theme.text_color),
                    );
                    ui.add_space(15.0);
                    if with_scroll {
                        egui::ScrollArea::vertical()
                            .id_salt(title)
                            .auto_shrink([false, false])
                            .show(ui, content);
                    } else {
                        content(ui);
                    }
                });
            }
        };

        // 渲染 A 区域 (左侧配置栏)
        render_panel(ui, rects.a, "⚙ 串口与设置", true, &mut |ui| {
            egui::Grid::new("serial_config_grid")
                .num_columns(2)
                .spacing([8.0, 12.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("串口号:")
                            .color(theme.text_color)
                            .size(body_f32),
                    );
                    ui.horizontal(|ui| {
                        let btn_width = 28.0;
                        egui::ComboBox::from_id_salt("port_select")
                            .width(ui.available_width() - btn_width)
                            .selected_text(&self.selected_port)
                            .show_ui(ui, |ui| {
                                for port in &self.available_ports {
                                    ui.selectable_value(
                                        &mut self.selected_port,
                                        port.clone(),
                                        port,
                                    );
                                }
                            });
                        if ui.button("🔄").on_hover_text("刷新").clicked() {
                            self.refresh_ports();
                        }
                    });
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("波特率:")
                            .color(theme.text_color)
                            .size(body_f32),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.baud_rate_str)
                            .desired_width(ui.available_width()),
                    );
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("数据位:")
                            .color(theme.text_color)
                            .size(body_f32),
                    );
                    egui::ComboBox::from_id_salt("data_bits")
                        .width(ui.available_width())
                        .selected_text(match self.data_bits {
                            serialport::DataBits::Five => "5",
                            serialport::DataBits::Six => "6",
                            serialport::DataBits::Seven => "7",
                            serialport::DataBits::Eight => "8",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.data_bits,
                                serialport::DataBits::Eight,
                                "8",
                            );
                            ui.selectable_value(
                                &mut self.data_bits,
                                serialport::DataBits::Seven,
                                "7",
                            );
                            ui.selectable_value(
                                &mut self.data_bits,
                                serialport::DataBits::Six,
                                "6",
                            );
                            ui.selectable_value(
                                &mut self.data_bits,
                                serialport::DataBits::Five,
                                "5",
                            );
                        });
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("校验位:")
                            .color(theme.text_color)
                            .size(body_f32),
                    );
                    egui::ComboBox::from_id_salt("parity")
                        .width(ui.available_width())
                        .selected_text(match self.parity {
                            serialport::Parity::None => "None",
                            serialport::Parity::Even => "Even",
                            serialport::Parity::Odd => "Odd",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.parity, serialport::Parity::None, "None");
                            ui.selectable_value(&mut self.parity, serialport::Parity::Even, "Even");
                            ui.selectable_value(&mut self.parity, serialport::Parity::Odd, "Odd");
                        });
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("停止位:")
                            .color(theme.text_color)
                            .size(body_f32),
                    );
                    egui::ComboBox::from_id_salt("stop_bits")
                        .width(ui.available_width())
                        .selected_text(match self.stop_bits {
                            serialport::StopBits::One => "1",
                            serialport::StopBits::Two => "2",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.stop_bits,
                                serialport::StopBits::One,
                                "1",
                            );
                            ui.selectable_value(
                                &mut self.stop_bits,
                                serialport::StopBits::Two,
                                "2",
                            );
                        });
                    ui.end_row();
                });

            ui.add_space(16.0);

            let btn_text = if self.is_connecting {
                "正在连接..."
            } else if self.is_open {
                "关闭串口"
            } else {
                "打开串口"
            };
            let btn_color = if self.is_open {
                egui::Color32::from_rgb(200, 80, 80)
            } else {
                theme.scrollbar_color
            };

            ui.add_sized(
                [ui.available_width(), 36.0],
                egui::Button::new(
                    egui::RichText::new(btn_text)
                        .color(if self.is_open {
                            egui::Color32::WHITE
                        } else {
                            theme.text_color
                        })
                        .size(16.0),
                )
                .fill(btn_color)
                .corner_radius(theme.ui_rounding),
            )
            .clicked()
            .then(|| self.toggle_port(tx, ui.ctx().clone()));

            ui.add_space(24.0);
            ui.separator();
            ui.add_space(16.0);

            ui.heading(
                egui::RichText::new("📥 接收设置")
                    .color(theme.text_color)
                    .size(body_f32 * 1.2),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .radio_value(&mut self.rx_display_mode, DisplayMode::Ascii, "ASCII")
                    .changed()
                {
                    self.rx_dirty = true;
                }
                if ui
                    .radio_value(&mut self.rx_display_mode, DisplayMode::Hex, "HEX")
                    .changed()
                {
                    self.rx_dirty = true;
                }
            });
            if ui
                .checkbox(&mut self.show_timestamp, "显示时间戳")
                .changed()
            {
                self.rx_dirty = true;
            }
            ui.checkbox(&mut self.is_auto_scroll, "自动滚动到最新");

            ui.add_space(24.0);
            ui.separator();
            ui.add_space(16.0);

            ui.heading(
                egui::RichText::new("📤 发送设置")
                    .color(theme.text_color)
                    .size(body_f32 * 1.2),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut self.tx_display_mode, DisplayMode::Ascii, "ASCII");
                ui.radio_value(&mut self.tx_display_mode, DisplayMode::Hex, "HEX");
            });
            ui.checkbox(&mut self.add_crlf, "追加回车换行 (CRLF)");
        });

        self.layout.handle_a_splitter(ui, &rects, tx);

        // 渲染 B 区域 (接收区) - 内联展开以使用 show_rows
        if rects.b.is_positive() {
            ui.painter().rect_filled(rects.b, radius, panel_bg);

            // --- 新增：光标交互逻辑 ---
            // 捕获区域 B 的交互，Sense::hover() 即可检测鼠标悬停
            let rx_resp = ui.interact(rects.b, ui.id().with("rx_area_interact"), egui::Sense::click_and_drag());
            if rx_resp.hovered() || rx_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }

            let inner = rects.b.shrink(margin);
            if inner.is_positive() {
                let clip_rect = inner.expand(3.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    ui.set_clip_rect(clip_rect);
                    ui.horizontal(|ui| {
                        ui.heading(
                            egui::RichText::new("📋 接收数据区")
                                .size(heading_f32)
                                .color(theme.text_color),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;

                            let draw_btn =
                                |ui: &mut egui::Ui, text: &str, base: egui::Color32| -> bool {
                                    let mut result = false;
                                    ui.scope(|ui| {
                                        let hover_c = egui::Color32::from_rgba_unmultiplied(
                                            base.r().saturating_add(30),
                                            base.g().saturating_add(30),
                                            base.b().saturating_add(30),
                                            255,
                                        );
                                        let active_c = egui::Color32::from_rgba_unmultiplied(
                                            base.r().saturating_sub(20),
                                            base.g().saturating_sub(20),
                                            base.b().saturating_sub(20),
                                            255,
                                        );
                                        let v = &mut ui.style_mut().visuals;
                                        v.widgets.inactive.weak_bg_fill = base;
                                        v.widgets.inactive.bg_fill = base;
                                        v.widgets.hovered.weak_bg_fill = hover_c;
                                        v.widgets.hovered.bg_fill = hover_c;
                                        v.widgets.active.weak_bg_fill = active_c;
                                        v.widgets.active.bg_fill = active_c;
                                        v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                                        v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
                                        v.widgets.active.bg_stroke = egui::Stroke::NONE;

                                        let btn = egui::Button::new(
                                            egui::RichText::new(text)
                                                .color(theme.text_color)
                                                .size(body_f32 * 1.1),
                                        )
                                        .corner_radius(theme.ui_rounding)
                                        .min_size(egui::vec2(0.0, 28.0));

                                        if ui.add(btn).clicked() {
                                            result = true;
                                        }
                                    });
                                    result
                                };

                            if draw_btn(ui, "清空显示", theme.btn_close_bg) {
                                self.rx_buffer.clear();
                                self.rx_dirty = true;
                            }
                            if draw_btn(ui, "数据筛选", theme.btn_ai_bg) {
                                // TODO: 数据筛选逻辑
                            }
                            if draw_btn(ui, "文字高亮", theme.btn_min_bg) {
                                // TODO: 文字高亮逻辑
                            }
                        });
                    });
                    ui.add_space(15.0);

                    // 为接收数据区增加一个底板，使其和发送区输入框的视觉效果保持一致
                    let available_for_text = ui.available_size();
                    let text_bg_rect =
                        egui::Rect::from_min_size(ui.cursor().min, available_for_text);
                    ui.painter().rect_filled(
                        text_bg_rect,
                        theme.ui_rounding,
                        theme.widget_bg_color,
                    );

                    let inner_text_bg = text_bg_rect.shrink(theme.ui_rounding as f32);
                    if inner_text_bg.is_positive() {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(inner_text_bg), |ui| {
                            let mut scroll =
                                egui::ScrollArea::vertical().auto_shrink([false, false]);
                            if self.is_auto_scroll {
                                scroll = scroll.stick_to_bottom(true);
                            }

                            // O(1) 终极渲染优化：只渲染屏幕可见的数据行
                            scroll.show_rows(
                                ui,
                                ui.text_style_height(&egui::TextStyle::Monospace),
                                self.cached_rx_lines.len(),
                                |ui, row_range| {
                                    for row in row_range {
                                        ui.label(
                                            egui::RichText::new(&self.cached_rx_lines[row])
                                                .color(theme.text_color)
                                                .family(egui::FontFamily::Monospace),
                                        );
                                    }
                                },
                            );
                        });
                    }
                    ui.allocate_rect(text_bg_rect, egui::Sense::hover());
                });
            }
        }

        self.layout.handle_c_splitter(ui, &rects, tx);

        // 渲染 C 区域 (发送区) — 内联展开以支持标题栏控制组件
        if rects.c.is_positive() {
            ui.painter().rect_filled(rects.c, radius, panel_bg);
            let inner = rects.c.shrink(margin);
            if inner.is_positive() {
                let clip_rect = inner.expand(3.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    ui.set_clip_rect(clip_rect);

                    // 标题行：左侧标题 + 右侧发送模式切换
                    ui.horizontal(|ui| {
                        ui.heading(
                            egui::RichText::new("⌨ 数据发送区")
                                .size(heading_f32)
                                .color(theme.text_color),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let mode_btn = |ui: &mut egui::Ui,
                                            label: &str,
                                            target: SendMode,
                                            current: SendMode,
                                            base: egui::Color32|
                             -> bool {
                                let is_active = current == target;
                                let fill = if is_active {
                                    base
                                } else {
                                    egui::Color32::from_rgba_unmultiplied(
                                        base.r(),
                                        base.g(),
                                        base.b(),
                                        80,
                                    )
                                };
                                let hover_c = egui::Color32::from_rgba_unmultiplied(
                                    base.r().saturating_add(30),
                                    base.g().saturating_add(30),
                                    base.b().saturating_add(30),
                                    255,
                                );
                                let mut clicked = false;
                                ui.scope(|ui| {
                                    let v = &mut ui.style_mut().visuals;
                                    v.widgets.inactive.weak_bg_fill = fill;
                                    v.widgets.inactive.bg_fill = fill;
                                    v.widgets.hovered.weak_bg_fill = hover_c;
                                    v.widgets.hovered.bg_fill = hover_c;
                                    v.widgets.active.weak_bg_fill = base;
                                    v.widgets.active.bg_fill = base;
                                    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                                    v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
                                    v.widgets.active.bg_stroke = egui::Stroke::NONE;

                                    let btn = egui::Button::new(
                                        egui::RichText::new(label)
                                            .color(theme.text_color)
                                            .size(body_f32),
                                    )
                                    .corner_radius(theme.ui_rounding)
                                    .min_size(egui::vec2(0.0, 24.0));
                                    if ui.add(btn).clicked() {
                                        clicked = true;
                                    }
                                });
                                clicked
                            };

                            // 从右往左排列
                            if mode_btn(
                                ui,
                                "多条发送",
                                SendMode::Multi,
                                self.send_mode,
                                theme.btn_set_bg,
                            ) {
                                self.send_mode = SendMode::Multi;
                            }
                            if mode_btn(
                                ui,
                                "单条发送",
                                SendMode::Single,
                                self.send_mode,
                                theme.btn_ai_bg,
                            ) {
                                self.send_mode = SendMode::Single;
                            }
                        });
                    });
                    ui.add_space(8.0);

                    // 输入区 + 发送按钮
                    let available = ui.available_size();
                    let btn_width = 80.0;
                    let btn_height = available.y;
                    let text_width = (available.x - btn_width - spacing).max(0.0);

                    let start_pos = ui.cursor().min;
                    let text_rect =
                        egui::Rect::from_min_size(start_pos, egui::vec2(text_width, available.y));
                    let send_btn_rect = egui::Rect::from_min_size(
                        egui::pos2(start_pos.x + text_width + spacing, start_pos.y),
                        egui::vec2(btn_width, btn_height),
                    );

                    ui.painter()
                        .rect_filled(text_rect, theme.ui_rounding, theme.widget_bg_color);
                    let inner_text_rect = text_rect.shrink(theme.ui_rounding as f32);
                    if inner_text_rect.is_positive() {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(inner_text_rect), |ui| {
                            // 仅在内容区较矮（约 2 行以内高度）时垂直居中，
                            // 用户拖高发送区后按正常顶部对齐，符合输入直觉
                            let line_h = ui.text_style_height(&egui::TextStyle::Monospace);
                            let num_lines = self.tx_text.split('\n').count().max(1);
                            let compact = inner_text_rect.height() <= line_h * 3.0;

                            let content_h = num_lines as f32 * line_h;
                            let top_pad = if compact {
                                ((inner_text_rect.height() - content_h) / 2.0).max(0.0)
                            } else {
                                0.0
                            };
                            let bottom_pad =
                                (inner_text_rect.height() - content_h - top_pad).max(0.0);

                            let text_edit = egui::TextEdit::multiline(&mut self.tx_text)
                                .frame(false)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin {
                                    left: 0,
                                    right: 0,
                                    top: top_pad as i8,
                                    bottom: bottom_pad as i8,
                                });

                            // 使用 add_sized 铺满整个区域，结合动态 margin 实现完美的居中。
                            // 这种做法让 TextEdit 原生接管整个空白区域的点击，
                            // 完美解决只闪烁但不进入输入状态（无法调出 IME、光标不会定位到末尾）的问题。
                            let resp = ui.add_sized(ui.available_size(), text_edit);

                            if resp.has_focus()
                                && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter))
                            {
                                self.send_data(tx);
                            }
                        });
                    }

                    if send_btn_rect.is_positive() {
                        ui.scope_builder(egui::UiBuilder::new().max_rect(send_btn_rect), |ui| {
                            let btn = ui.add_sized(
                                send_btn_rect.size(),
                                egui::Button::new(
                                    egui::RichText::new("发 送")
                                        .color(theme.text_color)
                                        .size(16.0),
                                )
                                .fill(theme.scrollbar_color)
                                .corner_radius(theme.ui_rounding),
                            );
                            if btn.clicked() {
                                self.send_data(tx);
                            }
                        });
                    }

                    ui.allocate_rect(
                        egui::Rect::from_min_size(start_pos, available),
                        egui::Sense::hover(),
                    );
                });
            }
        }

        self.layout.allocate(ui, &rects);
    }
}
