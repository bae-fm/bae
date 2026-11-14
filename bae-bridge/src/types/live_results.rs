/// A window of a list: where the screen counts it from, how many rows it
/// holds, and the cursor to read it from when the screen holds the row
/// beside it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeLibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
    pub cursor: Option<BridgeWindowCursor>,
}

/// The row beside a window, by its sort values, and which side the window
/// lies on. The values are those a delivered window gave for its first or
/// last row.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeWindowCursor {
    pub side: BridgeCursorSide,
    pub key: Vec<BridgeSortValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCursorSide {
    /// The window holds the rows after the row at the key.
    After,
    /// The window holds the rows before the row at the key.
    Before,
}

/// One sort value of a row, as the list's order evaluates it for the row.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeSortValue {
    Null,
    Integer { value: i64 },
    Real { value: f64 },
    Text { value: String },
    Blob { value: Vec<u8> },
}

/// One artist's group in the album grid grouped by artist.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryBrowseSection {
    /// The artist's id.
    pub id: String,
    /// The artist's name.
    pub title: String,
    /// The artist's picture, if it has one.
    pub image: Option<crate::types::BridgeImageRef>,
    /// Where the artist's albums start in the grid's rows, and all of them:
    /// never a page of them, so `limit` is how many albums the artist has.
    pub window: BridgeLibraryPageWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLiveQueryCause {
    Request,
    Write,
    RequestAndWrite,
}

impl BridgeLibraryBrowseSection {
    pub(crate) fn from_core(section: bae_views::LibraryBrowseSection) -> Self {
        Self {
            id: section.id,
            title: section.title,
            image: section.image.map(crate::types::BridgeImageRef::from_core),
            window: BridgeLibraryPageWindow::from_core(section.window),
        }
    }
}
