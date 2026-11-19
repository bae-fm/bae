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
    let onOpenImages: (String) -> Void
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
            MacImportFolderLine(
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
                    onPlayTrack: onPlayTrack,
                    images: images,
                    onOpenImages: onOpenImages,
                    onSaved: { _ in }
                )
            }
            else if let loadError {
                MacNotice(
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
                // Opening the release's form reads the library; nothing
                // shows until it lands.
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
        // When someone deletes the release while its editor is open, the
        // pane drops the editor; the import list then shows the folder as
        // found.
        .onChange(of: session?.releaseGone == true) { _, gone in
            if gone {
                session?.cancelTasks()
                session = nil
            }
        }
        .onDisappear {
            session?.cancelTasks()
        }
    }

    private var importedLine: some View {
        MacNotice(
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

    private static let coverSize: CGFloat = 200

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                MacImportFolderLine(
                    tab: candidate.tab,
                    folderName: candidate.displayName,
                    folderPaths: candidate.sourceFolderPaths,
                    onNavigateToPlacement: {}
                )
                switch step {
                case .preparing(step: .queued):
                    Label(
                        String(localized: "Queued to import"),
                        systemImage: "clock"
                    )
                    .foregroundStyle(.secondary)
                case .preparing, .running:
                    // Reads the percent itself, so a percent tick redraws
                    // the line and not the pane.
                    ImportProgressContainer(key: candidate.key, step: step)
                }
                HStack(alignment: .top, spacing: ThemeSpace.edge) {
                    Artwork(content: coverContent, pointSize: Self.coverSize)
                        .frame(width: Self.coverSize, height: Self.coverSize)
                        .clipShape(
                            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        )
                    if let values = candidate.edit {
                        MacImportReleaseSummary(
                            summary: ImportReleaseSummary(
                                candidate: candidate,
                                editValues: values
                            ),
                            style: .card
                        )
                    }
                }
            }
            .padding(.horizontal, ThemeSpace.section)
            .padding(.top, ThemeSpace.section)
            .padding(.bottom, ThemeSpace.page)
        }
    }
}
