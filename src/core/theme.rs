use eframe::egui;
use serde::{Deserialize, Serialize};

// ==========================================
// 🌟 魔法核心配置区 (物理常量，不可变)
// ==========================================
pub const MIN_VISUAL_WIDTH: f32 = 320.0;
pub const MIN_VISUAL_HEIGHT: f32 = 240.0;
pub const SHADOW_MARGIN: f32 = 16.0;

pub const RESIZE_EDGE_WIDTH: f32 = 6.0;
pub const RESIZE_CORNER_SIZE: f32 = 14.0;

pub const LOGO_OUTER_RATIO: f32 = 0.25;
pub const LOGO_INNER_RATIO: f32 = 0.6;
pub const LOGO_STROKE_RATIO: f32 = 0.06;
pub const TITLE_SPACING_RATIO: f32 = 0.25;
pub const TITLE_FONT_RATIO: f32 = 1.6;
pub const BTN_RADIUS_RATIO: f32 = 0.18;
pub const BTN_SPACING_RATIO: f32 = 0.24;
pub const BTN_ICON_RATIO: f32 = 0.22;
pub const BTN_STROKE_WIDTH: f32 = 0.5;
pub const BTN_STROKE_ALPHA: u8 = 40;
pub const BTN_HOVER_SCALE: f32 = 0.2;

pub const ANIM_DURATION: f32 = 0.1;
pub const PEEK_SHADOW_MULTIPLIER: f32 = 0.8;
pub const PEEK_SHADOW_MIN_ALPHA: f32 = 40.0;
pub const PEEK_BORDER_MIN_WIDTH: f32 = 1.5;
pub const UI_FADE_THRESHOLD: f32 = 0.01;

// ==========================================
// ✨ 序列化魔法：内存存 Color32，JSON 存 Hex
// ==========================================
pub mod color_hex {
    use eframe::egui::Color32;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(color: &Color32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex = format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r(),
            color.g(),
            color.b(),
            color.a()
        );
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Color32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Color32::from_hex(&s).unwrap_or(Color32::MAGENTA))
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub is_dark: bool,

    #[serde(with = "color_hex")]
    pub bg_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub title_bg_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub text_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub border_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub shadow_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub scrollbar_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub logo_color: egui::Color32,
    #[serde(with = "color_hex")]
    pub widget_bg_color: egui::Color32,

    pub bg_opacity: f32,
    pub corner_proportion: f32,
    pub shadow_intensity: f32,
    pub title_bar_height: u32,
    pub shadow_blur: u32,
    pub shadow_spread: u32,
    pub border_thickness: u32,

    pub ui_rounding: u8,
    pub ui_spacing: f32,
    pub heading_font_size: u32,
    pub body_font_size: u32,

    #[serde(with = "color_hex")]
    pub btn_close_bg: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_close_icon: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_max_bg: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_max_icon: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_min_bg: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_min_icon: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_set_bg: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_set_icon: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_ai_bg: egui::Color32,
    #[serde(with = "color_hex")]
    pub btn_ai_icon: egui::Color32,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "极简纯白".to_string(),
            is_dark: false,
            bg_color: egui::Color32::from_rgb(255, 255, 255),
            title_bg_color: egui::Color32::from_rgb(230, 230, 235),
            text_color: egui::Color32::from_rgb(40, 40, 40),
            border_color: egui::Color32::from_rgba_premultiplied(0, 0, 0, 64),
            shadow_color: egui::Color32::from_rgba_premultiplied(0, 0, 0, 48),
            scrollbar_color: egui::Color32::from_rgb(192, 192, 200),
            logo_color: egui::Color32::from_rgb(195, 39, 43),
            widget_bg_color: egui::Color32::from_rgb(208, 208, 216),

            bg_opacity: 0.96,
            corner_proportion: 1.0,
            shadow_intensity: 1.0,
            title_bar_height: 50,
            shadow_blur: 12,
            shadow_spread: 2,
            border_thickness: 1,

            ui_rounding: 20,
            ui_spacing: 16.0,
            heading_font_size: 24,
            body_font_size: 16,

            btn_close_bg: egui::Color32::from_rgb(255, 95, 86),
            btn_close_icon: egui::Color32::from_rgb(77, 0, 0),
            btn_max_bg: egui::Color32::from_rgb(39, 201, 63),
            btn_max_icon: egui::Color32::from_rgb(0, 77, 0),
            btn_min_bg: egui::Color32::from_rgb(255, 189, 46),
            btn_min_icon: egui::Color32::from_rgb(90, 48, 0),
            btn_set_bg: egui::Color32::from_rgb(178, 102, 255),
            btn_set_icon: egui::Color32::from_rgb(60, 0, 120),
            btn_ai_bg: egui::Color32::from_rgb(64, 196, 255), // 清新的天蓝色
            btn_ai_icon: egui::Color32::from_rgb(0, 70, 120),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_default_values() {
        let default_theme = ThemeConfig::default();
        assert_eq!(default_theme.name, "极简纯白");
        assert_eq!(default_theme.is_dark, false);
        assert_eq!(default_theme.title_bar_height, 50);
        assert_eq!(default_theme.ui_rounding, 20);
    }

    #[test]
    fn test_theme_color_hex_invalid() {
        // 测试解析非法的颜色格式时是否会回退到 MAGENTA
        let json_data = r#""invalid_color""#;
        #[derive(Deserialize)]
        struct TestColor {
            #[serde(with = "color_hex")]
            color: egui::Color32,
        }
        let parsed: Result<TestColor, _> =
            serde_json::from_str(&format!(r#"{{"color": {}}}"#, json_data));
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().color, egui::Color32::MAGENTA);
    }

    #[test]
    fn test_theme_serialization() {
        let theme = ThemeConfig::default();
        let json_str = serde_json::to_string(&theme).expect("Failed to serialize ThemeConfig");

        // 验证序列化后必定包含我们使用 hex 转换的某些颜色属性
        assert!(
            json_str.contains("\"bg_color\":\"#FFFFFF\"")
                || json_str.contains("\"bg_color\":\"#FFFFFFFF\"")
        );
        assert!(json_str.contains("\"name\":\"极简纯白\""));
    }

    #[test]
    fn test_theme_deserialization() {
        // 使用一个精简版的 JSON 测试反序列化
        let json_data = r###"{
            "name": "测试主题",
            "is_dark": true,
            "bg_color": "#000000FF",
            "title_bg_color": "#111111FF",
            "text_color": "#FFFFFFFF",
            "border_color": "#222222FF",
            "shadow_color": "#333333FF",
            "scrollbar_color": "#444444FF",
            "logo_color": "#555555FF",
            "widget_bg_color": "#666666FF",
            "bg_opacity": 0.8,
            "corner_proportion": 0.5,
            "shadow_intensity": 1.5,
            "title_bar_height": 40,
            "shadow_blur": 10,
            "shadow_spread": 2,
            "border_thickness": 2,
            "ui_rounding": 4,
            "ui_spacing": 20.0,
            "heading_font_size": 24,
            "body_font_size": 16,
            "btn_close_bg": "#FF0000FF",
            "btn_close_icon": "#FFFFFF",
            "btn_max_bg": "#00FF00FF",
            "btn_max_icon": "#FFFFFF",
            "btn_min_bg": "#0000FFFF",
            "btn_min_icon": "#FFFFFF",
            "btn_set_bg": "#888888FF",
            "btn_set_icon": "#FFFFFF",
            "btn_ai_bg": "#40C4FFFF",
            "btn_ai_icon": "#004678FF"
        }"###;

        let parsed_theme: ThemeConfig =
            serde_json::from_str(json_data).expect("Failed to parse JSON");

        assert_eq!(parsed_theme.name, "测试主题");
        assert_eq!(parsed_theme.is_dark, true);
        assert_eq!(parsed_theme.bg_opacity, 0.8);
        assert_eq!(parsed_theme.title_bar_height, 40);

        // 验证自定义颜色反序列化是否生效 (16进制解析)
        assert_eq!(
            parsed_theme.bg_color,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 255)
        );
        assert_eq!(
            parsed_theme.btn_close_bg,
            egui::Color32::from_rgba_unmultiplied(255, 0, 0, 255)
        );
    }
}
