import BaeKit
import SwiftUI

/// Loading and saving shared by candidate and library cover pickers.
@MainActor
@Observable
final class CoverPickerState {
    private(set) var remoteCovers: [BridgeRemoteCover]?
    private(set) var isLoading = false
    private(set) var isSaving = false
    private(set) var errorMessage: String?
    private var refreshTask: Task<Void, Never>?
    private var saveTask: Task<Void, Never>?
    private var loadVersion = 0

    func load(_ fetch: () async throws -> [BridgeRemoteCover]) async {
        loadVersion += 1
        let version = loadVersion
        isLoading = true
        errorMessage = nil
        defer {
            if loadVersion == version { isLoading = false }
        }
        do {
            let covers = try await fetch()
            try Task.checkCancellation()
            guard loadVersion == version else { return }
            remoteCovers = covers
        }
        catch is CancellationError {}
        catch {
            guard loadVersion == version, !Task.isCancelled else { return }
            errorMessage = error.displayLine.map {
                String(localized: "Failed to load covers: \($0)")
            }
        }
    }

    func refresh(_ fetch: @escaping () async throws -> [BridgeRemoteCover]) {
        refreshTask?.cancel()
        refreshTask = Task { await load(fetch) }
    }

    func save(
        _ apply: @escaping () async throws -> Void,
        onSaved: @escaping () -> Void
    ) {
        guard !isSaving else { return }
        isSaving = true
        errorMessage = nil
        saveTask = Task {
            defer { isSaving = false }
            do {
                try await apply()
                try Task.checkCancellation()
                onSaved()
            }
            catch is CancellationError {}
            catch {
                errorMessage = error.displayLine.map {
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
