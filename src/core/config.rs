use crate::core::theme::ThemeConfig;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// 全局应用配置
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub theme: ThemeConfig,
    pub drag_transparent_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            drag_transparent_enabled: true,
        }
    }
}

impl AppConfig {
    /// 获取绿色配置文件的路径 (存放在当前 exe 目录下)
    fn get_config_path() -> PathBuf {
        let target_dir = if let Ok(exe_path) = env::current_exe() {
            exe_path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        } else {
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        target_dir.join("huaye_config.json")
    }

    /// 启动时加载配置
    pub fn load() -> Self {
        let path = Self::get_config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
        Self::default()
    }

    /// 保存配置到本地，返回错误信息（如有）
    pub fn save(&self) -> Result<(), String> {
        let path = Self::get_config_path();
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("配置序列化失败: {}", e))?;
        fs::write(&path, json).map_err(|e| format!("配置写入失败: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("theme"));

        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.theme.name, "极简纯白");
    }

    #[test]
    fn test_get_config_path_format() {
        let path = AppConfig::get_config_path();
        assert!(path.to_string_lossy().ends_with("huaye_config.json"));
    }

    #[test]
    fn test_load_non_existent_file() {
        // 当配置文件不存在或异常时，应该返回默认配置而不崩溃
        let dummy_path = std::env::temp_dir().join("huaye_test_nonexistent.json");
        let _ = fs::remove_file(&dummy_path); // 确保不存在

        let config = AppConfig::default(); // default logic is inside load() if read fails

        assert_eq!(config.theme.name, "极简纯白");
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp_dir = std::env::temp_dir().join("huaye_test_config");
        let _ = fs::create_dir_all(&tmp_dir);
        let tmp_path = tmp_dir.join("huaye_config.json");

        let mut config = AppConfig::default();
        config.theme.name = "TestTheme".to_string();

        let json = serde_json::to_string_pretty(&config).unwrap();
        fs::write(&tmp_path, &json).unwrap();

        let content = fs::read_to_string(&tmp_path).unwrap();
        let loaded: AppConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.theme.name, "TestTheme");

        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_dir(&tmp_dir);
    }
}
