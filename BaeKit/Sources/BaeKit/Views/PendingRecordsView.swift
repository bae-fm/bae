import SwiftUI
import os.log

private let logger = Logger.bae("PendingRecordsView")

/// What coven is waiting on, one row per pending record: what it is about and
/// the reason, in plain words, with its diagnostic behind a disclosure, and
/// the record's action wherever the person can act. Without `perform` the list only informs, as
/// while a library is still being restored.
public struct PendingRecordsView: View {
    public let records: [BridgePendingRecord]
    public let perform: ((BridgePendingRecord) async throws -> Void)?

    public init(
        records: [BridgePendingRecord],
        perform: ((BridgePendingRecord) async throws -> Void)?
    ) {
        self.records = records
        self.perform = perform
    }

    public var body: some View {
        ForEach(records, id: \.self) { record in
            PendingRecordRow(record: record, perform: perform)
        }
    }
}

private struct PendingRecordRow: View {
    let record: BridgePendingRecord
    let perform: ((BridgePendingRecord) async throws -> Void)?

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    /// The action's label, when the person can act on this record here.
    private var actionTitle: String? {
        guard perform != nil, let key = record.actionKey else { return nil }
        return localizedCoreString(key)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(record.subject.localizedLine)
                .themeText(.strong)
            ErrorNotice(
                error: DisplayError(
                    line: localizedCoreString(record.reason.key),
                    detail: record.reason.detail
                ),
                tone: .neutral,
                action: actionTitle.map {
                    ErrorAction(verbatim: $0, isDisabled: task != nil) { run() }
                }
            )
            if let error {
                ErrorNotice(error: error)
            }
        }
        .onDisappear { task?.cancel() }
    }

    private func run() {
        guard let perform else {
            preconditionFailure("an action shows only with a perform closure")
        }
        error = nil
        task = Task {
            defer { task = nil }
            do { try await perform(record) }
            catch is CancellationError {
                logger.debug("pending action cancelled as its row closed")
            }
            catch { self.error = DisplayError(error) }
        }
    }
}
