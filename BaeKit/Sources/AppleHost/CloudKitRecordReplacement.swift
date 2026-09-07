#if BAE_CLOUDKIT
    import BaeKit
    import CloudKit
    import Foundation

    enum CloudKitRecordReplacement {
        static func replace(
            record: CKRecord,
            expected: String,
            asset: CKAsset,
            submit: (CKModifyRecordsOperation) -> Void
        ) throws -> BridgeCloudConditionalWriteOutcome {
            guard let version = record.recordChangeTag else {
                throw CloudKitError.Storage(
                    msg: "CloudKit record has no change tag"
                )
            }
            guard version == expected else { return .versionChanged }

            record["data"] = asset
            let completion = Completion()
            let operation = CKModifyRecordsOperation(recordsToSave: [record])
            operation.savePolicy = .ifServerRecordUnchanged
            operation.isAtomic = true
            operation.perRecordSaveBlock = { _, result in
                completion.record(result)
            }
            operation.modifyRecordsResultBlock = { result in
                completion.finish(result)
            }
            submit(operation)

            do {
                let saved = try completion.wait()
                guard let version = saved.recordChangeTag else {
                    throw CloudKitError.Storage(
                        msg: "CloudKit replacement returned no change tag"
                    )
                }
                return .replaced(version: version)
            }
            catch let error as CKError where error.code == .serverRecordChanged
            {
                return .versionChanged
            }
        }

        /// CloudKit invokes the record callback before the operation callback.
        /// Retain both results so an operation failure cannot hide behind a
        /// successful record callback, and a partial failure keeps its cause.
        private final class Completion: @unchecked Sendable {
            private let lock = NSLock()
            private let semaphore = DispatchSemaphore(value: 0)
            private var saved: Result<CKRecord, Error>?
            private var operation: Result<Void, Error>?

            func record(_ result: Result<CKRecord, Error>) {
                lock.withLock { saved = result }
            }

            func finish(_ result: Result<Void, Error>) {
                lock.withLock { operation = result }
                semaphore.signal()
            }

            func wait() throws -> CKRecord {
                semaphore.wait()
                return try lock.withLock {
                    guard let operation else {
                        throw CloudKitError.Storage(
                            msg:
                                "CloudKit replacement returned no operation result"
                        )
                    }
                    if case .failure(let error) = operation {
                        // A single-record partial failure wraps the record's
                        // actual error (including a concurrent replacement).
                        if let cloudError = error as? CKError,
                            cloudError.code == .partialFailure,
                            case .failure(let recordError) = saved
                        {
                            throw recordError
                        }
                        throw error
                    }
                    guard let saved else {
                        throw CloudKitError.Storage(
                            msg:
                                "CloudKit replacement returned no record result"
                        )
                    }
                    return try saved.get()
                }
            }
        }
    }
#endif
