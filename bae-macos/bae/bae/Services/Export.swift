import Foundation

/// Single-track export to a chosen output path + format. Used by the
/// release-detail export affordance.
final class Export: Sendable, Observable {
    let exportTrack:
        @Sendable (
            _ trackId: String, _ outputPath: String,
            _ format: BridgeExportFormat
        ) async throws -> Void
    /// The default filename stem (no extension) the save panel pre-fills for a
    /// track, rendered by core from the configured template. Cheap — reads only
    /// the database, no audio or cover.
    let suggestedName: @Sendable (_ trackId: String) async throws -> String

    init(
        exportTrack:
            @escaping @Sendable (String, String, BridgeExportFormat)
            async throws -> Void = { _, _, _ in },
        suggestedName:
            @escaping @Sendable (String) async throws -> String = { _ in "" }
    ) {
        self.exportTrack = exportTrack
        self.suggestedName = suggestedName
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            exportTrack: {
                try await handle.exportTrack(
                    trackId: $0,
                    outputPath: $1,
                    format: $2
                )
            },
            suggestedName: {
                try await handle.exportTrackSuggestedName(trackId: $0)
            }
        )
    }

    // periphery:ignore
    static let stub = Export()
}
