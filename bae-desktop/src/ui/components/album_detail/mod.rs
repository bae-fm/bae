//! Album detail components
//!
//! Re-exports pure view components from bae-ui and provides app-specific
//! components like the page wrapper with routing.

// App-specific components
mod back_button;
mod error;
mod loading;
mod page;
pub mod utils;

// Re-export the main view component from bae-ui
pub use bae_ui::AlbumDetailView;

// App-specific exports
pub use page::AlbumDetail;
