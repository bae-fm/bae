import BaeKit
import SwiftUI

/// The release header's editor: search, opened from the header's change
/// control and closed by picking a release.
///
/// This is the search pane the mapping pane replaced as a permanently mounted
/// surface. It is the same pane — identify state, signals toolbar, manual form,
/// results — reached the way an editor is reached, so the pane behind it stays
/// one column of files and one column of tracks rather than three panes fighting
/// for the width.
struct ImportSearchSheet: View {
    let candidateKey: String

    @Environment(Importer.self)
    private var importer
    @Environment(Library.self)
    private var library
    @Environment(ImportStore.self)
    private var importStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettings)
    private var openSettings

    var body: some View {
        if let candidate = importStore.candidate(forKey: candidateKey) {
            VStack(spacing: 0) {
                header
                searchPane(for: candidate)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .frame(width: 940, height: 700)
            .background(Theme.surface)
            .clipShape(RoundedRectangle(cornerRadius: 10))
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text(coreString("ui.import.header.find_release"))
                .font(.system(size: 13, weight: .semibold))
            Spacer()
            Button {
                uiStore.dismissModal()
            } label: {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 24, height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Close")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }

    private func searchPane(for candidate: Candidate) -> some View {
        ImportSearchFlow.buildSearchPane(
            services: ImportSearchFlow.ImportServices(
                importer: importer,
                library: library,
                importStore: importStore,
                configStore: configStore
            ),
            input: ImportSearchFlow.SearchPaneInput(
                candidate: candidate,
                key: candidateKey,
                selectedReleaseId: candidate.pickedReleaseId
            ),
            openSettings: { openSettings() },
            onAddAsUnknown: addAsUnknown(candidate),
            onSelect: { result in
                ImportSearchFlow.prefetchAndConfirm(
                    library: library,
                    importStore: importStore,
                    key: candidateKey,
                    releaseId: result.releaseId,
                    source: result.source
                )
                uiStore.dismissModal()
            },
        )
    }

    /// Seeding the tracklist from the files' own tags is a way of answering the
    /// header's question, so it closes the editor the same way a pick does. A
    /// re-identify candidate has no folder to read, so it is not offered.
    private func addAsUnknown(_ candidate: Candidate) -> (() -> Void)? {
        guard case .folder(let folderPath, _) = candidate.source else {
            return nil
        }
        return {
            ImportSearchFlow.addAsUnknown(
                importer: importer,
                importStore: importStore,
                key: candidateKey,
                folderPath: folderPath
            )
            uiStore.dismissModal()
        }
    }
}
