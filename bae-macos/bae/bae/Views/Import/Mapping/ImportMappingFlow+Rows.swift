import BaeKit
import Foundation

// MARK: - What the table's row controls store

extension ImportMappingFlow {
    /// Store a row's edited track. Core keys it by the row's own identity, so
    /// the table it lands on is the one the person was looking at.
    @MainActor
    static func editTrack(
        key: String,
        track: BridgeRawTrackEdit,
        services: ImportMappingServices
    ) async {
        await write(services: services) {
            try await services.importer.setCandidateTrackEdit(key, track)
        } describe: { line in
            String(localized: "Couldn't change that track: \(line)")
        }
    }

    /// Point a row at one of the folder's audio units. The row starts writing
    /// that audio because the editor is what says which audio a track's samples
    /// come from — core's reading of the folder produced the row, and this is
    /// the user overruling it.
    @MainActor
    static func chooseFile(
        key: String,
        trackId: String,
        audio: BridgeAudioFile,
        services: ImportMappingServices
    ) async {
        guard
            var track = services.importStore.candidate(forKey: key)?
                .mapping.trackMappings
                .compactMap(\.track)
                .first(where: { $0.id == trackId })
        else { return }
        track.file = audio
        await editTrack(key: key, track: track, services: services)
    }

    /// Include one source offer without reapplying metadata to the other tracks.
    @MainActor
    static func addTrack(
        key: String,
        audio: BridgeAudioFile,
        candidate: BridgeCandidateAsRead,
        services: ImportMappingServices
    ) async {
        await write(services: services) {
            try await services.importer.addCandidateTrack(key, audio, candidate)
        } describe: { line in
            String(localized: "Couldn't save that change: \(line)")
        }
    }

    /// Remove a track from the import while leaving its source audio available.
    @MainActor
    static func drop(
        key: String,
        trackId: String,
        services: ImportMappingServices
    ) async {
        await write(services: services) {
            try await services.importer.dropCandidateTrack(key, trackId)
        } describe: { line in
            String(localized: "Couldn't drop that track: \(line)")
        }
    }

    /// Run one write and put its failure in front of the user. A cancellation
    /// has no line and nothing to report.
    @MainActor
    private static func write(
        services: ImportMappingServices,
        _ operation: () async throws -> Void,
        describe: (String) -> String
    ) async {
        do {
            try await operation()
        }
        catch is CancellationError {
            return
        }
        catch {
            if let line = error.displayLine {
                services.onError(describe(line))
            }
        }
    }
}
