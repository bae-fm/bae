extension BridgeS3ProfileName {
    /// The service's own name, shown as is.
    public var displayName: String {
        switch self {
        case .aws: "AWS S3"
        case .gcs: "Google Cloud Storage"
        case .r2: "Cloudflare R2"
        case .rustFs: "RustFS"
        case .seaweedFs: "SeaweedFS"
        case .cephRgw: "Ceph RGW"
        case .versityGw: "Versity Gateway"
        }
    }
}
