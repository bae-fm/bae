import SwiftUI

/// The library's cloud home as coven records it, one row per field.
public struct CloudProviderConnectedSection: View {
    public let location: BridgeStorageLocation

    public init(location: BridgeStorageLocation) {
        self.location = location
    }

    public var body: some View {
        LabeledContent("Provider", value: location.displayName)
        switch location {
        case .s3(let bucket, let region, let endpoint, _):
            LabeledContent("Bucket", value: bucket)
            LabeledContent("Region", value: region)
            if let endpoint {
                LabeledContent("Endpoint", value: endpoint)
            }
        case .dropbox, .oneDrive:
            if let account = location.accountLabel {
                LabeledContent("Account", value: account)
            }
            LabeledContent("Folder", value: location.locationSummary)
        case .cloudKit(let container, _, _):
            LabeledContent("Container", value: container)
        }
    }
}
