use crate::core::theme::ThemeConfig;

#[allow(dead_code)]
pub struct GlobalState {
    pub version: String,
    pub theme: ThemeConfig,
    pub drag_transparent_enabled: bool,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            version: format!("{} (Modular)", env!("CARGO_PKG_VERSION")),
            theme: ThemeConfig::default(),
            drag_transparent_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_state_default() {
        let state = GlobalState::default();
        assert!(state.drag_transparent_enabled);
        assert!(state.version.ends_with("(Modular)"));
        assert_eq!(state.theme.name, "极简纯白");
    }

    #[test]
    fn test_global_state_modification() {
        let mut state = GlobalState::default();
        state.drag_transparent_enabled = false;
        state.version = "1.0.0".to_string();

        assert!(!state.drag_transparent_enabled);
        assert_eq!(state.version, "1.0.0");
    }
}
