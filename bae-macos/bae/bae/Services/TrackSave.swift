import BaeKit
import Foundation

/// Single-track save to a chosen output path under a preset. Used by the
/// release-detail "Save as…" affordance.
final class TrackSave: Sendable, Observable {
    let saveTrack:
        @Sendable (
            _ trackId: String, _ outputPath: URL, _ presetId: String
        ) async throws -> Void
    /// The default filename stem (no extension) the save panel pre-fills for a
    /// track, rendered by core from the library's one filename pattern, the
    /// same in every format. Reads only the database — no audio or cover.
    let suggestedName: @Sendable (_ trackId: String) async throws -> String

    init(
        saveTrack:
            @escaping @Sendable (String, URL, String)
            async throws -> Void = { _, _, _ in },
        suggestedName:
            @escaping @Sendable (String) async throws -> String = { _ in "" }
    ) {
        self.saveTrack = saveTrack
        self.suggestedName = suggestedName
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            saveTrack: {
                try await handle.saveTrack(
                    trackId: $0,
                    outputPath: $1.pathBytes,
                    presetId: $2
                )
            },
            suggestedName: {
                try await handle.saveTrackSuggestedName(trackId: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static func stub() -> TrackSave {
            TrackSave()
        }
    #endif
}
