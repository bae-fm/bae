import Foundation

extension BridgeCloudProvider {
    /// What a storage setup list says under the provider's name.
    public var setupSummary: String {
        switch self {
        case .cloudKit: String(localized: "Sync via your iCloud account")
        case .dropbox: String(localized: "Sync via Dropbox")
        case .oneDrive: String(localized: "Sync via Microsoft OneDrive")
        case .s3:
            String(
                localized:
                    "Any S3-compatible storage (AWS, Backblaze, Cloudflare R2, ...)"
            )
        }
    }

    /// The SF Symbol a storage setup list shows beside the provider.
    public var setupSymbol: String {
        switch self {
        case .cloudKit: "icloud"
        case .dropbox, .oneDrive: "externaldrive"
        case .s3: "externaldrive.connected.to.line.below"
        }
    }
}
