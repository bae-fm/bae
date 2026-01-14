// Context providers used by App component (cross-platform)
pub mod active_imports_context;
pub mod library_search_context;

// Desktop-only modules (used by TitleBar on all platforms)
pub mod imports_button;
pub mod imports_dropdown;
pub mod title_bar;
pub mod update_indicator;

pub mod album_detail;
pub mod app;
pub mod dialog;
pub mod dialog_context;
pub mod import;
pub mod library;
pub mod navbar;
pub mod now_playing_bar;
pub mod playback_hooks;
pub mod queue_sidebar;
pub mod settings;

pub use album_detail::AlbumDetail;
pub use app::App;
pub use library::Library;
pub use library_search_context::use_library_search;
pub use navbar::Navbar;
pub use playback_hooks::{use_playback_service, use_playback_state};
pub use settings::Settings;
pub use title_bar::TitleBar;
