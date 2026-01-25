//! Shared UI components

pub mod album_card;
pub mod album_detail;
pub mod app_layout;
pub mod button;
pub mod dropdown;
pub mod error_toast;
pub mod helpers;
pub mod icons;
pub mod import;
pub mod imports;
pub mod library;
pub mod modal;
pub mod playback;
pub mod resizable_panel;
pub mod settings;
pub mod title_bar;
pub mod utils;

pub use album_card::AlbumCard;
pub use album_detail::release_tabs_section::ReleaseTorrentInfo;
pub use album_detail::{
    AlbumArt, AlbumCoverSection, AlbumDetailView, AlbumMetadata, DeleteAlbumDialog,
    DeleteReleaseDialog, ExportErrorToast, PlayAlbumButton, ReleaseInfoModal, ReleaseTabsSection,
    TrackRow,
};
pub use app_layout::AppLayoutView;
pub use button::{Button, ButtonSize, ButtonVariant, ChromelessButton};
pub use dioxus_virtual_scroll::{
    GridLayout, KeyFn, RenderFn, ScrollTarget, VirtualGrid, VirtualGridConfig,
};
pub use dropdown::{Dropdown, Placement};
pub use error_toast::ErrorToast;
pub use helpers::{BackButton, ConfirmDialogView, ErrorDisplay, LoadingSpinner};
pub use icons::{
    AlertTriangleIcon, ArrowLeftIcon, CheckIcon, ChevronDownIcon, ChevronLeftIcon,
    ChevronRightIcon, DiscIcon, DownloadIcon, EllipsisIcon, ExternalLinkIcon, FileIcon,
    FileTextIcon, FolderIcon, ImageIcon, InfoIcon, KeyIcon, LayersIcon, LoaderIcon, LockIcon,
    MenuIcon, MonitorIcon, PauseIcon, PencilIcon, PlayIcon, PlusIcon, RefreshIcon, RowsIcon,
    SettingsIcon, SkipBackIcon, SkipForwardIcon, StarIcon, TrashIcon, UploadIcon, XIcon,
};
pub use import::{
    CdDriveStatus, CdSelectorView, ConfirmationView, DiscIdLookupErrorView, FileListView,
    FolderImportView, FolderImportViewProps, ImportErrorDisplayView, ImportSource,
    ImportSourceSelectorView, ImportView, ManualSearchPanelView, MatchItemView, MatchListView,
    MetadataDetectionPromptView, MultipleExactMatchesView, ReleaseSelectorView, ReleaseSidebarView,
    SearchSourceSelectorView, SelectedSourceView, TorrentFilesDisplayView, TorrentInfoDisplayView,
    TorrentInputMode, TorrentInputView, TorrentTrackerDisplayView, TrackerConnectionStatus,
    TrackerStatus,
};
pub use imports::{ImportsButtonView, ImportsDropdownView};
pub use library::LibraryView;
pub use modal::Modal;
pub use playback::{NowPlayingBarView, QueueSidebarState, QueueSidebarView};
pub use resizable_panel::{GrabBar, PanelPosition, ResizablePanel, ResizeDirection};
pub use settings::{
    AboutSectionView, ApiKeysSectionView, BitTorrentSectionView, BitTorrentSettings,
    EncryptionSectionView, SettingsTab, SettingsView, StorageLocation, StorageProfile,
    StorageProfileEditorView, StorageProfilesSectionView, SubsonicSectionView,
};
pub use title_bar::{NavItem, SearchResult, TitleBarView, UpdateState};
pub use utils::{format_duration, format_file_size};
