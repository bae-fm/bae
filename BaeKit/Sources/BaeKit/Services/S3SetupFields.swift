import Foundation
import Observation

/// The S3 setup form's state: the endpoint, region, bucket and access key.
/// coven knows a service by its endpoint and fills in how to talk to it. The
/// library takes the whole bucket. A failed setup leaves the fields as typed;
/// the failures that name one of them show under it, and a refused key shows
/// at the top.
@MainActor
@Observable
public final class S3SetupFields {
    /// A field a failed setup's reason can sit under.
    public enum Field: Equatable {
        case region
        case bucket
    }

    /// Why the service refused the access key, which no one field names: it
    /// shows at the top of the form, in words that name the key.
    public enum KeyRejection: Equatable {
        /// The service doesn't know the key, or the secret doesn't match.
        case notAccepted
        /// The key is valid but isn't allowed to use the bucket.
        case cannotUseBucket
    }

    /// A failed setup's reason and the field it sits under.
    public struct FieldFailure: Equatable {
        public let field: Field
        public let error: DisplayError

        public init(field: Field, error: DisplayError) {
            self.field = field
            self.error = error
        }
    }

    public var endpoint = ""
    public var region = ""
    public var bucket = ""
    public var accessKey = ""
    public var secretKey = ""
    public private(set) var fieldFailure: FieldFailure?

    public init() {}

    public var ready: Bool {
        !bucket.isEmpty && !region.isEmpty && !accessKey.isEmpty
            && !secretKey.isEmpty
    }

    /// The connection the form describes.
    public var config: BridgeSaveSyncConfig {
        BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            accessKey: accessKey,
            secretKey: secretKey,
            profile: nil
        )
    }

    /// Show `error` under its field when it names one: a region or endpoint
    /// that doesn't match the bucket under Region, a bucket that isn't there
    /// or that holds other files under Bucket. False for any other failure,
    /// which the step shows at the top.
    public func place(_ error: any Error) -> Bool {
        guard
            case .Diagnostic(.storage(let failure), _) = error as? BridgeError,
            let shown = DisplayError(error)
        else { return false }
        let field: Field
        switch failure {
        case .regionMismatch: field = .region
        case .containerNotFound, .ownLocationRequired: field = .bucket
        default: return false
        }
        fieldFailure = FieldFailure(field: field, error: shown)
        return true
    }

    /// The refusal of the access key `error` is, nil for any other failure.
    public static func keyRejection(_ error: any Error) -> KeyRejection? {
        guard case .Diagnostic(.storage(let failure), _) = error as? BridgeError
        else { return nil }
        switch failure {
        case .authentication: return .notAccepted
        case .permissionDenied: return .cannotUseBucket
        default: return nil
        }
    }

    public func clearFailure() {
        fieldFailure = nil
    }
}
