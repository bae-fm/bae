import BaeKit
import SwiftUI

/// Loading and saving shared by candidate and library cover pickers.
@MainActor
@Observable
final class CoverPickerState {
    private(set) var remoteItems: RemoteCoverItems
    private(set) var isSaving = false
    private(set) var error: DisplayError?
    private var refreshTask: Task<Void, Never>?
    private var saveTask: Task<Void, Never>?
    private var loadVersion = 0

    init(initialCovers: [BridgeRemoteCover] = []) {
        remoteItems = .loading(
            initialCovers.map {
                CoverItem(coverChoice: $0.coverChoice, label: $0.label)
            }
        )
    }

    func load(_ fetch: () async throws -> BridgeRemoteCoverGallery) async {
        loadVersion += 1
        let version = loadVersion
        let previousItems = remoteItems.items
        remoteItems = .loading(previousItems)
        error = nil
        do {
            let covers = try await fetch()
            try Task.checkCancellation()
            guard loadVersion == version else { return }
            remoteItems = RemoteCoverItems(covers)
        }
        catch is CancellationError {}
        catch {
            guard loadVersion == version, !Task.isCancelled else { return }
            let failure = DisplayError(error)?
                .rewording {
                    String(localized: "Failed to load covers: \($0)")
                }
            remoteItems = .failed(previousItems, failure: failure)
        }
    }

    func refresh(_ fetch: @escaping () async throws -> BridgeRemoteCoverGallery)
    {
        refreshTask?.cancel()
        refreshTask = Task { await load(fetch) }
    }

    func save(
        _ apply: @escaping () async throws -> Void,
        onSaved: @escaping () -> Void
    ) {
        guard !isSaving else { return }
        isSaving = true
        error = nil
        saveTask = Task {
            defer { isSaving = false }
            do {
                try await apply()
                try Task.checkCancellation()
                onSaved()
            }
            catch is CancellationError {}
            catch {
                self.error = DisplayError(error)?
                    .rewording {
                        String(localized: "Couldn't change the cover: \($0)")
                    }
            }
        }
    }

    func cancel() {
        refreshTask?.cancel()
        saveTask?.cancel()
    }
}
