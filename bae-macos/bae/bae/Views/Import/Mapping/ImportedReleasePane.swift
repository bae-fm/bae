import BaeKit
import SwiftUI

/// A completed import: the saved release's editable metadata and files, with
/// the source folder's images as its scan read them. It cannot change the
/// candidate, which holds no draft once imported.
struct ImportedReleasePane: View {
    let candidate: Candidate
    let releaseId: String
    let albumId: String
    let editCommands: ReleaseEditCommands
    let onViewInLibrary: (String) -> Void
    let onOpenImages: ([BridgeMappingImage], String) -> Void
    let onPlayTrack: (String) -> Void

    /// The height the loading and failed states hold.
    private static let placeholderHeight: CGFloat = 280

    /// The folder's images, as its scan read them: the candidate holds no
    /// draft once imported, so no mapping table.
    private var images: [BridgeMappingImage] {
        candidate.importedImages ?? []
    }

    @State
    private var session: ReleaseMetadataEditSession?
    @State
    private var loadError: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            CandidateFolderLine(
                tab: candidate.tab,
                folderName: candidate.displayName,
                folderPaths: candidate.sourceFolderPaths,
                onNavigateToPlacement: {}
            )
            if let session {
                importedLine
                ReleaseEditorView(
                    session: session,
                    displayName: candidate.displayName,
                    onViewAlbum: onViewInLibrary,
                    onPlayTrack: onPlayTrack,
                    images: images,
                    onOpenImages: onOpenImages,
                    onSaved: { _ in }
                )
            }
            else if let loadError {
                Notice(
                    error: loadError,
                    look: .centered,
                    action: loadError.retryAction {
                        Task { await load() }
                    }
                )
                .frame(
                    maxWidth: .infinity,
                    minHeight: Self.placeholderHeight
                )
            }
            else {
                // Opening the release's edit session reads the library;
                // nothing shows until it lands.
                Color.clear
                    .frame(
                        maxWidth: .infinity,
                        minHeight: Self.placeholderHeight
                    )
            }
        }
        .padding(.horizontal, ThemeSpace.section)
        .padding(.top, ThemeSpace.section)
        .padding(.bottom, ThemeSpace.page)
        .task(id: releaseId) { await load() }
        .onDisappear {
            session?.cancelTasks()
        }
    }

    private var importedLine: some View {
        Notice(
            title: String(localized: "Imported"),
            advice: nil,
            error: nil,
            tone: .success,
            action: ErrorAction("View in library") {
                onViewInLibrary(albumId)
            }
        )
    }
}

extension ImportedReleasePane {
    @MainActor
    private func load() async {
        session?.cancelTasks()
        session = nil
        loadError = nil
        do {
            let opened = try await editCommands.open(releaseId)
            session = ReleaseMetadataEditSession(
                releaseId: releaseId,
                opened: opened,
                commands: editCommands
            )
        }
        catch is CancellationError {}
        catch {
            loadError = DisplayError(error)
        }
    }
}

/// A candidate being imported, shown read-only while the import runs.
struct ImportingCandidatePane: View {
    let candidate: Candidate
    let step: BridgeImportStep
    let coverContent: ImageContent?
    let onOpenImages: ([BridgeMappingImage], String) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPreview: (BridgePreviewTarget) -> Void
    let onStopPreview: () -> Void
    let previewingTarget: BridgePreviewTarget?

    private static let coverSize: CGFloat = 200

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                CandidateFolderLine(
                    tab: candidate.tab,
                    folderName: candidate.displayName,
                    folderPaths: candidate.sourceFolderPaths,
                    onNavigateToPlacement: {}
                )
                switch step {
                case .preparing(step: .queued):
                    Label(
                        String(localized: "Waiting to import"),
                        systemImage: "clock"
                    )
                    .foregroundStyle(.secondary)
                case .preparing, .running:
                    // Reads the percent itself, so a percent tick redraws
                    // the line and not the pane.
                    ImportProgressContainer(key: candidate.key, step: step)
                }
                HStack(alignment: .top, spacing: ThemeSpace.edge) {
                    ImageView(content: coverContent, pointSize: Self.coverSize)
                        .frame(width: Self.coverSize, height: Self.coverSize)
                        .clipShape(
                            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        )
                    if let values = candidate.edit {
                        ImportReleaseSummaryView(
                            summary: ImportReleaseSummary(
                                candidate: candidate,
                                editValues: values
                            ),
                            style: .card
                        )
                    }
                }
                if !candidate.mapping.images.isEmpty {
                    MacImportImages(
                        images: candidate.mapping.images,
                        onOpen: onOpenImages
                    )
                }
                ReadOnlyCandidateMappingTable(
                    table: candidate.mapping,
                    previewingTarget: previewingTarget,
                    onOpenDocument: onOpenDocument,
                    onPreview: onPreview,
                    onStopPreview: onStopPreview
                )
            }
            .padding(.horizontal, ThemeSpace.section)
            .padding(.top, ThemeSpace.section)
            .padding(.bottom, ThemeSpace.page)
        }
    }
}

private struct ReadOnlyCandidateMappingTable: View {
    let table: BridgeMappingTable
    let previewingTarget: BridgePreviewTarget?
    let onOpenDocument: (String, String) -> Void
    let onPreview: (BridgePreviewTarget) -> Void
    let onStopPreview: () -> Void

    @State
    private var availableWidth = ReleaseMetadataTrackColumns.minimumTableWidth

    private var tableWidth: CGFloat {
        max(availableWidth, ReleaseMetadataTrackColumns.minimumTableWidth)
    }

    private var columns: ReleaseMetadataTrackColumns {
        .resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.edge) {
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    header
                    ForEach(
                        Array(table.trackSides.enumerated()),
                        id: \.offset
                    ) {
                        _,
                        side in
                        if !side.sideHeaderText.isEmpty {
                            Eyebrow(verbatim: side.sideHeaderText)
                                .frame(width: tableWidth, alignment: .leading)
                                .padding(.top, ThemeSpace.group)
                        }
                        ForEach(Array(side.runs.enumerated()), id: \.offset) {
                            _,
                            run in
                            if case .sheet(let sheet, _) = run {
                                sourceCaption(sheet.name)
                            }
                            rows(run)
                        }
                    }
                }
                .frame(width: tableWidth, alignment: .leading)
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            .onGeometryChange(for: CGFloat.self) {
                $0.size.width
            } action: {
                availableWidth = $0
            }
            if !table.files.isEmpty {
                ReleaseFilesView(
                    rows: table.files,
                    onOpenDocument: onOpenDocument
                )
            }
        }
    }

    private var header: some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            Eyebrow("Source")
                .frame(width: columns.source, alignment: .leading)
            Eyebrow("Track")
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            Eyebrow("Title")
                .frame(width: columns.title, alignment: .leading)
            Eyebrow("Artist")
                .frame(width: columns.artist, alignment: .leading)
            Eyebrow(
                verbatim: localizedCoreString("ui.import.slots.column.length")
            )
            .frame(
                width: ReleaseMetadataTrackColumns.length,
                alignment: .leading
            )
        }
        .padding(.vertical, ThemeSpace.compact)
    }

    @ViewBuilder
    private func rows(_ run: BridgeMappingTrackRun) -> some View {
        switch run {
        case .tracks(let mappings):
            ForEach(mappings, id: \.rowId, content: row)
        case .sheet(_, let entries):
            ForEach(entries, id: \.rowId, content: row)
        }
    }

    private func row(_ mapping: BridgeTrackMapping) -> some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            readOnlySource(mapping.source)
                .frame(width: columns.source, alignment: .leading)
            Text(verbatim: mapping.position)
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            Text(mapping.track.title)
                .frame(width: columns.title, alignment: .leading)
                .lineLimit(1)
            Text(trackArtistText(mapping.track.artistAssignments))
                .frame(width: columns.artist, alignment: .leading)
                .lineLimit(1)
            Text(mapping.displayedDuration)
                .monospacedDigit()
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .leading
                )
        }
        .themeText(.body)
        .padding(.vertical, ThemeSpace.related)
        .overlay(alignment: .top) {
            Rectangle().fill(Theme.hairline).frame(height: 1)
        }
    }

    private func readOnlySource(_ source: BridgeMappingSource) -> some View {
        HStack(spacing: ThemeSpace.compact) {
            if let target = source.previewTarget {
                Button {
                    target == previewingTarget
                        ? onStopPreview() : onPreview(target)
                } label: {
                    Image(
                        systemName: target == previewingTarget
                            ? "stop.fill" : "play.fill"
                    )
                }
                .buttonStyle(.plain)
            }
            Text(sourceName(source))
                .themeText(.mono)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

    private func sourceName(_ source: BridgeMappingSource) -> String {
        switch source {
        case .file(let file): file.name
        case .sheetEntry(let entry): entry.title ?? entry.containerName
        }
    }

    private func trackArtistText(
        _ assignments: BridgeTrackArtistAssignments
    ) -> String {
        switch assignments {
        case .albumArtists: String(localized: "Album artist")
        case .explicit(let artists): bridgeCreditLine(credits: artists)
        }
    }

    private func sourceCaption(_ name: String) -> some View {
        Label(name, systemImage: "list.bullet.rectangle")
            .themeText(.mono)
            .foregroundStyle(.secondary)
            .frame(width: tableWidth, alignment: .leading)
            .padding(.vertical, ThemeSpace.compact)
    }
}
