import Foundation

extension BridgeCloudProvider {
    /// The provider's display name. Brand names pass through verbatim
    /// (proper nouns, not translated); the generic "S3-compatible" case
    /// resolves the catalog key bae-core owns. bae-core chooses the provider;
    /// this is the UI's locale rendering of it.
    var displayName: String {
        switch self {
        case .cloudKit: return "iCloud"
        case .googleDrive: return "Google Drive"
        case .dropbox: return "Dropbox"
        case .oneDrive: return "OneDrive"
        case .s3:
            return localizedCloudProviderName(provider: self)
        // A provider Rust added that this Swift hasn't mapped yet: show the raw
        // case so it reads as visibly unhandled rather than rendering blank.
        @unknown default: return "\(self)"
        }
    }
}

extension BridgeSyncProvider {
    /// The connected provider's display name. `BridgeSyncProvider` carries the
    /// connection details; the name is just its provider, so this maps to the
    /// `BridgeCloudProvider` case and reuses its `displayName`.
    var displayName: String {
        switch self {
        case .s3: return BridgeCloudProvider.s3.displayName
        case .googleDrive: return BridgeCloudProvider.googleDrive.displayName
        case .dropbox: return BridgeCloudProvider.dropbox.displayName
        case .oneDrive: return BridgeCloudProvider.oneDrive.displayName
        case .cloudKit: return BridgeCloudProvider.cloudKit.displayName
        @unknown default: return "\(self)"
        }
    }
}

/// Resolve a cloud provider's name through the catalog. Used for the
/// translatable cases (S3-compatible, and local-only when `provider` is nil) —
/// the brand-name providers return `nil` from the bridge and use `displayName`
/// directly, so they shouldn't reach the fallback below.
func localizedCloudProviderName(provider: BridgeCloudProvider?) -> String {
    guard let key = bridgeCloudProviderLabelKey(provider: provider) else {
        // No catalog key means a brand-name provider (proper noun); its
        // passthrough name is correct. `provider` is non-nil here because nil
        // maps to the local-only key above, so force-unwrap surfaces a logic
        // error rather than rendering a blank label.
        // swiftlint:disable:next force_unwrapping
        return provider!.displayName
    }
    return NSLocalizedString(key, tableName: "Core", bundle: .main, comment: "")
}
