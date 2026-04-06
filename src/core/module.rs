use super::events::AppEvent;
use super::state::GlobalState;
use eframe::egui;

#[allow(dead_code)]
pub trait AppModule {
    fn name(&self) -> &str;
    fn icon(&self) -> &str;

    fn show_content(
        &mut self,
        ui: &mut egui::Ui,
        state: &GlobalState,
        tx: &flume::Sender<AppEvent>,
    );

    fn id(&self) -> &str {
        self.name()
    }

    fn status_bar_hint(&self) -> &str {
        "就绪"
    }
    fn on_exit(&mut self) {}
}
