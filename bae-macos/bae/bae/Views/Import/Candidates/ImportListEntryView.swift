import BaeKit
import Combine
import SwiftUI

/// What the person can do from a row of the import list.
struct ImportListEntryActions {
    /// Fold `group` open (`true`) or shut.
    let onSetGroupExpanded:
        (_ group: BridgeTriageGroup, _ expanded: Bool) ->
            Void
    /// Read every release below the header's folder as one.
    let onCombineFolder: (_ key: BridgeFolderReleaseDecisionKey) -> Void
    /// Read the release at `key` as the folders it is made of.
    let onSeparate: (_ key: String) -> Void
    /// Open the release this device imported from a Done row's files.
    let onShowInLibrary: (_ release: BridgeImportedRelease) -> Void
    /// Run one action a row's menu offers, for the row or the selection.
    let onPerform: (ImportCandidateActionOffer) -> Void
    /// What a candidate row's menu offers, asked when the menu opens.
    let menuOffers: (_ row: BridgeTriageRow) -> CandidateActionMenu
}

/// What the list's rows read from the window around the list. A table cell
/// hosts its row apart from the SwiftUI tree the list sits in, so the row
/// inherits none of it; the table hands it this.
struct ImportListRowEnvironment {
    let imageStore: ImageStore
    /// `nil` in a preview or a test with no bridge behind it.
    let importer: Importer?
    let candidateRuntime: AnyPublisher<[BridgeCandidateRuntimeChange], Never>
    /// The tone and accent the window draws in.
    let appearance: HostedAppearance
}

/// One row of the import list, with what it is drawn with.
struct ImportListEntry {
    let item: BridgeImportListItem
    /// The cover a candidate's row draws; `nil` for any other row.
    let coverContent: ImageContent?
    /// Whether a group header has the gap above its group drawn inside its
    /// row: every header but one at the top of the list.
    let gapAbove: Bool
    let actions: ImportListEntryActions
    let environment: ImportListRowEnvironment
}

/// One row of the import list as a table cell hosts it: a folder group's
/// header, a candidate's row, or a broken folder's row.
struct ImportListEntryView: View {
    let entry: ImportListEntry
    /// Increased while the table draws the row selected, which draws its
    /// text in the selection's colours.
    let prominence: BackgroundProminence
    /// Whether the row floats over the rows below; a floating header draws
    /// without the gap above its group.
    let floating: Bool

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .environment(\.backgroundProminence, prominence)
            .environment(entry.environment.imageStore)
            .environment(entry.environment.importer)
            .environment(
                \.candidateRuntimePublisher,
                entry.environment.candidateRuntime
            )
            .hostedAppearance(entry.environment.appearance)
    }

    @ViewBuilder
    private var content: some View {
        let actions = entry.actions
        switch entry.item {
        case .groupHeader(_, let group, _, let expanded, _):
            ImportListGroupHeader(
                group: group,
                expanded: expanded,
                onSetExpanded: { actions.onSetGroupExpanded(group, $0) },
                onCombine: { actions.onCombineFolder(group.key) }
            )
            // The gap above the group is the row's, out of the header's
            // surface. Floating over the rows below, the header has no gap
            // above it: it covers the whole row.
            .padding(
                .top,
                entry.gapAbove && !floating
                    ? ImportCandidateTableLayout.headerGap : 0
            )
        case .candidate(_, let row, let isGroupMember):
            // Every tab's row, drawn the same: a Done row is the row it was
            // in Found, and offers the release imported from it.
            TriageRowView(
                row: row,
                coverContent: entry.coverContent,
                isGroupMember: isGroupMember,
                menuOffers: { actions.menuOffers(row) },
                onPerform: actions.onPerform,
                onShowInLibrary: row.importedRelease.map { release in
                    { actions.onShowInLibrary(release) }
                }
            )
        case .invalid(_, let invalid, _, let isGroupMember):
            InvalidCandidateRow(
                invalid: invalid,
                isGroupMember: isGroupMember,
                onSeparate: { actions.onSeparate(invalid.candidateKey) }
            )
        }
    }
}

/// A folder group's header: its chevron and name, folding the group open or
/// shut on a click. Its members follow as rows of their own.
struct ImportListGroupHeader: View {
    let group: BridgeTriageGroup
    let expanded: Bool
    let onSetExpanded: (Bool) -> Void
    /// Read every release below the header's folder as one.
    let onCombine: () -> Void

    var body: some View {
        Button {
            onSetExpanded(!expanded)
        } label: {
            HStack(spacing: ImportListHierarchyLayout.headerSpacing) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .themeIcon(ImportListHierarchyLayout.headerChevron)
                    .foregroundStyle(.tertiary)
                    .frame(width: ImportListHierarchyLayout.headerChevron.size)
                Text(group.name)
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // Opaque, so a header floating over the rows below hides them.
        .background(Theme.surface)
        .contextMenu {
            // A header that only names a shared path component offers
            // nothing.
            if group.combinable {
                Button("Combine as One Release", action: onCombine)
            }
        }
    }
}

extension BridgeImportListItem {
    /// The candidate key selecting the row selects: `nil` for a group
    /// header, which is never selected.
    var selectionKey: String? {
        switch self {
        case .groupHeader:
            nil
        case .candidate(_, let row, _):
            row.candidateKey
        case .invalid(_, let invalid, _, _):
            invalid.candidateKey
        }
    }

    /// What typing a row's first letters matches: the first text the row
    /// draws — the release's title, or the folder's name where the row names
    /// its folder. `nil` for a group header, which typing never selects.
    var typeSelectText: String? {
        switch self {
        case .groupHeader:
            nil
        case .candidate(_, let row, _):
            switch row.reading {
            case .unidentified:
                row.folderName
            case .prefilled, .identified, .identifiedAlbum:
                ImportReleaseSummary(row: row)?.title
            }
        case .invalid(_, let invalid, _, _):
            invalid.tags?.albumTitle ?? invalid.sourceFolderName
        }
    }
}
