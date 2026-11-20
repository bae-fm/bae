import BaeKit
import Foundation

@testable import bae

extension ImportSelectionOperations {
    /// The inert operations with the ones a test watches replaced.
    static func stub(
        keysBetween:
            @escaping @Sendable (
                BridgeImportListRequest,
                Data,
                Data
            ) async throws -> [Data] = { _, _, _ in [] },
        allKeys:
            @escaping @Sendable (BridgeImportListRequest) async throws -> [Data] =
            { _ in [] },
        combine:
            @escaping @Sendable (BridgeImportListRequest, [Data]) async throws
            -> Data = { _, _ in
                throw StubError.notImplemented
            },
        separate:
            @escaping @Sendable (BridgeImportListRequest, [Data]) async throws
            -> [Data] = { _, _ in
                throw StubError.notImplemented
            },
        progressStream:
            @escaping @Sendable () -> LatestStream<
                BridgeSelectionActionProgress
            > = { .bulkActionProgress() },
        run:
            @escaping @Sendable (
                BridgeImportListRequest,
                BridgeCandidateAction,
                [Data],
                LatestStream<BridgeSelectionActionProgress>
            ) async throws -> [BridgeSelectionActionFailure] = { _, _, _, _ in
                []
            }
    ) -> ImportSelectionOperations {
        ImportSelectionOperations(
            keysBetween: keysBetween,
            allKeys: allKeys,
            sourceFolders: inert.sourceFolders,
            combine: combine,
            separate: separate,
            progressStream: progressStream,
            run: run
        )
    }
}

extension ImportSelection {
    /// A selection over `operations` on a list showing Found, as the sidebar
    /// shows it once it has read its first list.
    static func onList(
        operations: ImportSelectionOperations = .inert
    ) -> ImportSelection {
        let selection = ImportSelection(operations: operations)
        selection.show(
            BridgeImportListRequest(
                view: BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilter: .all,
                    order: .newestFirst
                ),
                folds: .allOpen
            )
        )
        return selection
    }
}
