//! Shared UI components

pub mod album_card;
pub mod album_detail;
pub mod app_layout;
pub mod dialog;
pub mod error_toast;
pub mod helpers;
pub mod icons;
pub mod import;
pub mod imports;
pub mod library;
pub mod playback;
pub mod settings;
pub mod title_bar;
pub mod utils;
pub mod virtual_grid;

pub use album_card::AlbumCard;
pub use album_detail::release_tabs_section::ReleaseTorrentInfo;
pub use album_detail::{
    AlbumArt, AlbumCoverSection, AlbumDetailView, AlbumMetadata, DeleteAlbumDialog,
    DeleteReleaseDialog, ExportErrorToast, PlayAlbumButton, ReleaseInfoModal, ReleaseTabsSection,
    TrackRow,
};
pub use app_layout::AppLayoutView;
pub use dialog::GlobalDialogView;
pub use error_toast::ErrorToast;
pub use helpers::{BackButton, ErrorDisplay, LoadingSpinner, PageContainer};
pub use icons::{
    AlertTriangleIcon, ArrowLeftIcon, CheckIcon, ChevronDownIcon, ChevronLeftIcon,
    ChevronRightIcon, DiscIcon, DownloadIcon, EllipsisIcon, ExternalLinkIcon, FileTextIcon,
    FolderIcon, InfoIcon, KeyIcon, LoaderIcon, LockIcon, MenuIcon, MusicIcon, PauseIcon,
    PencilIcon, PlayIcon, PlusIcon, RefreshIcon, SkipBackIcon, SkipForwardIcon, StarIcon,
    TrashIcon, UploadIcon, XIcon,
};
pub use import::{
    CdDriveStatus, CdSelectorView, ConfirmationView, DetectingMetadataView, DiscIdLookupErrorView,
    ExactLookupView, FileListView, FolderImportView, FolderImportViewProps, FolderSelectorView,
    ImportErrorDisplayView, ImportSource, ImportSourceSelectorView, ImportView,
    ManualSearchPanelView, MatchItemView, MatchListView, MetadataDetectionPromptView,
    ReleaseSelectorView, SearchSourceSelectorView, SelectedSourceView, TorrentFilesDisplayView,
    TorrentInfoDisplayView, TorrentInputMode, TorrentInputView, TorrentTrackerDisplayView,
    TrackerConnectionStatus, TrackerStatus,
};
pub use imports::{ImportsButtonView, ImportsDropdownView};
pub use library::LibraryView;
pub use playback::{NowPlayingBarView, QueueSidebarState, QueueSidebarView};
pub use settings::{
    AboutSectionView, ApiKeysSectionView, BitTorrentSectionView, BitTorrentSettings,
    EncryptionSectionView, SettingsTab, SettingsView, StorageLocation, StorageProfile,
    StorageProfileEditorView, StorageProfilesSectionView, SubsonicSectionView,
};
pub use title_bar::{NavItem, SearchResult, TitleBarView};
pub use utils::{format_duration, format_file_size};
pub use virtual_grid::{RenderFn, ScrollTarget, VirtualGrid, VirtualGridConfig};
