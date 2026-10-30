import Foundation
import Observation
import os.log

private let logger = Logger.bae("S3SetupFields")

/// The S3 setup form's state: the bucket, region, endpoint and access key,
/// and under Advanced the key prefix, the profile, and a detection of the
/// settings for a service coven doesn't know by its endpoint.
@MainActor
@Observable
public final class S3SetupFields {
    /// How setup learns the service's behaviors.
    public enum ProfileChoice: Hashable {
        /// Known by the endpoint's host, or AWS with no endpoint.
        case automatic
        case named(BridgeS3ProfileName)
        /// What the last detection found.
        case detected
    }

    /// The profiles coven knows, in the order a picker lists them.
    public static let profileNames: [BridgeS3ProfileName] = [
        .aws, .gcs, .r2, .rustFs, .seaweedFs, .cephRgw, .versityGw,
    ]

    public var bucket = "" { didSet { clearDetection() } }
    public var region = ""
    public var endpoint = "" { didSet { clearDetection() } }
    public var accessKey = ""
    public var secretKey = ""
    public var keyPrefix = ""
    public var profile: ProfileChoice = .automatic
    public private(set) var detection: BridgeS3Detection?
    public private(set) var detectError: DisplayError?
    public private(set) var isDetecting = false

    @ObservationIgnored
    private var detectTask: Task<Void, Never>?
    @ObservationIgnored
    private let detectSettings:
        @Sendable (BridgeStorageLocation, BridgeS3Credentials) async throws ->
            BridgeS3Detection

    public init(
        detectSettings:
            @escaping @Sendable (BridgeStorageLocation, BridgeS3Credentials)
            async throws -> BridgeS3Detection
    ) {
        self.detectSettings = detectSettings
    }

    public var ready: Bool {
        !bucket.isEmpty && !region.isEmpty && !accessKey.isEmpty
            && !secretKey.isEmpty
    }

    /// The connection the form describes, with the chosen profile.
    public var config: BridgeSaveSyncConfig {
        let chosen: BridgeS3Profile? =
            switch profile {
            case .automatic: nil
            case .named(let name): .named(profile: name)
            case .detected:
                detection.map { .custom(settings: $0.settings()) }
            }
        return BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            keyPrefix: keyPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            profile: chosen
        )
    }

    /// Probe the bucket with the entered key and record what the service does.
    public func detect() {
        let location = BridgeStorageLocation.s3(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            profile: nil,
            prefix: keyPrefix
        )
        let credentials = BridgeS3Credentials(
            accessKeyId: accessKey,
            secretAccessKey: secretKey
        )
        let detectSettings = detectSettings
        detectError = nil
        isDetecting = true
        detectTask = Task {
            defer {
                detectTask = nil
                isDetecting = false
            }
            do {
                detection = try await detectSettings(location, credentials)
            }
            catch is CancellationError {
                logger.debug("S3 detection cancelled")
            }
            catch {
                logger.error(
                    "S3 detection failed: \(String(reflecting: error))"
                )
                detectError = DisplayError(error)
            }
        }
    }

    public func cancelDetection() {
        detectTask?.cancel()
    }

    /// A detection belongs to the bucket and endpoint it probed.
    private func clearDetection() {
        detection = nil
        if profile == .detected { profile = .automatic }
    }
}
