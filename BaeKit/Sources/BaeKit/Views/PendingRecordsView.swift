import SwiftUI

/// What coven is waiting on while a library is restored or joined, one row
/// per pending record: what it is about over the reason, in plain words, with
/// its diagnostic behind Details.
public struct PendingRecordsView: View {
    public let records: [BridgePendingRecord]

    public init(records: [BridgePendingRecord]) {
        self.records = records
    }

    public var body: some View {
        ForEach(records, id: \.self) { record in
            PendingRecordRow(record: record)
        }
    }
}

/// One row: the subject over the reason, with Details at the right.
private struct PendingRecordRow: View {
    let record: BridgePendingRecord

    #if canImport(AppKit)
        private var showErrorDetails = ErrorDetailsOpener()
    #endif

    init(record: BridgePendingRecord) {
        self.record = record
    }

    private var reason: DisplayError {
        DisplayError(
            line: localizedCoreString(record.reason.key),
            detail: record.reason.detail
        )
    }

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(record.subject.localizedLine)
                    .themeText(.strong)
                Text(reason.line)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            #if canImport(AppKit)
                if let details = ErrorDetails(error: reason) {
                    Button("Details") { showErrorDetails(details) }
                        .buttonStyle(.accentBorderless)
                        .fixedSize()
                }
            #endif
        }
        #if canImport(AppKit)
            .errorDetailsAlert(showErrorDetails.presented)
        #endif
    }
}
