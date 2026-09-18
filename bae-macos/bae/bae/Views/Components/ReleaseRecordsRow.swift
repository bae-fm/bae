import BaeKit
import SwiftUI

/// Every catalog that describes a release, as a wrapped row of links.
///
/// One entry per catalog, in the order core lists them, each opening that
/// catalog's page for this release. The name is the catalog's own brand and
/// the address is built by core, so this row neither translates nor composes
/// anything — it draws what the records say.
struct ReleaseRecordsRow: View {
    let records: [BridgeReleaseRecord]

    var body: some View {
        FlowLayout(spacing: 12) {
            ForEach(records, id: \.catalog) { record in
                ReleaseRecordLink(record: record)
            }
        }
        .accessibilityIdentifier("release-records")
    }
}

/// One catalog's name and the way to its page for this release.
private struct ReleaseRecordLink: View {
    let record: BridgeReleaseRecord

    var body: some View {
        if let url = URL(string: record.url) {
            Link(destination: url) {
                name
                    .foregroundStyle(Theme.accent)
            }
            .buttonStyle(.plain)
        }
        else {
            // The address comes from core; one that will not parse is a broken
            // record, and the name still says which catalog describes this
            // release.
            name.foregroundStyle(.secondary)
        }
    }

    private var name: some View {
        HStack(spacing: 3) {
            Text(verbatim: bridgeCatalogName(catalog: record.catalog))
                .font(.system(size: 11.5, weight: .medium))
            Image(systemName: "arrow.up.right")
                .font(.system(size: 9, weight: .semibold))
        }
        .fixedSize()
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Two catalogs") {
        ReleaseRecordsRow(records: PreviewData.releaseRecordsPair)
            .padding()
            .frame(width: 420)
            .background(Theme.surfaceElevated)
    }

    #Preview("Every catalog") {
        ReleaseRecordsRow(records: PreviewData.releaseRecordsEveryCatalog)
            .padding()
            .frame(width: 420)
            .background(Theme.surfaceElevated)
    }
#endif
