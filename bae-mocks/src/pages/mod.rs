//! Demo and mock pages

mod album_detail;
mod import;
mod layout;
mod library;
mod mock_dropdown;
mod mock_index;
mod settings;

pub use album_detail::AlbumDetail;
pub use import::Import;
pub use layout::DemoLayout;
pub use library::Library;
pub use mock_dropdown::MockDropdownTest;
pub use mock_index::{
    MockAlbumDetail, MockButton, MockFolderImport, MockIndex, MockLibrary, MockPill, MockTextInput,
    MockTitleBar, MockTooltip,
};
pub use settings::Settings;
