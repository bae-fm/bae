import BaeKit
import SwiftUI

/// Every catalog that describes a release, as a wrapped row of links to each
/// catalog's page, in the order core lists them.
struct ReleaseRecordsRow: View {
    let records: [BridgeReleaseRecord]
    var scale: ReleaseFactsScale = .pane

    var body: some View {
        FlowLayout(
            spacing: scale.recordSpacing,
            rowSpacing: scale.recordRowSpacing
        ) {
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
            // An address that won't parse still names the catalog.
            name.foregroundStyle(.secondary)
        }
    }

    private var name: some View {
        HStack(spacing: 3) {
            Text(verbatim: bridgeCatalogName(catalog: record.catalog))
            Image(systemName: "arrow.up.right")
                .imageScale(.small)
        }
        .themeText(.chip)
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
