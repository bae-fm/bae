import BaeKit
import SwiftUI

// MARK: - ImportConfirmationView

/// Confirmation pane shown after the user picks a result. Renders the
/// import-specific chrome (cover affordance, library-status banner,
/// track-count mismatch warning, action button) wrapped around the
/// edit-metadata editor (`EditMetadataForm`). Source-derived (or
/// file-tag-derived for Unknown) values seed the editor; the user can
/// adjust album / track / pressing fields before commit.
///
/// On commit the caller passes the editor's current values to the
/// import command as the metadata overlay.
///
/// `trackCountMismatch` is the source-vs-local-files discrepancy
/// banner from the picked release detail. Always `false` for Unknown
/// imports — there's no source release to disagree with.
/// `expectedTrackCount` is shown alongside that banner; passed
/// regardless of the flag's value because the banner reads it.
struct ImportConfirmationView<CoverContent: View>: View {
    @Binding
    var values: BridgeRawReleaseEdit
    @Binding
    var storageManaged: Bool
    @Binding
    var storagePinned: Bool
    let trackCountMismatch: Bool
    let expectedTrackCount: UInt32
    let libraryStatus: BridgeLibraryStatus?
    /// The candidate this pane confirms — routes the high-frequency loudness
    /// ticks to the leaf bar during the measuring-loudness phase.
    let candidateKey: String
    let importStatus: BridgeCandidateImportStatus?
    /// Commit-time error written to the candidate (invalid edit shape, a
    /// failed `start_import` dispatch). Distinct from the
    /// `importStatus`-derived error, which the import pipeline emits once an
    /// import is under way.
    let error: String?
    let hasCoverOptions: Bool
    let importing: Bool
    /// What this import claims to hold and where its metadata came from, as
    /// bae-core derived it from the evidence. `nil` for an Unknown import,
    /// which claims nothing and has no source release to name.
    let claim: BridgeClaimLine?
    let onConfirmImport: () -> Void
    let onViewInLibrary: (String) -> Void
    let onEditCover: () -> Void
    @ViewBuilder
    let coverContent: () -> CoverContent

    @Environment(ConfigStore.self)
    private var configStore

    private var isComplete: Bool {
        if case .complete = importStatus {
            return true
        }
        return false
    }

    /// `Import` stays disabled when the editor is in an invalid state
    /// (bae-core can't shape the raw form into a savable edit) — the same
    /// rule the post-commit `Save` button uses.
    private var actionDisabled: Bool {
        if case .invalid = shapeReleaseEdit(raw: values) {
            return true
        }
        return false
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                headerCard
                statusBanners

                EditMetadataForm(form: $values)
            }
            .padding(20)
        }
    }

    /// The claim, stated under the album summary it qualifies. A pick from the
    /// results above it is what moves the claim, so the two read together —
    /// this is a statement of what the pick means, never a control.
    @ViewBuilder
    fileprivate var claimLine: some View {
        if let claim {
            ImportClaimLine(claim: claim)
        }
    }

    /// Title / artist / "format · year · N tracks" beside the cover. The
    /// fields are also editable below; this is the at-a-glance summary of
    /// what's about to be imported.
    private var albumSummary: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(values.albumTitle)
                .font(.system(size: 17, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.tail)
            Text(values.albumArtistText)
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Text(metaLine)
                .font(.system(size: 11.5))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Live "format · year · N tracks" summary from the editable values, so
    /// it tracks what the user is editing. Empty pressing fields drop out
    /// rather than leaving stray separators.
    private var metaLine: String {
        let count = values.tracks.count
        let trackText = String(localized: "\(count) tracks")
        return [values.pressing.format, values.pressing.year, trackText]
            .filter { !$0.isEmpty }
            .joined(separator: " · ")
    }

}

// MARK: - Header card and status banners

extension ImportConfirmationView {
    @ViewBuilder
    fileprivate var headerCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            headerRow
            claimLine
        }
        .padding(14)
        .background(Theme.surface)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(.white.opacity(0.07), lineWidth: 1)
        }
    }

    fileprivate var headerRow: some View {
        HStack(alignment: .top, spacing: 16) {
            coverContent()
                .overlay(alignment: .topTrailing) {
                    if !importing, !isComplete, hasCoverOptions {
                        Image(systemName: "pencil")
                            .font(.caption2)
                            .foregroundStyle(.white)
                            .padding(3)
                            .background(.black.opacity(0.5))
                            .clipShape(
                                RoundedRectangle(cornerRadius: 3)
                            )
                            .padding(2)
                    }
                }
                .onTapGesture {
                    if !importing, !isComplete, hasCoverOptions {
                        onEditCover()
                    }
                }

            albumSummary

            VStack(alignment: .trailing, spacing: 10) {
                ImportConfirmationCardAction(
                    importStatus: importStatus,
                    candidateKey: candidateKey,
                    actionDisabled: actionDisabled,
                    onConfirmImport: onConfirmImport,
                    onViewInLibrary: onViewInLibrary,
                )
                if !importing, !isComplete {
                    if configStore.config.hasCloudHome {
                        HStack(spacing: 10) {
                            ImportCheckboxToggle(
                                "Managed",
                                isOn: $storageManaged
                            )
                            if storageManaged {
                                ImportCheckboxToggle(
                                    "Keep local copy",
                                    isOn: $storagePinned
                                )
                            }
                        }
                        .fixedSize()
                    }
                }
            }
        }
    }

    fileprivate var statusBanners: some View {
        ImportConfirmationBanners(
            libraryStatus: libraryStatus,
            trackCountMismatch: trackCountMismatch,
            expectedTrackCount: expectedTrackCount,
            importStatus: importStatus,
            error: error,
            onViewInLibrary: onViewInLibrary,
        )
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Main Pane - Confirm") {
        @Previewable
        @State
        var values = rawReleaseEditFromUserEdit(
            edit: PreviewData.releaseSeedBridge,
            trackIdPrefix: "import-track"
        )
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportConfirmationView(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            trackCountMismatch: PreviewData.releaseDetailBridge
                .trackCountMismatch,
            expectedTrackCount: PreviewData.releaseDetailBridge.trackCount,
            libraryStatus: nil,
            candidateKey: "preview-candidate",
            importStatus: nil,
            error: nil,
            hasCoverOptions: false,
            importing: false,
            claim: BridgeClaimLine(
                choice: .exact(
                    releaseId: PreviewData.releaseDetailBridge.releaseId,
                    source: PreviewData.releaseDetailBridge.source
                ),
                evidence: .discIdAlone,
                release: "CD \u{00b7} 2004 \u{00b7} UK \u{00b7} CAT-1234",
                trackCount: PreviewData.releaseDetailBridge.trackCount,
                showsMetadataSource: false
            ),
            onConfirmImport: {},
            onViewInLibrary: { _ in },
            onEditCover: {},
            coverContent: {
                ZStack {
                    Theme.placeholder
                    Image(systemName: "photo")
                        .foregroundStyle(.tertiary)
                }
                .frame(width: 80, height: 80)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            },
        )
        .frame(width: 1212, height: 982)
        .windowBackground()
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(PreviewData.configStore)
    }
#endif
