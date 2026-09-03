import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ReIdentifySheet")

/// Stable key for a re-identify candidate. The identify pipeline routes
/// state changes by candidate key; the sheet uses this key to look its
/// candidate up in the import store and to clean it up on dismiss.
enum ReIdentifyKey {
    static func make(forReleaseId releaseId: String) -> String {
        "reidentify:\(releaseId)"
    }
}

/// Re-identify modal. Re-runs the full identify pipeline against an
/// existing library release's files, then commits the user's pick via
/// `re_identify_release`. After commit, prompts whether to also reseed
/// metadata from the new source.
///
/// Reuses `ImportSearchPane` end-to-end: identify state flows through the
/// import event bus into `ImportStore.reIdentifyCandidates`, keyed by
/// `ReIdentifyKey.make(forReleaseId:)`. The sheet looks the candidate up
/// at render time and feeds it to the same builder folder imports use.
struct ReIdentifySheet: View {
    let releaseId: String
    let displayName: String
    let onClose: () -> Void

    @Environment(Importer.self)
    private var importer
    @Environment(ReleaseEditor.self)
    private var releaseEditor
    @Environment(ImportStore.self)
    private var importStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettings)
    private var openSettings
    @Environment(SettingsNavigation.self)
    private var settingsNavigation

    @State
    private var phase: Phase = .identifying
    @State
    private var commitTask: Task<Void, Never>?
    @State
    private var landingAlbumId: String?
    /// The pressing the user clicked, pending the Set-identity commit in the
    /// footer. `nil` until a row is picked.
    @State
    private var selectedResult: BridgeMetadataResult?
    private enum Phase: Equatable {
        case identifying
        case committing
        case askRefresh
        case refreshing
        case error(String)
    }

    private var key: String {
        ReIdentifyKey.make(forReleaseId: releaseId)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .frame(width: 700, height: 600)
        .background(Theme.background)
        .task {
            await startReIdentify()
        }
        .onDisappear {
            commitTask?.cancel()
            // Stop the core-side pipeline: the identify driver and any in-flight
            // artwork OCR for this candidate.
            importer.cancelAutoIdentify(key)
            // Drop the candidate so a future re-open starts cold rather
            // than replaying the prior session's terminal state.
            importStore.reIdentifyCandidates.removeValue(forKey: key)
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("Re-identify")
                    .font(.headline)
                Text(displayName)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            // A rip identifies itself when no source knows it. The escape
            // belongs beside Close: it commits the release outright rather
            // than picking anything on the page below.
            Button(coreString("ui.import.metadata.file_tags") + "\u{2026}") {
                commit(.fileTags)
            }
            .buttonStyle(.link)
            Button("Close") { closeAndNavigate() }
                .keyboardShortcut(.cancelAction)
        }
        .padding()
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        switch phase {
        case .identifying:
            if let candidate = importStore.reIdentifyCandidates[key] {
                CandidateRuntimeReader(key: key) { runtime in
                    CandidateSignalsReader(key: key) { signals in
                        identifyPane(
                            candidate: candidate,
                            runtime: runtime,
                            signals: signals
                        )
                    }
                }
            }
            else {
                ProgressView("Starting re-identify...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        case .committing:
            ProgressView("Committing identity...")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .askRefresh:
            refreshPrompt
        case .refreshing:
            ProgressView("Refreshing metadata from new source...")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .error(let message):
            errorBanner(message: message)
        }
    }

    /// The identify pane while the sheet is running its own re-identification.
    /// Its run has no candidate row anywhere — the release is already in the
    /// library — so both its identify state and the signals its manual form
    /// suggests from come from this sheet's key on the two shared signals,
    /// the same ones the import pane reads.
    private func identifyPane(
        candidate: Candidate,
        runtime: BridgeCandidateRuntimeSnapshot?,
        signals: Signals?
    ) -> some View {
        VStack(spacing: 0) {
            ImportSearchFlow.buildSearchPane(
                services: ImportSearchFlow.ImportServices(
                    importer: importer,
                    importStore: importStore
                ),
                input: ImportSearchFlow.SearchPaneInput(
                    candidate: candidate,
                    key: key,
                    selectedReleaseId: selectedResult?.releaseId,
                    runtime: runtime,
                    liveSignals: signals
                ),
                openSettings: {
                    settingsNavigation.open(
                        .discogs,
                        present: { openSettings() }
                    )
                },
                // The sheet's own header closes it, so the pane offers no
                // way back of its own.
                onBack: nil,
                // Re-identify has no editable confirm page (the release
                // already has metadata; "Edit metadata..." covers
                // post-commit edits). Picking a pressing claims it, and the
                // footer commits it via `re_identify_release`.
                onSelect: { result in
                    selectedResult = result
                },
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            if let selectedResult {
                selectionFooter(for: selectedResult)
            }
        }
    }

    // MARK: - Selection footer

    /// Footer shown once a pressing is picked: the commit that stores it as
    /// the release's selected external metadata provenance.
    private func selectionFooter(
        for result: BridgeMetadataResult
    ) -> some View {
        HStack(alignment: .center, spacing: 12) {
            Spacer(minLength: 0)
            Button("Set identity") {
                commit(
                    .externalRelease(
                        releaseId: result.releaseId,
                        source: result.source
                    )
                )
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Theme.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(.white.opacity(0.08)).frame(height: 1)
        }
    }

    // MARK: - Refresh prompt

    // Only reachable after a source-backed commit. A File Tags commit reseeds
    // its rows from the rip's file tags inside `re_identify_release`, so it has
    // nothing to confirm and never lands here.
    private var refreshPrompt: some View {
        VStack(spacing: 16) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 48))
                .foregroundStyle(.green)
            Text("Identity updated.")
                .font(.headline)
            Text(
                "Pull the album, track, and pressing fields from the newly-identified source? Edits you've made are overwritten."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 420)
            HStack(spacing: 12) {
                Button("Keep current metadata") { finish() }
                Button("Refresh from new source") {
                    refreshMetadata()
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Close the sheet, then navigate the album grid to whichever album
    /// the release lives on now. set_identity may have moved it: a
    /// cross-source merge lands it on a sibling album, a File Tags commit
    /// always opens a fresh one. The grid follows the release so the
    /// user lands looking at the same content they re-identified. When
    /// the user dismisses before any commit landed (`landingAlbumId`
    /// nil), close without navigating — the source album stays expanded.
    private func closeAndNavigate() {
        let target = landingAlbumId
        onClose()
        if let target {
            uiStore.navigateToAlbum(target, releaseId: releaseId)
        }
    }

    // MARK: - Error banner

    private func errorBanner(message: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 48))
                .foregroundStyle(.red)
            Text("Re-identify failed.")
                .font(.headline)
            Text(message)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
            // closeAndNavigate respects landingAlbumId — if the identity
            // commit succeeded and only the refresh failed, the user
            // still gets routed to the new album. Pure-identify errors
            // (no commit) just close.
            Button("Close") { closeAndNavigate() }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Actions

extension ReIdentifySheet {
    fileprivate func startReIdentify() async {
        // Seed the candidate so `ImportSearchFlow.buildSearchPane` has
        // something to read. It carries only this session's own work — the
        // pick and the search state; the run's identify state comes from the
        // candidate-runtime signal under the same key.
        if importStore.reIdentifyCandidates[key] == nil {
            importStore.reIdentifyCandidates[key] = Candidate(
                reIdentifyKey: key,
                releaseId: releaseId,
                displayName: displayName
            )
        }
        // Identify subscribes, then extraction streams the release's signals
        // (disc ID + artwork resolved from the library). Resolution failures
        // are logged in core, not surfaced — identify lands in ManualOnly when
        // nothing resolves, the same as a folder with no signals.
        importer.autoIdentifyRelease(key, releaseId)
    }

    fileprivate func commit(_ choice: BridgeReleaseReseed) {
        commitTask?.cancel()
        phase = .committing
        let releaseEditor = releaseEditor
        let releaseId = self.releaseId
        commitTask = Task { @MainActor in
            do {
                let albumId = try await releaseEditor.reIdentifyRelease(
                    releaseId,
                    choice
                )
                landingAlbumId = albumId
                switch choice {
                case .fileTags:
                    // `re_identify_release` reseeds the rows from the rip's
                    // file tags as part of a File Tags commit, so there's
                    // nothing to confirm — go straight to the new album.
                    closeAndNavigate()
                case .externalRelease:
                    phase = .askRefresh
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Re-identify commit cancelled for release \(releaseId)"
                )
            }
            catch {
                logger.error(
                    "Re-identify commit failed: \(error.localizedDescription)"
                )
                // A failure with no line to show — a cancellation — is not an
                // error phase; there would be nothing in it.
                if let line = error.displayLine {
                    phase = .error(line)
                }
            }
        }
    }

    fileprivate func refreshMetadata() {
        commitTask?.cancel()
        phase = .refreshing
        let releaseEditor = releaseEditor
        let releaseId = self.releaseId
        commitTask = Task { @MainActor in
            do {
                let raw = try await releaseEditor.resetReleaseEditToSource(
                    releaseId
                )
                let shaped = shapeReleaseEdit(raw: raw)
                guard case .valid(let edit) = shaped else {
                    if case .invalid(let reason) = shaped {
                        phase = .error(reason.localizedMessage)
                    }
                    return
                }
                try await releaseEditor.updateReleaseMetadataUserEdit(
                    releaseId,
                    edit
                )
                finish()
            }
            catch is CancellationError {
                logger.debug(
                    "Refresh cancelled for release \(releaseId)"
                )
            }
            catch {
                logger.error(
                    "Refresh failed: \(error.localizedDescription)"
                )
                // A failure with no line to show — a cancellation — is not an
                // error phase; there would be nothing in it.
                if let line = error.displayLine {
                    phase = .error(line)
                }
            }
        }
    }

    fileprivate func finish() {
        // The user just answered the refresh prompt — they don't need a
        // second "Re-identify complete" beat after that. Close the sheet
        // and navigate the grid to wherever the release landed.
        closeAndNavigate()
    }
}

#if DEBUG
    // The stub importer's auto-identify is a no-op, so the sheet seeds its
    // candidate and renders the search pane in its starting (manual-only) state.
    #Preview("Re-identify Sheet") {
        ReIdentifySheet(
            releaseId: "rel-a-01",
            displayName: "Album Title \u{00B7} 2019 \u{00B7} CD",
            onClose: {},
        )
        .environment(SettingsNavigation())
        .albumDetailPreviewEnvironment(store: PreviewData.seededLibraryStore())
        .candidateReaderPreviewEnvironment()
        .preferredColorScheme(.dark)
    }
#endif
