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

    /// Apply one row's artist assignments to the selected rows atomically.
    @MainActor
    static func setTrackArtists(
        key: String,
        trackIds: [String],
        assignments: BridgeTrackArtistAssignments,
        services: ImportMappingServices
    ) async {
        await write(services: services) {
            try await services.importer.setCandidateTrackArtists(
                key,
                trackIds,
                assignments
            )
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

    /// Drop a row the release names and this folder has nothing for. Nothing
    /// on disk changes: the release is simply imported without that track.
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
