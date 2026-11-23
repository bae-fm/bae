import BaeKit
import SwiftUI

/// One triage row: cover, title, metadata, and status, with what is running
/// for the candidate read from the row itself — the list delivers the row
/// again when that changes. Every tab draws its rows with it: a Done row is
/// the row it was in Found, with its release's upload while files of it wait
/// to upload. Its height must not change on selection, which would shift
/// every row below.
struct MacImportRow: View {
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
    /// Done row's Show in library does, as a double-click on the row in the
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
    private var live: BridgeTriageListLiveState { row.live }

    var body: some View {
        rowContent
            .frame(minHeight: ImportCandidateTableLayout.height(for: row))
            .groupMemberRail(isGroupMember)
            .contentShape(Rectangle())
            .macContextMenu {
                if let onShowInLibrary {
                    MacMenuItem("Show in library", action: onShowInLibrary)
                    MacMenuItem.divider
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
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
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
            MacImportReleaseSummary(summary: summary, style: .sidebar) {
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
        switch live.status?.line {
        case .importing(let step):
            // A running import's bar moves by the second, so only this leaf
            // reads its percent.
            ImportProgressLineContainer(key: row.candidateKey, step: step)
                .themeText(.detail)
        case .uploading(let releaseId):
            DoneRowUploadLineContainer(releaseId: releaseId)
                .themeText(.detail)
        case .queuedToUpload:
            ImportRowStateText(text: BridgeUploadActivity.queued.phaseText)
        case .error(let failure):
            ImportRowStateText(text: failure.detail)
        case nil:
            EmptyView()
        }
    }

}

/// A Done row's bar while files of its release upload. Only this leaf
/// reads the release's upload stream.
private struct DoneRowUploadLineContainer: View {
    let releaseId: String
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        MacProgressLine(
            BridgeUploadActivity.uploading.phaseText,
            bar: StreamBar(
                stream: .upload(
                    outboxStore.uploadProgress(forRelease: releaseId)
                )
            )
        )
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
extension MacImportRow {
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
            else if case .queuedToImport = live.status {
                QueuedImportIcon()
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
            let waiting = String(localized: "Queued to identify")
            trailingIcon("clock", tint: Theme.textSecondary)
                .help(waiting)
                .accessibilityLabel(waiting)
        case .running:
            MacSpinner()
                .help(String(localized: "Identifying\u{2026}"))
        }
    }

    private func trailingIcon<S: ShapeStyle>(_ systemName: String, tint: S)
        -> some View
    {
        Image(systemName: systemName)
            .themeIcon(.small)
            .frame(
                width: QueuedImportIcon.box,
                height: QueuedImportIcon.box
            )
            .foregroundStyle(tint)
    }
}
