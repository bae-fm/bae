import AppKit
import BaeKit
import SwiftUI

// MARK: - ImportCheckboxToggle

struct ImportCheckboxToggle: View {
    let title: LocalizedStringKey
    @Binding
    var isOn: Bool

    init(_ title: LocalizedStringKey, isOn: Binding<Bool>) {
        self.title = title
        _isOn = isOn
    }

    var body: some View {
        Toggle(title, isOn: $isOn)
            .toggleStyle(.checkbox)
            .font(.callout)
    }
}

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
    /// Exact / Metadata-only choice. `nil` for Unknown imports (no source
    /// pressing to claim exactly), which hides the toggle.
    let exactness: ImportExactnessChoice?
    let onConfirmImport: () -> Void
    let onViewInLibrary: (String) -> Void
    let onEditCover: () -> Void
    @ViewBuilder
    let coverContent: () -> CoverContent

    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(ConfigStore.self)
    private var configStore

    private var isComplete: Bool {
        if case .complete = importStatus {
            return true
        }
        return false
    }

    private var completedAlbumId: String? {
        if case .complete(releaseId: _, albumId: let albumId) = importStatus {
            return albumId
        }
        return nil
    }

    /// Cloud-upload progress for the imported release, while its files are
    /// still queued. "Imported" means the rows are committed and the uploads
    /// durably queued — this surfaces the remaining transfer instead of
    /// presenting a cloud-only import as fully landed.
    private var uploadProgress: BridgeUploadProgress? {
        guard
            case .complete(releaseId: let releaseId, albumId: _) = importStatus
        else {
            return nil
        }
        return outboxStore.progress(forRelease: releaseId)
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

                if let exactness {
                    ImportAsToggle(
                        isMetadataOnly: exactness.isMetadataOnly,
                        onSelectExactness: exactness.onSelect,
                    )
                    if exactness.isMetadataOnly {
                        MetadataOnlyNote()
                    }
                }

                EditMetadataForm(
                    form: $values,
                    pressingFieldsDisabled: exactness?.isMetadataOnly == true,
                )
            }
            .padding(20)
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

    @ViewBuilder
    private var cardAction: some View {
        if isComplete {
            VStack(alignment: .trailing, spacing: 2) {
                if let progress = uploadProgress {
                    if progress.failed > 0 {
                        Label(
                            "Imported — upload failed, retrying",
                            systemImage: "exclamationmark.arrow.circlepath"
                        )
                        .foregroundStyle(.orange)
                        .font(.callout)
                        .help(
                            "The cloud upload failed and retries automatically. See the Storage Manager for details."
                        )
                    }
                    else {
                        VStack(alignment: .trailing, spacing: 4) {
                            Label(
                                "Imported — uploading to cloud",
                                systemImage: "icloud.and.arrow.up"
                            )
                            .foregroundStyle(.secondary)
                            .font(.callout)
                            ProgressView(value: progress.fraction)
                                .progressViewStyle(.linear)
                                .frame(width: 160)
                        }
                    }
                }
                else {
                    Label("Imported", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                        .font(.callout)
                }
                if let albumId = completedAlbumId {
                    Button("View in Library") { onViewInLibrary(albumId) }
                        .buttonStyle(.link)
                        .font(.callout)
                }
            }
        }
        else if let status = importStatus {
            switch status {
            case .importing(_, let step):
                if case .running(.measuringLoudness)? = step {
                    // The loudness pass is the long pole; show its live,
                    // determinate per-track bar (updated imperatively off the
                    // high-frequency signal) instead of an indeterminate spinner.
                    ImportLoudnessProgressRepresentable(key: candidateKey)
                        .frame(width: 200, height: 32)
                }
                else {
                    HStack(spacing: 6) {
                        ProgressView()
                            .controlSize(.small)
                        if let step {
                            Text(step.localizedText)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                    }
                }
            case .error:
                Button("Retry Import") { onConfirmImport() }
                    .buttonStyle(.borderedProminent)
            case .complete:
                EmptyView()
            }
        }
        else {
            Button("Import") { onConfirmImport() }
                .buttonStyle(.borderedProminent)
                .disabled(actionDisabled)
        }
    }
}

// MARK: - Header card and status banners

extension ImportConfirmationView {
    @ViewBuilder
    fileprivate var headerCard: some View {
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
                cardAction
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
        .padding(14)
        .background(Theme.surface)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(.white.opacity(0.07), lineWidth: 1)
        }
    }

    @ViewBuilder
    fileprivate var statusBanners: some View {
        if let libStatus = libraryStatus {
            if libStatus.releaseInLibrary {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text("This release is already in your library")
                        .font(.callout)
                        .foregroundStyle(.orange)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(Color.orange.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            else if libStatus.albumInLibrary {
                HStack(spacing: 8) {
                    Image(systemName: "info.circle.fill")
                        .foregroundStyle(.blue)
                    Text(
                        "Another release of this album is in your library"
                    )
                    .font(.callout)
                    Spacer()
                    if let albumId = libStatus.albumId {
                        Button("View in Library") {
                            onViewInLibrary(albumId)
                        }
                        .controlSize(.small)
                    }
                }
                .padding(10)
                .background(Color.blue.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
            }
        }

        if trackCountMismatch {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text(
                    "Track count mismatch: release has \(Int(expectedTrackCount)) tracks but local files don't match"
                )
                .font(.callout)
                .foregroundStyle(.orange)
            }
            .padding(10)
            .background(Color.orange.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }

        if case .error(let bridgeError) = importStatus,
            let displayed = DisplayError(bridgeError)
        {
            ErrorDetailDisclosure(error: displayed)
                .padding(10)
                .background(Color.red.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }

        if let error {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                Text(error)
                    .font(.callout)
                    .foregroundStyle(.red)
            }
            .padding(10)
            .background(Color.red.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 6))
        }
    }
}

// MARK: - CoverItem

struct CoverItem: Identifiable, Equatable {
    static func == (lhs: CoverItem, rhs: CoverItem) -> Bool {
        lhs.id == rhs.id
    }

    var id: BridgeCoverSelection { selection }
    var selection: BridgeCoverSelection { coverChoice.selection }
    var previewSource: ImageLoader.Source {
        ImageLoader.Source(bridge: coverChoice.previewSource)
    }

    var thumbnailSource: ImageLoader.Source {
        ImageLoader.Source(bridge: coverChoice.thumbnailSource)
    }

    let coverChoice: BridgeCoverChoice
    let label: String

    init(
        coverChoice: BridgeCoverChoice,
        label: String
    ) {
        self.coverChoice = coverChoice
        self.label = label
    }
}

// MARK: - CoverPickerView

/// A gallery-style picker for selecting cover art from remote and local sources.
/// Presented as a sheet from the import confirmation view.
struct CoverPickerView: View {
    let remoteCoverArts: [BridgeRemoteCover]
    let localArtwork: [BridgeArtworkFile]
    let selectedCover: BridgeCoverChoice?
    let onSelect: (BridgeCoverChoice) -> Void
    let onDone: () -> Void

    @State
    private var cursor: Cursor<CoverItem>?

    private var items: [CoverItem] {
        var result: [CoverItem] = []
        for cover in remoteCoverArts {
            result.append(
                CoverItem(
                    coverChoice: cover.coverChoice,
                    label: cover.label
                )
            )
        }
        for file in localArtwork {
            // Local artwork is collected from the candidate's scanned folder,
            // so it's always present on disk.
            result.append(
                CoverItem(
                    coverChoice: file.coverChoice,
                    label: file.file.name
                )
            )
        }
        return result
    }

    private var selectedItemId: BridgeCoverSelection? {
        selectedCover?.selection
    }

    private func rebuild() {
        cursor = Cursor(
            items: items,
            preferring: cursor?.current.id ?? selectedItemId
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Divider()

            if let cursor {
                // Large preview
                GeometryReader { geometry in
                    let previewHeight = geometry.size.height - 120
                    ZStack {
                        coverPreview(for: cursor.current)
                            .frame(
                                maxWidth: geometry.size.width - 40,
                                maxHeight: previewHeight
                            )
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                // Source label + count
                VStack(spacing: 8) {
                    Text(
                        "\(cursor.current.label) \u{2014} \(cursor.index + 1) of \(cursor.items.count)"
                    )
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                    // Thumbnail strip
                    if cursor.canCycle {
                        ThumbnailStrip(
                            cursor: cursor,
                            centered: false,
                            onSelect: { self.cursor?.select(id: $0) },
                            stroke: { item, isActive in
                                item.id == selectedItemId
                                    ? (Color.accentColor, 2)
                                    : isActive
                                        ? (Color.primary, 2) : (Color.clear, 0)
                            }
                        ) { item in
                            ImageView(
                                source: item.thumbnailSource,
                                pointSize: 56
                            )
                        }
                    }

                    // Use This Cover button
                    HStack {
                        Spacer()
                        Button("Use This Cover") {
                            onSelect(cursor.current.coverChoice)
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(cursor.current.id == selectedItemId)
                    }
                    .padding(.horizontal, 16)
                }
                .padding(.bottom, 12)
            }
            else {
                ContentUnavailableView(
                    "No cover art available",
                    systemImage: "photo",
                    description: Text("No remote or local artwork found"),
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Theme.background)
        .onAppear { rebuild() }
        .onChange(of: items) { _, _ in rebuild() }
        .onKeyPress(.leftArrow) {
            cursor?.goToPrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            cursor?.goToNext()
            return .handled
        }
        .onKeyPress(.escape) {
            onDone()
            return .handled
        }
    }

    @ViewBuilder
    private func coverPreview(for item: CoverItem) -> some View {
        ImageView(source: item.previewSource, contentMode: .fit, pointSize: 600)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .shadow(radius: 10)
    }

}

#if DEBUG
    // MARK: - Previews

    #Preview("Main Pane - Confirm") {
        @Previewable
        @State
        var values = rawReleaseEditFromUserEdit(
            edit: shapeUserEditForChoice(
                seed: PreviewData.releaseSeedBridge,
                choice: .exact(
                    releaseId: PreviewData.releaseDetailBridge.releaseId,
                    source: PreviewData.releaseDetailBridge.source,
                )
            ),
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
            exactness: ImportExactnessChoice(
                isMetadataOnly: false,
                onSelect: { _ in }
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
        .environment(
            ConfigStore(
                config: Config(
                    bridge: BridgeConfig(
                        libraryId: "lib-preview",
                        libraryName: "Preview Library",
                        libraryPath: "/preview",
                        encryptionKeyStored: false,
                        encryptionKeyFingerprint: nil,
                        pauseBetweenSides: false,
                        maxConcurrentUploads: 3,
                        maxConcurrentDownloads: 3,
                        showRemainingTime: false,
                        libraryFullWidth: false,
                        exportFilenameTokens: PreviewData
                            .exportFilenameTokens,
                        exportPresets: PreviewData.exportPresets,
                        defaultTrackExportSelection: .original,
                        defaultReleaseExportSelection: .original,
                        mcp: BridgeMcpConfig(enabled: false, port: 47777),
                        discogsTokenStatus: .notConfigured,
                        discogsUsable: false,
                        sync: nil
                    )
                ),
                syncReady: false
            )
        )
    }
#endif
