import BaeKit
import Foundation

/// Single-track save to a chosen output path under a preset. Used by the
/// release-detail "Save As…" affordance.
final class TrackSave: Sendable, Observable {
    let saveTrack:
        @Sendable (
            _ trackId: String, _ outputPath: String, _ presetId: String
        ) async throws -> Void
    /// The default filename stem (no extension) the save panel pre-fills for a
    /// track under the selected preset, rendered by core from that preset's
    /// token pattern. Reads only the database — no audio or cover.
    let suggestedName:
        @Sendable (_ trackId: String, _ presetId: String) async throws -> String

    init(
        saveTrack:
            @escaping @Sendable (String, String, String)
            async throws -> Void = { _, _, _ in },
        suggestedName:
            @escaping @Sendable (String, String) async throws -> String = {
                _,
                _ in ""
            }
    ) {
        self.saveTrack = saveTrack
        self.suggestedName = suggestedName
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            saveTrack: {
                try await handle.saveTrack(
                    trackId: $0,
                    outputPath: $1,
                    presetId: $2
                )
            },
            suggestedName: {
                try await handle.saveTrackSuggestedName(
                    trackId: $0,
                    presetId: $1
                )
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
