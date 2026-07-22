import BaeKit
import Foundation

/// Single-track save to a chosen output path under a preset. Used by the
/// release-detail "Save As…" affordance.
final class Export: Sendable, Observable {
    let saveTrack:
        @Sendable (
            _ trackId: String, _ outputPath: String, _ presetId: String
        ) async throws -> Void
    /// The default filename stem (no extension) the save panel pre-fills for a
    /// track, rendered by core from the configured template. Reads only the
    /// database — no audio or cover.
    let suggestedName: @Sendable (_ trackId: String) async throws -> String

    init(
        saveTrack:
            @escaping @Sendable (String, String, String)
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
                    outputPath: $1,
                    presetId: $2
                )
            },
            suggestedName: {
                try await handle.exportTrackSuggestedName(trackId: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static let stub = Export()
    #endif
}
