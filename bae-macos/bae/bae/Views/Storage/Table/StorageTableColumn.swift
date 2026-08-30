import BaeKit
import Foundation
import SwiftUI

// MARK: - Columns

/// Identifiers and seed metadata for the six storage columns. The widths are
/// the initial layout only; the table view owns resize/reorder at runtime.
/// The storage column has no sort field — storage state isn't a sort key the
/// core exposes.
enum StorageTableColumn: String, CaseIterable {
    case album
    case artist
    case media
    case storage
    case files
    case size

    /// One column's metadata, kept in a single spec so the per-property
    /// accessors don't fan out into four parallel switches over the same cases.
    private struct Spec {
        let title: String
        let width: CGFloat
        let minWidth: CGFloat
        let sortField: BridgeStorageSortField?
    }

    private var spec: Spec {
        switch self {
        case .album:
            Spec(
                title: String(localized: "Album"),
                width: 280,
                minWidth: 160,
                sortField: .albumTitle
            )
        case .artist:
            Spec(
                title: String(localized: "Artist"),
                width: 200,
                minWidth: 100,
                sortField: .artistNames
            )
        case .media:
            Spec(
                title: coreString("core.release.media"),
                width: 80,
                minWidth: 60,
                sortField: .media
            )
        case .storage:
            Spec(
                title: String(localized: "Storage"),
                width: 140,
                minWidth: 100,
                sortField: nil
            )
        case .files:
            Spec(
                title: String(localized: "Files"),
                width: 60,
                minWidth: 44,
                sortField: .fileCount
            )
        case .size:
            Spec(
                title: String(localized: "Size"),
                width: 100,
                minWidth: 70,
                sortField: .totalSize
            )
        }
    }

    var title: String { spec.title }
    var width: CGFloat { spec.width }
    var minWidth: CGFloat { spec.minWidth }
    var sortField: BridgeStorageSortField? { spec.sortField }
}

/// `NSSortDescriptor.key` values for the sortable columns: the column raw
/// value, mapped back to a `BridgeStorageSortField` when the user clicks a
/// header.
func sortField(forDescriptorKey key: String) -> BridgeStorageSortField? {
    StorageTableColumn(rawValue: key)?.sortField
}

/// Files and Size read right-aligned (numeric); the rest are leading.
func cellAlignment(_ column: StorageTableColumn) -> Alignment {
    switch column {
    case .files, .size: .trailing
    default: .leading
    }
}
