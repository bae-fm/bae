//! bae demo - Web demo for screenshot generation
//!
//! A minimal web app that renders UI components with fixture data.
//! Used for Playwright-based screenshot generation.

mod demo_data;
mod mocks;
mod pages;

use dioxus::prelude::*;
use pages::{
    AlbumDetail, DemoLayout, Import, Library, MockAlbumDetail, MockFolderImport, MockIndex,
    Settings,
};

pub const FAVICON: Asset = asset!("/assets/favicon.ico");
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
pub const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(DemoLayout)]
    #[route("/")]
    Library {},
    #[route("/album/:album_id")]
    AlbumDetail { album_id: String },
    #[route("/import")]
    Import {},
    #[route("/settings")]
    Settings {},
    #[end_layout]
    // Mock routes (no app layout, with controls)
    #[route("/mocks")]
    MockIndex {},
    #[route("/mock/folder-import?:state")]
    MockFolderImport { state: Option<String> },
    #[route("/mock/album-detail?:state")]
    MockAlbumDetail { state: Option<String> },
}

#[component]
pub fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "min-h-screen", Router::<Route> {} }
    }
}

fn main() {
    dioxus::launch(App);
}
