//! bae demo - Web demo for screenshot generation
//!
//! A minimal web app that renders UI components with fixture data.
//! Used for Playwright-based screenshot generation.

pub mod demo_data;
pub mod mocks;
pub mod pages;
pub mod storage;
pub mod ui;

use dioxus::prelude::*;
use pages::{
    AlbumDetail, DemoLayout, Import, Library, MockAlbumDetail, MockButton, MockDropdownTest,
    MockFolderImport, MockIndex, MockLibrary, MockTitleBar, Settings,
};

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
pub const FLOATING_UI_CORE: Asset = asset!("/assets/floating-ui.core.min.js");
pub const FLOATING_UI_DOM: Asset = asset!("/assets/floating-ui.dom.min.js");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    // Mock index at root
    #[route("/")]
    MockIndex {},
    // Demo app with full layout
    #[layout(DemoLayout)]
    #[route("/app")]
    Library {},
    #[route("/app/album/:album_id")]
    AlbumDetail { album_id: String },
    #[route("/app/import")]
    Import {},
    #[route("/app/settings")]
    Settings {},
    #[end_layout]
    // Mock pages with controls
    #[route("/button?:state")]
    MockButton { state: Option<String> },
    #[route("/folder-import?:state")]
    MockFolderImport { state: Option<String> },
    #[route("/album-detail?:state")]
    MockAlbumDetail { state: Option<String> },
    #[route("/library?:state")]
    MockLibrary { state: Option<String> },
    #[route("/title-bar?:state")]
    MockTitleBar { state: Option<String> },
    #[route("/dropdown-test")]
    MockDropdownTest {},
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: FLOATING_UI_CORE }
        document::Script { src: FLOATING_UI_DOM }
        div { class: "min-h-screen", Router::<Route> {} }
    }
}
