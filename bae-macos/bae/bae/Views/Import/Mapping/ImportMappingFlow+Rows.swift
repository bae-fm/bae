import BaeKit
import Foundation

// MARK: - What the table's row controls store

extension ImportMappingFlow {
    /// Store a row's edited title and artists. Core keys it by the row's own
    /// identity, so the table it lands on is the one the person was looking
    /// at. A cancellation has no line and nothing to report.
    @MainActor
    static func editTrack(
        key: String,
        track: BridgeRawTrackEdit,
        services: ImportMappingServices
    ) async {
        do {
            try await services.importer.setCandidateTrackEdit(key, track)
        }
        catch is CancellationError {
            return
        }
        catch {
            if let failure = DisplayError(error)?
                .rewording({
                    String(localized: "Couldn't change that track: \($0)")
                })
            {
                services.onError(failure)
            }
        }
    }
}
