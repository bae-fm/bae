import SwiftUI
import os.log

private let logger = Logger.bae("PendingRecordsView")

/// What coven is waiting on, one row per pending record: what it is about
/// over the reason, in plain words, with its diagnostic behind Details and the
/// record's action beside it wherever the person can act. Without `perform`
/// the list only informs, as while a library is still being restored.
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

/// One row of a Settings list: the subject over the reason, with Details and
/// the record's action at the right.
private struct PendingRecordRow: View {
    let record: BridgePendingRecord
    let perform: ((BridgePendingRecord) async throws -> Void)?

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?
    #if canImport(AppKit)
        @Environment(\.showErrorDetails)
        private var showErrorDetails
    #endif

    /// The action's label, when the person can act on this record here.
    private var actionTitle: String? {
        guard perform != nil, let key = record.actionKey else { return nil }
        return localizedCoreString(key)
    }

    private var reason: DisplayError {
        DisplayError(
            line: localizedCoreString(record.reason.key),
            detail: record.reason.detail
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            HStack(spacing: ThemeSpace.group) {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text(record.subject.localizedLine)
                        .themeText(.strong)
                    Text(reason.line)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                buttons
            }
            if let error {
                Notice(error: error)
            }
        }
        .onDisappear { task?.cancel() }
    }

    @ViewBuilder
    private var buttons: some View {
        HStack(spacing: ThemeSpace.related) {
            #if canImport(AppKit)
                if let details = ErrorDetails(error: reason) {
                    Button("Details") { showErrorDetails(details) }
                        .buttonStyle(.borderless)
                }
            #endif
            if let actionTitle {
                Button(actionTitle) { run() }
                    .disabled(task != nil)
            }
        }
        .fixedSize()
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
