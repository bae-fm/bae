import SwiftUI

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
        LabeledContent("Storage", value: location.locationText)
        Button("Disconnect", role: .destructive, action: onDisconnect)
    }
}
