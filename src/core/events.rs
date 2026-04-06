use crate::core::theme::ThemeConfig;
use std::sync::OnceLock;

pub static GLOBAL_EVENT_TX: OnceLock<flume::Sender<AppEvent>> = OnceLock::new();

pub enum AppEvent {
    #[allow(dead_code)]
    LogMessage(String),
    UpdateTheme(ThemeConfig),
    UpdateDragTransparent(bool),
    ToastRequest { text: String, is_error: bool },
    FatalError(String),
    SysInfoUpdate { cpu_usage: f32, mem_usage: u64 },
    #[allow(dead_code)]
    DataReady,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_event_log_message() {
        let event = AppEvent::LogMessage("Test log".to_string());
        assert!(matches!(event, AppEvent::LogMessage(_)));
    }

    #[test]
    fn test_app_event_update_theme() {
        let theme = ThemeConfig::default();
        let event = AppEvent::UpdateTheme(theme.clone());
        assert!(matches!(event, AppEvent::UpdateTheme(t) if t.name == "极简纯白"));
    }

    #[test]
    fn test_app_event_update_drag_transparent() {
        let event_true = AppEvent::UpdateDragTransparent(true);
        let event_false = AppEvent::UpdateDragTransparent(false);
        assert!(matches!(event_true, AppEvent::UpdateDragTransparent(true)));
        assert!(matches!(
            event_false,
            AppEvent::UpdateDragTransparent(false)
        ));
    }

    #[test]
    fn test_app_event_toast_request() {
        let event = AppEvent::ToastRequest {
            text: "Hello".to_string(),
            is_error: false,
        };
        assert!(
            matches!(event, AppEvent::ToastRequest { text, is_error: false } if text == "Hello")
        );

        let error_event = AppEvent::ToastRequest {
            text: "Error".to_string(),
            is_error: true,
        };
        assert!(
            matches!(error_event, AppEvent::ToastRequest { text, is_error: true } if text == "Error")
        );
    }

    #[test]
    fn test_app_event_fatal_error() {
        let event = AppEvent::FatalError("Critical failure".to_string());
        assert!(matches!(event, AppEvent::FatalError(msg) if msg == "Critical failure"));
    }

    #[test]
    fn test_app_event_sys_info_update() {
        let event = AppEvent::SysInfoUpdate {
            cpu_usage: 50.0,
            mem_usage: 1024,
        };
        assert!(matches!(
            event,
            AppEvent::SysInfoUpdate {
                cpu_usage: 50.0,
                mem_usage: 1024
            }
        ));
    }

    #[test]
    fn test_app_event_data_ready() {
        let event = AppEvent::DataReady;
        assert!(matches!(event, AppEvent::DataReady));
    }

    #[test]
    fn test_app_event_all_variants() {
        let events = vec![
            AppEvent::LogMessage("test".to_string()),
            AppEvent::UpdateTheme(ThemeConfig::default()),
            AppEvent::UpdateDragTransparent(true),
            AppEvent::ToastRequest {
                text: "test".to_string(),
                is_error: false,
            },
            AppEvent::FatalError("test".to_string()),
            AppEvent::SysInfoUpdate {
                cpu_usage: 0.0,
                mem_usage: 0,
            },
            AppEvent::DataReady,
        ];
        assert_eq!(events.len(), 7);
    }
}
