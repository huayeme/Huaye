use crate::core::theme::*;
use eframe::egui;
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowButton {
    Close,
    Maximize,
    Minimize,
    Settings,
    AI,
}

pub fn draw_flower_logo(
    painter: &egui::Painter,
    center: egui::Pos2,
    title_h: f32,
    color: egui::Color32,
) {
    let logo_outer_r = title_h * LOGO_OUTER_RATIO;
    let logo_inner_r = logo_outer_r * LOGO_INNER_RATIO;
    let logo_stroke_w = (title_h * LOGO_STROKE_RATIO).max(1.5);
    let logo_stroke = egui::Stroke::new(logo_stroke_w, color);

    painter.circle_stroke(center, logo_outer_r, logo_stroke);

    let draw_arc = |start_deg: f32, end_deg: f32| {
        let n_points = 30;
        let mut points = [egui::Pos2::ZERO; 31];
        for (i, item) in points.iter_mut().enumerate().take(n_points + 1) {
            let t = i as f32 / n_points as f32;
            let deg = start_deg + (end_deg - start_deg) * t;
            let rad = (deg - 90.0) * PI / 180.0;
            *item = center + egui::vec2(rad.cos() * logo_inner_r, rad.sin() * logo_inner_r);
        }

        painter.circle_filled(points[0], logo_stroke_w / 2.0, color);
        painter.circle_filled(points[n_points], logo_stroke_w / 2.0, color);
        painter.add(egui::Shape::line(points.to_vec(), logo_stroke));
    };

    draw_arc(35.0, 55.0);
    draw_arc(100.0, 350.0);
}

pub fn draw_window_control_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    btn_type: WindowButton,
    color: egui::Color32,
    is_maximized: bool,
) {
    let line_w = radius * BTN_ICON_RATIO;
    let draw_rounded_line = |p1: egui::Pos2, p2: egui::Pos2, width: f32| {
        painter.line_segment([p1, p2], egui::Stroke::new(width, color));
        painter.circle_filled(p1, width / 2.0, color);
        painter.circle_filled(p2, width / 2.0, color);
    };

    match btn_type {
        WindowButton::Close => {
            let d = radius * 0.35;
            draw_rounded_line(center - egui::vec2(d, d), center + egui::vec2(d, d), line_w);
            draw_rounded_line(
                center - egui::vec2(d, -d),
                center + egui::vec2(d, -d),
                line_w,
            );
        }
        WindowButton::Minimize => {
            let d = radius * 0.4;
            draw_rounded_line(
                center - egui::vec2(d, 0.0),
                center + egui::vec2(d, 0.0),
                line_w,
            );
        }
        WindowButton::Maximize => {
            if is_maximized {
                // 向内收缩的图标（"还原/退出全屏"）
                // 解决米老鼠效应：draw_rounded_line 在线段较短时（尤其是两端圆角半径较大），
                // 两个短线段呈 90 度交汇，会导致顶点处的圆角和端点圆角交叠，看起来像三个小圆球。
                // 我们直接使用一条 PathLine 无缝绘制，去掉手动指定的顶点圆块，从而保持清爽的 V 字。
                let d = radius * 0.5;
                let gap = radius * 0.10;

                // 右上角的 └ 形状
                painter.add(egui::Shape::line(
                    vec![
                        center + egui::vec2(d, -gap),
                        center + egui::vec2(gap, -gap),
                        center + egui::vec2(gap, -d),
                    ],
                    egui::Stroke::new(line_w, color),
                ));

                // 左下角的 ┐ 形状
                painter.add(egui::Shape::line(
                    vec![
                        center + egui::vec2(-d, gap),
                        center + egui::vec2(-gap, gap),
                        center + egui::vec2(-gap, d),
                    ],
                    egui::Stroke::new(line_w, color),
                ));
            } else {
                // "+" 号，表示"最大化"
                let d = radius * 0.4;
                draw_rounded_line(
                    center - egui::vec2(d, 0.0),
                    center + egui::vec2(d, 0.0),
                    line_w,
                );
                draw_rounded_line(
                    center - egui::vec2(0.0, d),
                    center + egui::vec2(0.0, d),
                    line_w,
                );
            }
        }
        WindowButton::Settings => {
            let center_r = radius * 0.25;
            painter.circle_stroke(center, center_r, egui::Stroke::new(line_w, color));
            for i in 0..6 {
                let angle = (i as f32) * PI / 3.0;
                let dir = egui::vec2(angle.cos(), angle.sin());
                let p1 = center + dir * (center_r + 0.5);
                let p2 = center + dir * (radius * 0.55);
                draw_rounded_line(p1, p2, line_w * 0.8);
            }
        }
        WindowButton::AI => {
            // 画一个可爱的 AI 星星/火花图标
            let d1 = radius * 0.45;
            let d2 = radius * 0.15;

            let p_top = center - egui::vec2(0.0, d1);
            let p_bottom = center + egui::vec2(0.0, d1);
            let p_left = center - egui::vec2(d1, 0.0);
            let p_right = center + egui::vec2(d1, 0.0);

            // 四个内缩点
            let p_tl = center - egui::vec2(d2, d2);
            let p_tr = center + egui::vec2(d2, -d2);
            let p_bl = center + egui::vec2(-d2, d2);
            let p_br = center + egui::vec2(d2, d2);

            let stroke = egui::Stroke::new(line_w * 0.8, color);
            painter.add(egui::Shape::convex_polygon(
                vec![p_top, p_tr, p_right, p_br, p_bottom, p_bl, p_left, p_tl],
                color,
                stroke,
            ));

            // 右上角加个小星星
            let small_star_center = center + egui::vec2(radius * 0.35, -radius * 0.35);
            painter.circle_filled(small_star_center, radius * 0.1, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_button_clone() {
        let btn = WindowButton::Close;
        let cloned = btn.clone();
        assert!(btn == cloned);
    }

    #[test]
    fn test_window_button_copy() {
        let btn = WindowButton::Minimize;
        let copied = btn;
        assert!(btn == copied);
    }

    #[test]
    fn test_window_button_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(WindowButton::Close);
        set.insert(WindowButton::Maximize);
        set.insert(WindowButton::Minimize);
        set.insert(WindowButton::Settings);
        set.insert(WindowButton::AI);
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn test_window_button_equality() {
        assert!(WindowButton::Close == WindowButton::Close);
        assert!(WindowButton::Maximize == WindowButton::Maximize);
        assert!(WindowButton::Minimize == WindowButton::Minimize);
        assert!(WindowButton::Settings == WindowButton::Settings);
        assert!(WindowButton::AI == WindowButton::AI);
        assert!(WindowButton::Close != WindowButton::Maximize);
    }

    #[test]
    fn test_window_button_all_variants() {
        let variants = [
            WindowButton::Close,
            WindowButton::Maximize,
            WindowButton::Minimize,
            WindowButton::Settings,
            WindowButton::AI,
        ];
        assert_eq!(variants.len(), 5);
        for v in variants {
            assert!(
                matches!(v, WindowButton::Close)
                    || matches!(v, WindowButton::Maximize)
                    || matches!(v, WindowButton::Minimize)
                    || matches!(v, WindowButton::Settings)
                    || matches!(v, WindowButton::AI)
            );
        }
    }
}
