/// What a storage admin's credentials are for, from a pending record.
public enum AdminAccessRequest {
    /// Retry an operation that needs storage admin rights.
    case retry(operation: Int64)
    /// Delete a removed member's storage access key.
    case deleteAccessKey(operation: Int64, accessKeyId: String)
}

/// A pending record's action that asks the person for more before it runs.
public enum PendingActionSheet: Identifiable {
    case adminAccess(AdminAccessRequest)
    case s3Key
    case restoreFile(BridgePendingRecord)
    case review(BridgePendingRecord)

    /// The sheet a record's action asks for, or nil when it runs at once.
    public init?(_ record: BridgePendingRecord) {
        switch record.action {
        case .restoreFile:
            self = .restoreFile(record)
        case .provideAdminAccess(let id):
            self = .adminAccess(.retry(operation: id))
        case .deleteAccessKey(let operation, let accessKeyId):
            self = .adminAccess(
                .deleteAccessKey(operation: operation, accessKeyId: accessKeyId)
            )
        case .review:
            self = .review(record)
        case .wait, .updateApp, .signIn, .reconnect, .resumeUploads,
            .retryOperation, .restoreFromCode:
            return nil
        }
    }

    public var id: String {
        switch self {
        case .adminAccess(.retry(let operation)): "admin-\(operation)"
        case .adminAccess(.deleteAccessKey(let operation, _)):
            "delete-key-\(operation)"
        case .s3Key: "s3-key"
        case .restoreFile(let record): "restore-file-\(record.hashValue)"
        case .review(let record): "review-\(record.hashValue)"
        }
    }
}

/// How a library signs in to its storage again when a pending record asks:
/// a new S3 access key, the provider's sign-in (core picks the access the
/// library's location needs), or the system account iCloud uses.
public enum StorageReauthorization {
    case s3Key
    case accountSignIn
    /// iCloud signs in through the system's settings; reconnecting picks the
    /// account up.
    case systemAccount

    public init(provider: BridgeCloudProvider) {
        switch provider {
        case .s3:
            self = .s3Key
        case .dropbox, .oneDrive:
            self = .accountSignIn
        case .cloudKit:
            self = .systemAccount
        }
    }
}

extension BridgeStorageLocation {
    /// This S3 location's connection with a new access key, after the old one
    /// stopped working. Setup checks it against the recorded location before
    /// keeping it.
    public func replacingS3Key(
        accessKey: String,
        secretKey: String
    ) -> BridgeSaveSyncConfig {
        guard
            case .s3(
                let bucket,
                let region,
                let endpoint,
                let profile,
                let prefix
            ) = self
        else {
            preconditionFailure("an S3 key is asked only for an S3 library")
        }
        return BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint,
            keyPrefix: prefix,
            accessKey: accessKey,
            secretKey: secretKey,
            profile: profile
        )
    }
}
