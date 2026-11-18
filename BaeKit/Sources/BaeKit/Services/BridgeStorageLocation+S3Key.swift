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
                let profile
            ) = self
        else {
            preconditionFailure("an S3 key is asked only for an S3 library")
        }
        return BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint,
            accessKey: accessKey,
            secretKey: secretKey,
            profile: profile
        )
    }
}
