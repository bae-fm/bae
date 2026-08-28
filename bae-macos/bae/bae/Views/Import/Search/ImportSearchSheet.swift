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
    let presentation: ModalPresentation

    /// Which half of the pane is showing. The sheet owns it: it opens on what
    /// identification found, and a session that ended in the typed search does
    /// not decide where the next one starts.
    @State
    private var mode: SearchMode = .signals

    @Environment(Importer.self)
    private var importer
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
                CandidateRuntimeReader(key: candidateKey) { runtime in
                    CandidateSignalsReader(key: candidateKey) { signals in
                        searchPane(
                            for: candidate,
                            runtime: runtime,
                            signals: signals
                        )
                    }
                }
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
                uiStore.dismissModal(presentation)
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

    private func searchPane(
        for candidate: Candidate,
        runtime: BridgeCandidateRuntimeSnapshot?,
        signals: Signals?
    ) -> some View {
        ImportSearchFlow.buildSearchPane(
            services: ImportSearchFlow.ImportServices(
                importer: importer,
                importStore: importStore,
                configStore: configStore
            ),
            input: ImportSearchFlow.SearchPaneInput(
                candidate: candidate,
                key: candidateKey,
                selectedReleaseId: candidate.pickedRelease?.releaseId,
                runtime: runtime,
                liveSignals: signals
            ),
            mode: $mode,
            openSettings: { openSettings() },
            // Reading the folder from File Tags is the mapping pane's own source
            // control, always visible there — not a link inside the search.
            onUseFileTags: nil,
            onSelect: { result in
                ImportSearchFlow.chooseReleaseFromSearchSheet(
                    result,
                    importer: importer,
                    importStore: importStore,
                    key: candidateKey,
                    onConfirmed: {
                        uiStore.dismissModal(presentation)
                    }
                )
            },
        )
    }
}
