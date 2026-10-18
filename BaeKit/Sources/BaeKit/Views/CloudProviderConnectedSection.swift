import SwiftUI

/// The library's cloud home as coven records it, one row per field, and the
/// disconnect action.
public struct CloudProviderConnectedSection: View {
    public let location: BridgeStorageLocation
    public let onDisconnect: () -> Void

    public init(
        location: BridgeStorageLocation,
        onDisconnect: @escaping () -> Void
    ) {
        self.location = location
        self.onDisconnect = onDisconnect
    }

    public var body: some View {
        LabeledContent("Provider", value: location.displayName)
        switch location {
        case .s3(let bucket, let region, let endpoint, _, let prefix):
            LabeledContent("Bucket", value: bucket)
            LabeledContent("Region", value: region)
            if let endpoint {
                LabeledContent("Endpoint", value: endpoint)
            }
            if !prefix.isEmpty {
                LabeledContent("Key Prefix", value: prefix)
            }
        case .dropbox, .oneDrive:
            if let account = location.accountLabel {
                LabeledContent("Account", value: account)
            }
            LabeledContent("Folder", value: location.locationSummary)
        case .cloudKit(let container, _, _):
            LabeledContent("Container", value: container)
        }
        Button("Disconnect", role: .destructive, action: onDisconnect)
    }
}
