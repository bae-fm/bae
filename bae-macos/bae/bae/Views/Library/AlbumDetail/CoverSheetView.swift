import BaeKit
import SwiftUI

/// Persisted-release artwork is read by library file identity, including files
/// available only through cloud storage. Scanned candidate paths never enter
/// this picker.
struct CoverSheetView: View {
    let releaseId: String
    let fetchRemoteCovers: () async throws -> [BridgeRemoteCover]
    let onSelect: (BridgeCoverSelection) async throws -> Void
    let onDone: () -> Void

    @Environment(Library.self)
    private var library
    @State
    private var remoteCovers: [BridgeRemoteCover] = []
    @State
    private var releaseFiles: [BridgeFile] = []
    @State
    private var loading = true
    @State
    private var saving = false
    @State
    private var remoteError: String?
    @State
    private var releaseError: String?
    @State
    private var saveError: String?
    @State
    private var refreshTask: Task<Void, Never>?
    @State
    private var saveTask: Task<Void, Never>?

    var body: some View {
        CoverGalleryView(
            remoteItems: remoteCovers.map {
                CoverItem(coverChoice: $0.coverChoice, label: $0.label)
            },
            releaseItems: releaseFiles.map {
                CoverItem(releaseId: releaseId, file: $0)
            },
            selectedCover: nil,
            isLoading: loading,
            isSaving: saving,
            errorMessage: saveError ?? releaseError ?? remoteError,
            onRefresh: {
                refreshTask?.cancel()
                refreshTask = Task { await loadCovers() }
            },
            onSelect: { item in
                guard !saving else { return }
                saving = true
                saveError = nil
                saveTask = Task { @MainActor in
                    do {
                        try await onSelect(item.selection)
                        saving = false
                        onDone()
                    }
                    catch is CancellationError { saving = false }
                    catch {
                        saving = false
                        saveError = error.displayLine.map {
                            String(
                                localized: "Couldn't change the cover: \($0)"
                            )
                        }
                    }
                }
            },
            onDone: onDone
        )
        .task(id: releaseId) { await loadCovers() }
        .task(id: releaseId) {
            for await result in library.releaseDetails(releaseId) {
                do {
                    guard let release = try result.get() else {
                        releaseFiles = []
                        releaseError = String(
                            localized:
                                "This release is no longer in the library."
                        )
                        continue
                    }
                    releaseFiles = release.imageFiles
                    releaseError = nil
                }
                catch { releaseError = error.displayLine }
            }
        }
        .onDisappear {
            refreshTask?.cancel()
            saveTask?.cancel()
        }
    }

    @MainActor
    private func loadCovers() async {
        loading = true
        remoteError = nil
        do {
            remoteCovers = try await fetchRemoteCovers()
            loading = false
        }
        catch is CancellationError { loading = false }
        catch {
            remoteError = error.displayLine.map {
                String(localized: "Failed to load covers: \($0)")
            }
            loading = false
        }
    }
}
