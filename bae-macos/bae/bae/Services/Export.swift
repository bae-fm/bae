import Foundation

/// Single-track export to a chosen output path + export selection. Used by the
/// release-detail export affordance.
final class Export: Sendable, Observable {
    let exportTrack:
        @Sendable (
            _ trackId: String, _ outputPath: String,
            _ selection: BridgeExportSelection
        ) async throws -> Void
    /// The default filename stem (no extension) the save panel pre-fills for a
    /// track, rendered by core from the configured template. Reads only the
    /// database — no audio or cover.
    let suggestedName: @Sendable (_ trackId: String) async throws -> String
    /// Filename extension for a selected export path, without a leading dot.
    let extensionForSelection:
        @Sendable (_ trackId: String, _ selection: BridgeExportSelection)
            async throws -> String

    init(
        exportTrack:
            @escaping @Sendable (String, String, BridgeExportSelection)
            async throws -> Void = { _, _, _ in },
        suggestedName:
            @escaping @Sendable (String) async throws -> String = { _ in "" },
        extensionForSelection:
            @escaping @Sendable (String, BridgeExportSelection) async throws
            -> String = { _, _ in "" }
    ) {
        self.exportTrack = exportTrack
        self.suggestedName = suggestedName
        self.extensionForSelection = extensionForSelection
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            exportTrack: {
                try await handle.exportTrack(
                    trackId: $0,
                    outputPath: $1,
                    selection: $2
                )
            },
            suggestedName: {
                try await handle.exportTrackSuggestedName(trackId: $0)
            },
            extensionForSelection: {
                try await handle.exportTrackExtension(
                    trackId: $0,
                    selection: $1
                )
            }
        )
    }

    // periphery:ignore
    static let stub = Export()
}
