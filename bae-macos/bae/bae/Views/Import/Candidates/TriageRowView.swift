import BaeKit
import SwiftUI

/// One triage row: cover, title, metadata, and status, with what is running
/// for the candidate read from the row itself — the list delivers the row
/// again when that changes. Every tab draws its rows with it: a Done row is
/// the row it was in Found, with its release's upload while files of it wait
/// to upload. Its height must not change on selection, which would shift
/// every row below.
struct TriageRowView: View {
    /// The cover's edge in points; the sidebar preloads Pending's covers at
    /// this size, so it must match for the cached image to be used.
    static let coverPointSize = ThemeSize.rowArtwork

    let row: BridgeTriageRow
    let coverContent: ImageContent?
    let isGroupMember: Bool
    /// What the row's menu offers, in the same order as the selection's pane:
    /// the row's own actions, or the selection's when the row is part of a
    /// larger one. Asked when the menu opens, not when the row is drawn: the
    /// answer reads the selection, and a list draws its rows far more often
    /// than anyone opens a menu.
    let menuOffers: () -> CandidateActionMenu
    let onPerform: (ImportCandidateActionOffer) -> Void
    /// Open the release this device imported from the row's files: what a
    /// Done row's Show in Library does, as a double-click on the row in the
    /// list's table does. `nil` off Done.
    let onShowInLibrary: (() -> Void)?

    init(
        row: BridgeTriageRow,
        coverContent: ImageContent?,
        isGroupMember: Bool,
        menuOffers: @escaping () -> CandidateActionMenu = { .empty },
        onPerform: @escaping (ImportCandidateActionOffer) -> Void = { _ in },
        onShowInLibrary: (() -> Void)? = nil
    ) {
        self.row = row
        self.coverContent = coverContent
        self.isGroupMember = isGroupMember
        self.menuOffers = menuOffers
        self.onPerform = onPerform
        self.onShowInLibrary = onShowInLibrary
    }

    /// What is running for the candidate, and the state the row is in with it.
    private var live: BridgeCandidateLiveState { row.live }

    var body: some View {
        rowContent
            .groupMemberRail(isGroupMember)
            .contentShape(Rectangle())
            .contextMenu {
                if let onShowInLibrary {
                    Button("Show in Library", action: onShowInLibrary)
                    Divider()
                }
                CandidateActionMenuItems(
                    menu: menuOffers,
                    onPerform: onPerform
                )
            }
    }

    /// The matched release's cover, or a placeholder so every row's text
    /// lines up.
    private var rowContent: some View {
        ImportListRowLayout(coverContent: coverContent) {
            meta
        } trailing: {
            trailing
        }
    }

    // MARK: - Meta

    @ViewBuilder
    private var meta: some View {
        VStack(alignment: .leading, spacing: 0) {
            switch row.reading {
            case .unidentified:
                folderLine
            case .prefilled, .identified, .identifiedAlbum:
                releaseSummary
            }
            stateLine
        }
    }

    /// The saved draft's summary, which every reading but `unidentified` has.
    @ViewBuilder
    private var releaseSummary: some View {
        if let summary = ImportReleaseSummary(row: row) {
            ImportReleaseSummaryView(summary: summary, style: .sidebar) {
                RecordArrow(readFromRecord: row.reading.readFromRecord)
            }
        }
    }

    /// An unidentified row names its folder, as the main pane's heading does.
    private var folderLine: some View {
        ImportFolderNameLine(folderName: row.folderName) {
            RecordArrow(readFromRecord: row.reading.readFromRecord)
        }
    }

    @ViewBuilder
    private var stateLine: some View {
        switch live.import {
        case .running, .writing:
            // A running import updates by the second, so only this leaf
            // observes its progress.
            ImportProgressLine(key: row.candidateKey)
                .themeText(.detail)
        case .queued:
            EmptyView()
        case nil:
            if let release = row.importedRelease, let upload = row.upload {
                // The bar moves each display frame, so only this leaf reads
                // the release's upload stream.
                DoneRowUploadLine(
                    releaseId: release.releaseId,
                    progress: upload
                )
                .themeText(.detail)
            }
            else if let statusLine {
                Text(statusLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }

}

/// A Done row's line while files of its release wait to upload: the phase,
/// as the Storage Manager names it, and while any file is being sent a bar
/// of the bytes of the release's size stored or sent, which a native bar
/// reads from the release's upload stream each display frame. Files only
/// waiting draw no bar: nothing is moving them, and with no storage
/// connected nothing will until one is.
private struct DoneRowUploadLine: View {
    let releaseId: String
    let progress: BridgeUploadProgress
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        let phase = progress.heldActivity.phaseText
        if progress.uploading > 0 {
            ProgressLine(
                phase,
                bar: StreamBar(
                    stream: .upload(
                        outboxStore.uploadProgress(forRelease: releaseId)
                    )
                )
            )
        }
        else {
            Text(verbatim: phase)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }
}

extension BridgeTriageRow {
    /// The release this device imported from a Done row's files.
    var importedRelease: BridgeImportedRelease? {
        if case .complete(let release) = importStatus {
            release
        }
        else {
            nil
        }
    }
}

/// The row's status line and trailing column.
extension TriageRowView {
    /// The line under the release summary: how bae broke for a row in
    /// error, or a failed import.
    private var statusLine: String? {
        if case .error(let failure) = live.standing {
            return failure.detail
        }
        switch row.placement {
        case .pending, .skipped:
            return nil
        case .failed, .done:
            return importStatusLine
        }
    }

    private var importStatusLine: String? {
        switch row.importStatus {
        case .complete, nil:
            return nil
        case .error(let error):
            return error.localizedLine
        }
    }

    // MARK: - Trailing

    /// The trailing column, kept at its ideal width so the title truncates
    /// first: the badge of a row waiting on the person, or what is running
    /// for the candidate.
    private var trailing: some View {
        Group {
            if let badge = live.badge {
                RowBadgeChip(badge: badge)
            }
            else if let step = live.identifying {
                identifyingTrailing(step)
            }
            else if let standing = live.import {
                importStandingTrailing(standing)
            }
        }
        .fixedSize()
    }

    /// The lookup at work, which only a row Identifying shows.
    @ViewBuilder
    private func identifyingTrailing(
        _ step: BridgeIdentifyingStep
    ) -> some View {
        switch step {
        case .queued:
            trailingIcon("clock", tint: .secondary)
                .help(String(localized: "Waiting to be identified"))
        case .running:
            ProgressView()
                .controlSize(.small)
                .help(String(localized: "Identifying\u{2026}"))
        }
    }

    /// A queued import's clock; a running one's progress is its state line.
    @ViewBuilder
    private func importStandingTrailing(
        _ standing: BridgeImportStanding
    ) -> some View {
        switch standing {
        case .queued:
            QueuedImportIcon()
        case .running, .writing:
            EmptyView()
        }
    }

    private func trailingIcon<S: ShapeStyle>(_ systemName: String, tint: S)
        -> some View
    {
        Image(systemName: systemName)
            .themeIcon(.small)
            .foregroundStyle(tint)
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Triage rows") {
        let importStore = ImportStore()
        VStack(alignment: .leading, spacing: 0) {
            TriageRowView(
                row: PreviewData.triageRowUnidentified,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowUnidentified
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowPrefilledFromTags,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPrefilledFromTags
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifiedOnline,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifiedOnline
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifiedSeveralMatches,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifiedSeveralMatches
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentified,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentified
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowPickAPressing,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPickAPressing
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowSeveralMatchesFromSignals,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSeveralMatchesFromSignals
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowAlreadyInLibrary,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowAlreadyInLibrary
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowNoMatch,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowNoMatch
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifying,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifying
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowFailed,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowFailed
                ),
                isGroupMember: false
            )
        }
        .padding()
        .frame(width: 340)
        .environment(PreviewData.artImageStore())
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }

    // A Done row is the row it was in Found.
    #Preview("Done rows") {
        let importStore = ImportStore()
        let rows = [
            PreviewData.triageRowDoneImported,
            PreviewData.triageRowDoneFromTags,
        ]
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(rows, id: \.candidateKey) { row in
                TriageRowView(
                    row: row,
                    coverContent: importStore.sidebarCover(for: row),
                    isGroupMember: false,
                    onShowInLibrary: {}
                )
            }
        }
        .padding()
        .frame(width: 340)
        .environment(PreviewData.artImageStore())
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }

    // A Done row's line while its release uploads: the phase and a bar
    // while a file is being sent, the phase alone while its files wait.
    #Preview("Done rows uploading") {
        PreviewScenes.importDoneRowsUploading()
    }

    #Preview("Where the draft was read from") {
        let importStore = ImportStore()
        let rows = [
            PreviewData.triageRowReadFromRecord,
            PreviewData.triageRowNotReadFromRecord,
        ]
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(rows, id: \.candidateKey) { row in
                TriageRowView(
                    row: row,
                    coverContent: importStore.sidebarCover(for: row),
                    isGroupMember: false
                )
            }
            // The same row as the list's first, drawn as the selection draws
            // it: the whole text column goes white.
            TriageRowView(
                row: PreviewData.triageRowReadFromRecord,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowReadFromRecord
                ),
                isGroupMember: false
            )
            .environment(\.backgroundProminence, .increased)
            .background(Color.accentColor)
        }
        .padding()
        .frame(width: 340)
        .environment(PreviewData.artImageStore())
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }
#endif
