import Foundation

extension BridgeCloudProvider {
    /// What a storage setup list says under the provider's name.
    public var setupSummary: String {
        switch self {
        case .cloudKit:
            String(
                localized: "Uses the Apple Account this Mac is signed in to."
            )
        case .dropbox: String(localized: "A shared folder in your Dropbox.")
        case .oneDrive: String(localized: "A folder in your OneDrive.")
        case .s3: String(localized: "A bucket you manage.")
        }
    }
}
