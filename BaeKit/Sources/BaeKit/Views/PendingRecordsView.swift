import SwiftUI

public struct PendingRecordsView: View {
    public let records: BridgePendingRecords
    public let perform: (BridgePendingRecord) async throws -> Void

    public init(
        records: BridgePendingRecords,
        perform: @escaping (BridgePendingRecord) async throws -> Void
    ) {
        self.records = records
        self.perform = perform
    }

    public var body: some View {
        switch records {
        case .loading: ProgressView()
        case .failed(let error):
            if let displayed = DisplayError(error) {
                ErrorDetailDisclosure(error: displayed)
            }
        case .ready(let records):
            ForEach(records, id: \.self) { record in
                PendingRecordRow(record: record, perform: perform)
            }
        }
    }
}

private struct PendingRecordRow: View {
    let record: BridgePendingRecord
    let perform: (BridgePendingRecord) async throws -> Void
    @State
    private var working = false
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ErrorDetailDisclosure(
                error: DisplayError(
                    line: localizedCoreString(
                        bridgePendingReasonKey(reason: record.reason.kind)
                    ),
                    detail: record.reason.detail
                ),
                tone: .neutral,
                showIcon: false
            )
            if let key = bridgePendingActionKey(action: record.action) {
                Button(localizedCoreString(key)) {
                    working = true
                    error = nil
                    Task {
                        do { try await perform(record) }
                        catch { self.error = DisplayError(error) }
                        working = false
                    }
                }
                .disabled(working)
            }
            if let error { ErrorDetailDisclosure(error: error) }
        }
    }
}
