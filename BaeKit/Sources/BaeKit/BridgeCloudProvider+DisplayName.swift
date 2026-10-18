import Foundation

extension BridgeCloudProvider {
    /// The provider's display name. Brand names pass through verbatim
    /// (proper nouns, not translated); the generic "S3-compatible" case
    /// resolves the catalog key bae-core owns. bae-core chooses the provider;
    /// this is the UI's locale rendering of it.
    public var displayName: String {
        switch self {
        case .cloudKit: return "iCloud"
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

extension BridgeStorageLocation {
    /// The connected provider's display name. `BridgeStorageLocation` carries the
    /// connection details; the name is just its provider, so this maps to the
    /// `BridgeCloudProvider` case and reuses its `displayName`.
    public var displayName: String {
        switch self {
        case .s3: return BridgeCloudProvider.s3.displayName
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
public func localizedCloudProviderName(provider: BridgeCloudProvider?) -> String
{
    guard let key = bridgeCloudProviderLabelKey(provider: provider) else {
        // No catalog key means a brand-name provider (proper noun); its
        // passthrough name is correct. `provider` is non-nil here because nil
        // maps to the local-only key above.
        guard let provider else {
            preconditionFailure("nil provider must map to the local-only key")
        }
        return provider.displayName
    }
    return NSLocalizedString(
        key,
        tableName: "Core",
        bundle: .module,
        comment: ""
    )
}

extension BridgeStorageLocation {
    /// The one value that tells this location apart from another on the same
    /// provider, shown under the provider's name in compact rows.
    public var locationSummary: String {
        switch self {
        case .s3(let bucket, _, _, _, _): bucket
        case .dropbox(.appFolder, _): String(localized: "App folder")
        case .dropbox(.sharedFolder(let namespaceId), _): namespaceId
        case .oneDrive(_, let folderId, _): folderId
        case .cloudKit(let container, _, _): container
        }
    }

    /// The provider with the account or bucket a library is in, as restore
    /// rows and library rows show it: "Dropbox: ana@example.com", "S3-compatible:
    /// bucket (endpoint)", "iCloud". The endpoint is the one the person entered;
    /// Amazon S3 has none, so its region names the place instead. A Dropbox or
    /// OneDrive location coven hasn't set up has no account yet.
    public var accountLine: String {
        switch self {
        case .dropbox(_, let account?), .oneDrive(_, _, let account?):
            String(
                localized: "\(displayName): \(account)",
                comment:
                    "A storage provider and the account a library is in, such as Dropbox: ana@example.com"
            )
        case .s3(let bucket, let region, let endpoint, _, _):
            String(
                localized: "\(displayName): \(bucket) (\(endpoint ?? region))",
                comment:
                    "An S3 library's provider, bucket, and endpoint (or region, for Amazon S3)"
            )
        case .dropbox, .oneDrive, .cloudKit:
            displayName
        }
    }

    /// The signed-in account a Dropbox or OneDrive library is in, once coven
    /// has set it up; S3 and iCloud have none.
    public var accountLabel: String? {
        switch self {
        case .dropbox(_, let account), .oneDrive(_, _, let account): account
        case .s3, .cloudKit: nil
        }
    }
}
