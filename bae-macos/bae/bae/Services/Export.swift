import Foundation

/// Single-track export to a chosen output path + format. Used by the
/// release-detail export affordance.
final class Export: Sendable, Observable {
    let exportTrack:
        @Sendable (
            _ trackId: String, _ outputPath: String,
            _ format: BridgeExportFormat
        ) async throws -> Void

    init(
        exportTrack:
            @escaping @Sendable (String, String, BridgeExportFormat)
            async throws -> Void = { _, _, _ in }
    ) {
        self.exportTrack = exportTrack
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            exportTrack: {
                try await handle.exportTrack(
                    trackId: $0,
                    outputPath: $1,
                    format: $2
                )
            }
        )
    }

    // periphery:ignore
    static let stub = Export()
}
