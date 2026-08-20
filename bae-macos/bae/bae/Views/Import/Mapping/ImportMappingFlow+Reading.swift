import BaeKit
import Foundation
import os.log

private let logger = Logger.bae("ImportMappingFlow")

// MARK: - Reading the folder's mapping

extension ImportMappingFlow {
    /// Read the folder's mapping table for what it is being read as: the
    /// release picked for it, its own file tags, or the open question the
    /// identify phase leaves.
    @MainActor
    static func readMapping(
        key: String,
        services: ImportMappingServices
    ) async {
        guard let candidate = services.importStore.candidate(forKey: key) else {
            return
        }
        switch (candidate.identity, candidate.pick) {
        case (.unknown, _):
            await ImportSearchFlow.refreshDecidedIdentity(
                importer: services.importer,
                importStore: services.importStore,
                key: key,
                pick: .unknown
            )
            .value
        case (.release, .some(let pick)):
            await ImportSearchFlow.refreshDecidedIdentity(
                importer: services.importer,
                importStore: services.importStore,
                key: key,
                pick: .release(
                    source: pick.source,
                    releaseId: pick.releaseId,
                    claim: pick.claim
                )
            )
            .value
        case (.release, .none):
            readCandidateMapping(key: key, services: services)
            return
        }
    }

    /// The table for a folder nobody has picked a release for: every source
    /// unit it offers, with what each becomes left open.
    @MainActor
    static func readCandidateMapping(
        key: String,
        services: ImportMappingServices
    ) {
        do {
            let mapping = try services.importer.candidateMapping(key)
            services.importStore.mutateCandidate(forKey: key) {
                $0.mapping = mapping
            }
        }
        catch {
            logger.error(
                "Reading the folder's mapping failed: \(error.localizedDescription)"
            )
            // No line means a cancellation: the log above already recorded it
            // and there is nothing to put in front of the user.
            if let line = error.displayLine {
                services.onError(
                    String(
                        localized:
                            "Couldn't read what this folder holds: \(line)"
                    )
                )
            }
        }
    }
}
