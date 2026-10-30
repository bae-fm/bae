import BaeKit
import Foundation
import SwiftUI

// MARK: - Columns

/// Identifiers and seed metadata for the storage columns, each with the
/// field core sorts it by. The widths are the standard layout's; the person
/// resizes, reorders and hides columns, which `StorageTableLayout` keeps.
enum StorageTableColumn: String, CaseIterable, Codable {
    case release
    case artist
    case media
    case storage
    case files
    case size
    case dateAdded

    /// One column's metadata, kept in a single spec so the per-property
    /// accessors don't fan out into four parallel switches over the same cases.
    private struct Spec {
        let title: String
        let width: CGFloat
        let minWidth: CGFloat
        let sortField: BridgeStorageSortField
    }

    private var spec: Spec {
        switch self {
        case .release:
            Spec(
                title: String(localized: "Release"),
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
                width: 190,
                minWidth: 150,
                sortField: .storage
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
        case .dateAdded:
            Spec(
                title: String(localized: "Date Added"),
                width: 120,
                minWidth: 80,
                sortField: .dateAdded
            )
        }
    }

    var title: String { spec.title }
    var width: CGFloat { spec.width }
    var minWidth: CGFloat { spec.minWidth }
    var sortField: BridgeStorageSortField { spec.sortField }

    /// Whether the person can hide the column. The release column names each
    /// row, so it always shows: the table never runs out of columns.
    var hideable: Bool { self != .release }
}

/// `NSSortDescriptor.key` values for the columns: the column raw value,
/// mapped back to a `BridgeStorageSortField` when the user clicks a header.
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
