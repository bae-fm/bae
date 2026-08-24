import BaeKit
import SwiftUI

/// The one surface a folder becomes a release on: the folder it is about, two
/// sections and a commit bar. Section one is where the folder's metadata comes
/// from; section two is every source unit it offers alongside the track
/// committing makes of it.
///
/// There is no identify⇄confirm layout flip. The table is the same table before
/// and after a release is picked — picking one fills its BECOMES column in
/// place, and the identity control switches between the release and the
/// folder's own tags without emptying it.
struct ImportMappingPane: View {
    let candidate: Candidate
    /// What is in flight for this candidate: the run whose state the pane
    /// shows while one is live, and how far a running import has got. `nil`
    /// when nothing is running for it.
    let runtime: BridgeCandidateRuntimeSnapshot?
    /// What each track sheet may be bound to, by the sheet's file id.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The path currently auditioning, if any.
    let previewingPath: String?
    let libraryStatus: BridgeLibraryStatus?
    let hasCoverOptions: Bool
    let coverContent: ImageContent?
    /// Where an album-level field's typed value goes: a row under this
    /// candidate, written as the field is left.
    let editActions: ReleaseFieldWriter
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    let mappingActions: ImportMappingActions
    let commitActions: ImportCommitActions
    let onSetIdentity: (ImportIdentity) -> Void
    /// Read the folder this candidate came out of the other way, which rewrites
    /// the folder's decision as the user's and rescans.
    let onReleaseDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void
    let onFindRelease: () -> Void
    /// Pick one of identification's matched pressings from the inline options
    /// — the same pick a search-sheet row click runs.
    let onPickRelease: (BridgeMetadataResult) -> Void
    let onEditCover: () -> Void

    /// The folder's mapping, as core reads it back for this candidate.
    private var mapping: BridgeMappingTable {
        candidate.mapping
    }

    /// The run in flight while there is one, else the state the stored verdict
    /// stands back up as.
    private var identifyState: IdentifyState {
        shownIdentifyState(
            resumed: candidate.resumedIdentifyState,
            runtime: runtime
        )
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                CandidateFolderLine(
                    folderName: candidate.displayName,
                    folderPath: candidate.key,
                    formatLabel: candidate.files.formatLabel
                )
                ForEach(
                    candidate.row?.resolvedBoundaries ?? [],
                    id: \.key
                ) { boundary in
                    FolderReadingControl(
                        boundary: boundary,
                        onDecision: onReleaseDecision
                    )
                }
                identitySection
                banners
                if !mapping.images.isEmpty {
                    ImportMappingGallery(
                        images: mapping.images,
                        actions: mappingActions
                    )
                }
                ImportMappingTable(
                    table: mapping,
                    bindingOptions: bindingOptions,
                    previewingPath: previewingPath,
                    unprobed: Set(candidate.detail?.unprobed ?? []),
                    actions: mappingActions,
                )
            }
            .padding(20)
        }
    }

    /// The card's commit row — present exactly when there is something to
    /// commit, which is the precondition the commit itself reads: a failed
    /// re-pick leaves the table and the album fields in place but nothing
    /// settled to commit them under.
    private var commitControls: ImportCommitControls? {
        guard candidate.hasSettled else {
            return nil
        }
        return ImportCommitControls(
            unansweredCount: mapping.unansweredCount,
            candidateKey: candidate.key,
            importStatus: candidate.row?.importStatus,
            importInFlight: runtime?.import,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned,
            actions: commitActions,
        )
    }

    private var identitySection: some View {
        ImportIdentitySection(
            identity: candidate.identity,
            title: headerTitle,
            artist: candidate.edit?.albumArtistText ?? "",
            metaLine: headerMetaLine,
            evidence: candidate.evidence,
            hasPick: candidate.pickedRelease != nil,
            pickedSource: candidate.pickedRelease?.source,
            isReading: candidate.pickInFlight,
            coverContent: coverContent,
            hasCoverOptions: hasCoverOptions,
            editValues: candidate.edit,
            editActions: editActions,
            matchOptions: matchOptions,
            hasSettled: candidate.hasSettled,
            commit: commitControls,
            onSetIdentity: onSetIdentity,
            onFindRelease: onFindRelease,
            onEditCover: onEditCover,
        )
    }

    /// Identification's matches, offered inline while the pick is still open:
    /// the folder reading as a release, a `Found` state to offer, and no
    /// settled identity. They stay up through the pick's own read — the
    /// clicked row carries the spinner and the list stays put — and hand over
    /// to the release card only when the read lands and settles the identity.
    private var matchOptions: ImportMatchOptions? {
        guard candidate.identity == .release,
            !candidate.hasSettled,
            case .found(let groups, let libraryStatuses, _, let provenance) =
                identifyState
        else {
            return nil
        }
        return ImportMatchOptions(
            groups: groups,
            libraryStatuses: libraryStatuses,
            provenance: provenance,
            isImporting: ImportSearchFlow.isImporting(candidate),
            loadingReleaseId: nil,
            onSelect: onPickRelease,
        )
    }

    /// The album title the card leads with: what the editor holds once there is
    /// one, and the folder's own name before that.
    private var headerTitle: String {
        let title = candidate.edit?.albumTitle ?? ""
        return title.isEmpty ? candidate.displayName : title
    }

    /// "CD · 1996 · XE · 463360 2 · 10 tracks" from the live editor and the
    /// live table, so it tracks what is being edited. Empty pressing fields
    /// drop out rather than leaving stray separators, and reading the folder as
    /// Unknown says so where a pressing would be.
    private var headerMetaLine: String {
        guard let values = candidate.edit else {
            return candidate.files.formatLabel
        }
        let count = mapping.willWriteCount
        let trackText = String(localized: "\(count) tracks")
        let lead =
            candidate.identity == .unknown
            ? [coreString("ui.import.metadata.from_file_tags")]
            : [
                values.pressing.format,
                values.pressing.year,
                values.pressing.country,
                values.pressing.catalogNumber,
            ]
        return (lead + [trackText])
            .filter { !$0.isEmpty }
            .joined(separator: " \u{00b7} ")
    }

    private var banners: some View {
        ImportConfirmationBanners(
            libraryStatus: libraryStatus,
            importStatus: candidate.row?.importStatus,
            error: candidate.error,
            failure: candidate.failure,
            onRetry: commitActions.confirmImport,
            onViewInLibrary: commitActions.viewInLibrary,
        )
    }
}
