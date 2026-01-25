use crate::ui::app_context::AppServices;
use crate::ui::app_service::AppService;
use crate::ui::{Route, FAVICON, FLOATING_UI_CORE, FLOATING_UI_DOM, MAIN_CSS, TAILWIND_CSS};
use dioxus::prelude::*;
use tracing::debug;

#[component]
pub fn App() -> Element {
    debug!("Rendering app component");

    // Get backend services from launch context
    let services = use_context::<AppServices>();

    // Create AppService (owns Store + handles event subscriptions)
    let app_service = AppService::new(&services);

    // Start all event subscriptions
    app_service.start_subscriptions();

    // Provide AppService as context for all components
    use_context_provider(|| app_service.clone());

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: FLOATING_UI_CORE }
        document::Script { src: FLOATING_UI_DOM }
        Router::<Route> {}
    }
}
