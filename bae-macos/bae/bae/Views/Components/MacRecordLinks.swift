import BaeKit
import SwiftUI

/// Every catalog that describes a release, as a wrapped row of links to each
/// catalog's page, in the order core lists them.
struct MacRecordLinks: View {
    let records: [BridgeReleaseRecord]

    var body: some View {
        FlowLayout(spacing: 12, rowSpacing: 4) {
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
            }
            .buttonStyle(.accentPlain)
        }
        else {
            // An address that won't parse still names the catalog.
            name
        }
    }

    private var name: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(verbatim: bridgeCatalogName(catalog: record.catalog))
            Image(systemName: "arrow.up.right")
                .imageScale(.small)
        }
        .themeText(.chip)
        .foregroundStyle(.secondary)
        .fixedSize()
    }
}
