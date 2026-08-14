use super::*;

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_ui_events(&self, callback: Box<dyn crate::types::UiEventCallback>) {
        let bus = self.ui_event_bus.clone();
        let runtime = self.runtime.handle().clone();
        crate::operation_runtime::spawn(runtime, move || async move {
            let rx = bus.subscribe();
            pump_ui_events(rx, callback).await;
        });
    }
}
