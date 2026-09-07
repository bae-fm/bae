#if BAE_CLOUDKIT
    import BaeKit
    import CloudKit
    import Foundation
    import Testing

    @testable import bae

    @Suite("CloudKit conditional replacement")
    struct CloudKitRecordReplacementTests {
        @Test
        func staleVersionDoesNotSubmit() throws {
            let result = try CloudKitRecordReplacement.replace(
                record: try record("current"),
                expected: "old",
                asset: asset()
            ) { _ in Issue.record("stale replacement reached CloudKit") }
            #expect(result == .versionChanged)
        }

        @Test
        func replacementChecksServerVersionAndReturnsSavedVersion() throws {
            let input = try record("current")
            let saved = try record("next")
            let bytes = asset()
            let result = try CloudKitRecordReplacement.replace(
                record: input,
                expected: "current",
                asset: bytes
            ) { operation in
                #expect(operation.savePolicy == .ifServerRecordUnchanged)
                #expect(operation.isAtomic)
                #expect(operation.recordsToSave?.first === input)
                #expect((input["data"] as? CKAsset) === bytes)
                operation.perRecordSaveBlock?(
                    input.recordID,
                    .success(saved)
                )
                operation.modifyRecordsResultBlock?(.success(()))
            }
            #expect(result == .replaced(version: "next"))
        }

        @Test
        func concurrentServerReplacementReportsVersionChanged() throws {
            let result = try CloudKitRecordReplacement.replace(
                record: try record("current"),
                expected: "current",
                asset: asset()
            ) { operation in
                operation.perRecordSaveBlock?(
                    CKRecord.ID(recordName: "current"),
                    .failure(CKError(.serverRecordChanged))
                )
                operation.modifyRecordsResultBlock?(
                    .failure(CKError(.partialFailure))
                )
            }
            #expect(result == .versionChanged)
        }

        @Test
        func operationFailureIsNotMaskedByRecordSuccess() throws {
            let saved = try record("next")
            #expect(throws: CKError.self) {
                try CloudKitRecordReplacement.replace(
                    record: try record("current"),
                    expected: "current",
                    asset: asset()
                ) { operation in
                    operation.perRecordSaveBlock?(
                        CKRecord.ID(recordName: "current"),
                        .success(saved)
                    )
                    operation.modifyRecordsResultBlock?(
                        .failure(CKError(.networkFailure))
                    )
                }
            }
        }

        @Test
        func missingSavedVersionFails() throws {
            #expect(throws: CloudKitError.self) {
                try CloudKitRecordReplacement.replace(
                    record: try record("current"),
                    expected: "current",
                    asset: asset()
                ) { operation in
                    operation.perRecordSaveBlock?(
                        CKRecord.ID(recordName: "current"),
                        .success(CKRecord(recordType: "BaeFile"))
                    )
                    operation.modifyRecordsResultBlock?(.success(()))
                }
            }
        }

        private func record(_ version: String) throws -> CKRecord {
            // Build a server-record fixture using CloudKit's encoded system
            // fields. CKRecord cannot be subclassed; its archived ETag is the
            // read-only change tag that the production operation must preserve.
            let record = CKRecord(
                recordType: "BaeFile",
                recordID: CKRecord.ID(recordName: version)
            )
            let encoder = NSKeyedArchiver(requiringSecureCoding: true)
            record.encodeSystemFields(with: encoder)
            encoder.encode(version, forKey: "FixtureVersion")
            encoder.finishEncoding()
            var archive = try #require(
                PropertyListSerialization.propertyList(
                    from: encoder.encodedData,
                    format: nil
                )
                    as? [String: Any]
            )
            var fields = try #require(archive["$top"] as? [String: Any])
            fields["ETag"] = try #require(fields["FixtureVersion"])
            archive["$top"] = fields
            let data = try PropertyListSerialization.data(
                fromPropertyList: archive,
                format: .binary,
                options: 0
            )
            let decoder = try NSKeyedUnarchiver(forReadingFrom: data)
            decoder.requiresSecureCoding = true
            defer { decoder.finishDecoding() }
            let decoded = try #require(CKRecord(coder: decoder))
            #expect(decoded.recordChangeTag == version)
            return decoded
        }

        private func asset() -> CKAsset {
            CKAsset(fileURL: URL(fileURLWithPath: "/unused-test-asset"))
        }
    }
#endif
