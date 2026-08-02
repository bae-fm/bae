import BaeKit
import SwiftUI

/// The one surface a folder becomes a release on: two sections and a commit
/// bar. Section one is what the folder is being read as; section two is every
/// source unit it offers alongside the track committing makes of it.
///
/// There is no identify⇄confirm layout flip. The table is the same table before
/// and after a release is picked — picking one fills its BECOMES column in
/// place, and the identity control switches between the release and the
/// folder's own tags without emptying it.
struct ImportMappingPane: View {
    let candidate: Candidate
    /// What each track sheet may be bound to, by the sheet's file id.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The path currently auditioning, if any.
    let previewingPath: String?
    let libraryStatus: BridgeLibraryStatus?
    let hasCoverOptions: Bool
    let coverContent: ImageContent?
    /// The album-level fields. `nil` while nothing has been settled for this
    /// folder — there is nothing to edit or commit then, so the commit bar
    /// stays off the pane.
    let editor: Binding<BridgeRawReleaseEdit>?
    @Binding
    var storageManaged: Bool
    @Binding
    var storagePinned: Bool
    let mappingActions: ImportMappingActions
    let commitActions: ImportCommitActions
    let onSetIdentity: (ImportIdentity) -> Void
    let onFindRelease: () -> Void
    let onEditCover: () -> Void

    /// The folder's mapping, or an empty table while the first read is still in
    /// flight — the pane's own shape does not change for it.
    private var mapping: BridgeMappingTable {
        candidate.mapping ?? BridgeMappingTable(rows: [], reconciliation: nil)
    }

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    identitySection
                    banners
                    ImportMappingTable(
                        table: mapping,
                        bindingOptions: bindingOptions,
                        previewingPath: previewingPath,
                        actions: mappingActions,
                    )
                }
                .padding(20)
            }
            // Shown exactly when there is something to commit, which is the
            // precondition the commit itself reads — a failed re-pick leaves
            // the table and the album fields in place but nothing settled to
            // commit them under.
            if candidate.commitEdit != nil {
                ImportCommitBar(
                    willWriteCount: mapping.willWriteCount,
                    unansweredCount: mapping.unansweredCount,
                    candidateKey: candidate.key,
                    importStatus: candidate.importStatus,
                    storageManaged: $storageManaged,
                    storagePinned: $storagePinned,
                    actions: commitActions,
                )
            }
        }
    }

    private var identitySection: some View {
        ImportIdentitySection(
            identity: candidate.identity,
            title: headerTitle,
            artist: editor?.wrappedValue.albumArtistText ?? "",
            metaLine: headerMetaLine,
            claim: candidate.claim,
            hasPick: candidate.pick != nil,
            isReading: candidate.prefetchTask != nil,
            coverContent: coverContent,
            hasCoverOptions: hasCoverOptions,
            editor: editor,
            onSetIdentity: onSetIdentity,
            onFindRelease: onFindRelease,
            onEditCover: onEditCover,
        )
    }

    /// The album title the card leads with: what the editor holds once there is
    /// one, and the folder's own name before that.
    private var headerTitle: String {
        let title = editor?.wrappedValue.albumTitle ?? ""
        return title.isEmpty ? candidate.displayName : title
    }

    /// "CD · 1996 · 9 tracks" from the live editor and the live table, so it
    /// tracks what is being edited. Empty pressing fields drop out rather than
    /// leaving stray separators, and reading the folder as Unknown says so
    /// where a pressing would be.
    private var headerMetaLine: String {
        guard let values = editor?.wrappedValue else {
            return candidate.files.formatLabel
        }
        let count = mapping.willWriteCount
        let trackText = String(localized: "\(count) tracks")
        let lead =
            candidate.identity == .unknown
            ? [coreString("ui.import.identity.from_file_tags")]
            : [values.pressing.format, values.pressing.year]
        return (lead + [trackText])
            .filter { !$0.isEmpty }
            .joined(separator: " \u{00b7} ")
    }

    private var banners: some View {
        ImportConfirmationBanners(
            libraryStatus: libraryStatus,
            importStatus: candidate.importStatus,
            error: candidate.error,
            onViewInLibrary: commitActions.viewInLibrary,
        )
    }
}
