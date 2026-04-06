pub mod dashboard;
pub mod settings;
pub mod terminal;

pub fn build_app_modules() -> Vec<Box<dyn crate::core::module::AppModule>> {
    vec![
        Box::new(dashboard::DashboardModule::new()),
        Box::new(settings::SettingsModule::new()),
        Box::new(terminal::Terminal::new()),
    ]
}
