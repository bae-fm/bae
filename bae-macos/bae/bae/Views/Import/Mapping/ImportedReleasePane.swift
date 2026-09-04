import BaeKit
import SwiftUI

/// A completed import rendered from the persisted release edit projection and
/// the source-folder evidence that produced it. It has no candidate mutation
/// callbacks.
struct ImportedReleasePane: View {
    let candidate: Candidate
    let releaseId: String
    let albumId: String
    let seedReleaseEdit:
        @Sendable (String) async throws -> BridgeReleaseEditSeed
    let saveReleaseEdit:
        @Sendable (String, BridgeReleaseUserEdit) async throws -> Void
    let resetReleaseEdit:
        @Sendable (String) async throws -> BridgeRawReleaseEdit
    let changeCover:
        @Sendable (String, BridgeCoverSelection) async throws -> Void
    let fetchRemoteCovers:
        @Sendable (String) async throws -> [BridgeRemoteCover]
    let onViewInLibrary: (String) -> Void
    let onOpenImages: ([BridgeMappingImage], String) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPlayTrack: (Int) -> Void

    @State
    private var session: ReleaseMetadataEditSession?
    @State
    private var loadError: String?
    @State
    private var coverChangeTask: Task<Void, Never>?

    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                CandidateFolderLine(
                    placement: candidate.row?.placement,
                    folderName: candidate.displayName,
                    folderPath: candidate.key,
                    onNavigateToPlacement: {}
                )
                if let session {
                    actionBar(session)
                    ReleaseMetadataEditorContent(
                        session: session,
                        onEditCover: { presentCoverPicker(session) },
                        onPlayTrack: onPlayTrack
                    )
                    if !candidate.mapping.images.isEmpty {
                        imagesSection
                    }
                    if !candidate.mapping.files.isEmpty {
                        filesSection
                    }
                }
                else if let loadError {
                    LoadFailureView(line: loadError) {
                        Task { await load() }
                    }
                    .frame(maxWidth: .infinity, minHeight: 280)
                }
                else {
                    ProgressView("Loading")
                        .frame(maxWidth: .infinity, minHeight: 280)
                }
            }
            .padding(.horizontal, 24)
            .padding(.top, 20)
            .padding(.bottom, 32)
        }
        .task(id: releaseId) { await load() }
        .onDisappear {
            session?.cancelTasks()
            coverChangeTask?.cancel()
        }
    }

    private func actionBar(_ session: ReleaseMetadataEditSession) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                Label("Imported", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
                Button("View in Library") { onViewInLibrary(albumId) }
                    .buttonStyle(.link)
                Spacer(minLength: 12)
                Button("Reset to Source") { session.resetToSource() }
                    .opacity(session.canResetToSource ? 1 : 0)
                    .allowsHitTesting(session.canResetToSource)
                    .disabled(session.isBusy)
                Button("Cancel") { session.cancelChanges() }
                    .opacity(session.hasChanges ? 1 : 0)
                    .allowsHitTesting(session.hasChanges)
                    .disabled(session.isBusy)
                ProgressView()
                    .controlSize(.small)
                    .opacity(session.isBusy ? 1 : 0)
                Button("Save") {
                    session.save(onSuccess: {})
                }
                .buttonStyle(.borderedProminent)
                .disabled(session.isBusy)
            }
            if let message = session.validationMessage
                ?? session.failureMessage
            {
                Label(message, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
    }

    private var imagesSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            FormSectionHeader(title: String(localized: "Images"), ruled: true)
            ImportMappingGallery(
                images: candidate.mapping.images,
                evidence: candidate.fileEvidence,
                onOpen: onOpenImages
            )
        }
    }

    private var filesSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            FormSectionHeader(title: String(localized: "Files"), ruled: true)
                .padding(.bottom, 6)
            ForEach(candidate.mapping.files, id: \.rowId) { row in
                completedFileRow(row)
                    .padding(.horizontal, 2)
                    .padding(.vertical, 10)
                    .overlay(alignment: .top) {
                        Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
                    }
            }
        }
    }

    @ViewBuilder
    private func completedFileRow(_ row: BridgeMappingFileRow) -> some View {
        switch row {
        case .file(let file):
            if file.role.fileRole.isDocument {
                Button {
                    onOpenDocument(file.name, file.localPath)
                } label: {
                    completedFileLine(name: file.name, size: file.sizeText)
                }
                .buttonStyle(.plain)
            }
            else {
                completedFileLine(name: file.name, size: file.sizeText)
            }
        case .sheet(let sheet):
            Button {
                onOpenDocument(sheet.name, sheet.localPath)
            } label: {
                HStack(spacing: 12) {
                    Text(sheet.name)
                        .font(.system(size: 12, design: .monospaced))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                    Text(Int64(sheet.size).formatted(.byteCount(style: .file)))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    private func completedFileLine(name: String, size: String) -> some View {
        HStack(spacing: 12) {
            Text(name)
                .font(.system(size: 12, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
            Text(size)
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
    }
}

extension ImportedReleasePane {
    private func presentCoverPicker(_ session: ReleaseMetadataEditSession) {
        uiStore.presentModal {
            CoverSheetView(
                releaseImages: candidate.mapping.images.map {
                    ReleaseImageOption(
                        id: $0.fileId,
                        name: $0.name,
                        path: $0.localPath
                    )
                },
                fetchRemoteCovers: {
                    try await fetchRemoteCovers(releaseId)
                },
                onSelectRemote: {
                    applyCover($0.coverChoice.selection, to: session)
                },
                onSelectReleaseImage: {
                    applyCover(.releaseImage(fileId: $0), to: session)
                },
                onDone: { uiStore.dismissModal() }
            )
            .frame(width: 500, height: 450)
            .background(Theme.background)
        }
    }

    private func applyCover(
        _ selection: BridgeCoverSelection,
        to session: ReleaseMetadataEditSession
    ) {
        coverChangeTask?.cancel()
        coverChangeTask = Task { @MainActor in
            do {
                try await changeCover(releaseId, selection)
                uiStore.dismissModal()
            }
            catch is CancellationError {}
            catch {
                if let line = error.displayLine {
                    uiStore.showError(
                        String(localized: "Couldn't change the cover: \(line)")
                    )
                }
                return
            }
            do {
                let refreshed = try await seedReleaseEdit(releaseId)
                session.updateCover(refreshed.cover)
            }
            catch is CancellationError {}
            catch {
                if let line = error.displayLine {
                    uiStore.showError(
                        "\(String(localized: "Couldn't load image")): \(line)"
                    )
                }
            }
        }
    }

    @MainActor
    private func load() async {
        session?.cancelTasks()
        session = nil
        loadError = nil
        do {
            let seed = try await seedReleaseEdit(releaseId)
            session = ReleaseMetadataEditSession(
                releaseId: releaseId,
                seed: seed,
                save: saveReleaseEdit,
                reset: resetReleaseEdit
            )
        }
        catch is CancellationError {}
        catch {
            loadError = error.displayLine
        }
    }
}

/// A claimed candidate remains visible while import runs, but nothing in this
/// tree can revise the captured candidate revision.
struct ImportingCandidatePane: View {
    let candidate: Candidate
    let runtime: BridgeCandidateRuntimeSnapshot?
    let coverContent: ImageContent?
    let onOpenImages: ([BridgeMappingImage], String) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPreview: (BridgePreviewTarget) -> Void
    let onStopPreview: () -> Void
    let previewingTarget: BridgePreviewTarget?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                CandidateFolderLine(
                    placement: candidate.row?.placement,
                    folderName: candidate.displayName,
                    folderPath: candidate.key,
                    onNavigateToPlacement: {}
                )
                ProgressLine(
                    runtime?.import?.step?.localizedText
                        ?? String(localized: "Importing\u{2026}"),
                    progress: runtime?.import?.progressPercent
                        .map {
                            Double($0) / 100
                        }
                )
                HStack(alignment: .top, spacing: 24) {
                    ImageView(content: coverContent, pointSize: 200)
                        .frame(width: 200, height: 200)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
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
                    VStack(alignment: .leading, spacing: 10) {
                        FormSectionHeader(
                            title: String(localized: "Images"),
                            ruled: true
                        )
                        ImportMappingGallery(
                            images: candidate.mapping.images,
                            evidence: candidate.fileEvidence,
                            onOpen: onOpenImages
                        )
                    }
                }
                ReadOnlyCandidateMappingTable(
                    table: candidate.mapping,
                    previewingTarget: previewingTarget,
                    onOpenDocument: onOpenDocument,
                    onPreview: onPreview,
                    onStopPreview: onStopPreview
                )
            }
            .padding(.horizontal, 24)
            .padding(.top, 20)
            .padding(.bottom, 32)
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
        VStack(alignment: .leading, spacing: 18) {
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    header
                    ForEach(
                        Array(table.trackSections.enumerated()),
                        id: \.offset
                    ) {
                        _,
                        section in
                        if !section.sideHeaderText.isEmpty {
                            Text(verbatim: section.sideHeaderText)
                                .font(.system(size: 10, weight: .bold))
                                .tracking(1.2)
                                .textCase(.uppercase)
                                .foregroundStyle(.secondary)
                                .frame(width: tableWidth, alignment: .leading)
                                .padding(.top, 12)
                        }
                        if case .sheet(let sheet, _) = section.content {
                            sourceCaption(sheet.name)
                        }
                        rows(section)
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
                readOnlyFiles
            }
        }
    }

    private var header: some View {
        HStack(spacing: ReleaseMetadataTrackColumns.spacing) {
            FormEyebrow(text: Text("Source"))
                .frame(width: columns.source, alignment: .leading)
            FormEyebrow(text: Text("Track"))
                .frame(width: ReleaseMetadataTrackColumns.track)
            FormEyebrow(text: Text("Title"))
                .frame(width: columns.title, alignment: .leading)
            FormEyebrow(text: Text("Artist"))
                .frame(width: columns.artist, alignment: .leading)
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.slots.column.length")
                )
            )
            .frame(width: ReleaseMetadataTrackColumns.length)
            Color.clear.frame(width: ReleaseMetadataTrackColumns.action)
        }
        .padding(.vertical, 6)
    }

    @ViewBuilder
    private func rows(_ section: BridgeMappingTrackSection) -> some View {
        switch section.content {
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
            if let track = mapping.track {
                Text(track.trackNumber?.formatted() ?? "\u{2014}")
                    .frame(width: ReleaseMetadataTrackColumns.track)
                Text(track.title)
                    .frame(width: columns.title, alignment: .leading)
                    .lineLimit(1)
                Text(trackArtistText(track.artistAssignments))
                    .frame(width: columns.artist, alignment: .leading)
                    .lineLimit(1)
            }
            else {
                Color.clear.frame(width: ReleaseMetadataTrackColumns.track)
                Text(coreString("ui.import.becomes.awaiting_pick"))
                    .frame(width: columns.title, alignment: .leading)
                Color.clear.frame(width: columns.artist)
            }
            Text(mapping.displayedDuration)
                .monospacedDigit()
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .trailing
                )
            Color.clear.frame(width: ReleaseMetadataTrackColumns.action)
        }
        .font(.system(size: 12))
        .padding(.vertical, 10)
        .overlay(alignment: .top) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }

    private func readOnlySource(_ source: BridgeMappingSource) -> some View {
        HStack(spacing: 6) {
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
                .font(.system(size: 12, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

    private func sourceName(_ source: BridgeMappingSource) -> String {
        switch source {
        case .file(let file): file.name
        case .sheetEntry(let entry): entry.title ?? entry.containerName
        case .missing: coreString("ui.import.slots.no_file")
        }
    }

    private func trackArtistText(
        _ assignments: BridgeTrackArtistAssignments
    ) -> String {
        switch assignments {
        case .albumArtists: String(localized: "Album artist")
        case .explicit(let artists):
            artists.map(\.displayName).formatted(.list(type: .and))
        }
    }

    private func sourceCaption(_ name: String) -> some View {
        Label(name, systemImage: "list.bullet.rectangle")
            .font(.system(size: 12, weight: .medium, design: .monospaced))
            .foregroundStyle(.secondary)
            .frame(width: tableWidth, alignment: .leading)
            .padding(.vertical, 6)
    }

    private var readOnlyFiles: some View {
        VStack(alignment: .leading, spacing: 0) {
            FormSectionHeader(title: String(localized: "Files"), ruled: true)
            ForEach(table.files, id: \.rowId) { fileRow in
                switch fileRow {
                case .file(let file):
                    HStack {
                        if file.role.fileRole.isDocument {
                            Button(file.name) {
                                onOpenDocument(file.name, file.localPath)
                            }
                            .buttonStyle(.plain)
                        }
                        else {
                            Text(file.name)
                        }
                        Spacer()
                        Text(file.sizeText).foregroundStyle(.tertiary)
                    }
                case .sheet(let sheet):
                    Button(sheet.name) {
                        onOpenDocument(sheet.name, sheet.localPath)
                    }
                    .buttonStyle(.plain)
                }
            }
            .font(.system(size: 12, design: .monospaced))
            .padding(.vertical, 10)
        }
    }
}
