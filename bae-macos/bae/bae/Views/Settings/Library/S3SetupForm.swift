import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("S3SetupForm")

/// S3 setup: the bucket, region, endpoint and access key, and nothing else.
/// coven knows a service by its endpoint and fills in how to talk to it;
/// Advanced holds the key prefix, the profile, and a detection of the
/// settings for a service coven doesn't know.
struct S3SetupForm: View {
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var bucket = ""
    @State
    private var region = ""
    @State
    private var endpoint = ""
    @State
    private var accessKey = ""
    @State
    private var secretKey = ""
    @State
    private var keyPrefix = ""
    @State
    private var showAdvanced = false
    @State
    private var profile: ProfileChoice = .automatic
    @State
    private var detection: BridgeS3Detection?
    @State
    private var detectTask: Task<Void, Never>?
    @State
    private var detectError: DisplayError?

    /// How setup learns the service's behaviors.
    private enum ProfileChoice: Hashable {
        /// Known by the endpoint's host, or AWS with no endpoint.
        case automatic
        case named(BridgeS3ProfileName)
        /// What the last detection found.
        case detected
    }

    private var ready: Bool {
        !bucket.isEmpty && !region.isEmpty && !accessKey.isEmpty
            && !secretKey.isEmpty
    }

    var body: some View {
        SetupStep(
            actionTitle: "Connect",
            ready: ready,
            operation: { [sync] in try await sync.saveSyncConfig(config) },
            onConnected: onConnected
        ) {
            Section {
                TextField("Bucket", text: $bucket)
                TextField("Region", text: $region)
                TextField("Endpoint", text: $endpoint)
                    .textContentType(.URL)
            } footer: {
                Text("Leave the endpoint empty for AWS.")
            }
            Section {
                TextField("Access Key", text: $accessKey)
                SecureField("Secret Key", text: $secretKey)
            }
            Section {
                DisclosureGroup("Advanced", isExpanded: $showAdvanced) {
                    advanced
                }
            }
        }
        .onChange(of: endpoint) { clearDetection() }
        .onChange(of: bucket) { clearDetection() }
        .onDisappear { detectTask?.cancel() }
    }

    @ViewBuilder
    private var advanced: some View {
        TextField("Key Prefix", text: $keyPrefix)
        Picker("Profile", selection: $profile) {
            Text("Automatic").tag(ProfileChoice.automatic)
            ForEach(Self.profileNames, id: \.self) { name in
                Text(verbatim: name.displayName).tag(ProfileChoice.named(name))
            }
            Text("Detected settings")
                .tag(ProfileChoice.detected)
                .selectionDisabled(detection == nil)
        }
        HStack {
            Button("Detect Settings") { detect() }
                .disabled(detectTask != nil || !ready)
            if detectTask != nil {
                ProgressView()
                    .controlSize(.small)
            }
        }
        if let detectError {
            ErrorDetailDisclosure(error: detectError)
        }
        if let detection {
            DetectedSettings(detection: detection)
        }
    }

    private static let profileNames: [BridgeS3ProfileName] = [
        .aws, .gcs, .r2, .rustFs, .seaweedFs, .cephRgw, .versityGw,
    ]

    private var location: BridgeStorageLocation {
        .s3(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            profile: nil,
            prefix: keyPrefix
        )
    }

    private var config: BridgeSaveSyncConfig {
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
            keyPrefix: keyPrefix.isEmpty ? nil : keyPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            profile: chosen
        )
    }

    private func clearDetection() {
        detection = nil
        if profile == .detected { profile = .automatic }
    }

    private func detect() {
        let location = location
        let credentials = BridgeS3Credentials(
            accessKeyId: accessKey,
            secretAccessKey: secretKey
        )
        detectError = nil
        detectTask = Task {
            defer { detectTask = nil }
            do {
                detection = try await sync.detectS3Settings(
                    location,
                    credentials
                )
            }
            catch is CancellationError {
                logger.debug("S3 detection cancelled")
            }
            catch {
                logger.error(
                    "S3 detection failed: \(error.localizedDescription)"
                )
                detectError = DisplayError(error)
            }
        }
    }
}

/// What detection found: the bucket's versioning, and the settings and probe
/// results as coven recorded them, for someone setting up an unknown service.
private struct DetectedSettings: View {
    let detection: BridgeS3Detection

    var body: some View {
        switch detection.versioning() {
        case .known(let state):
            LabeledContent("Bucket versioning", value: state.label)
        case .failed(_, let detail):
            ErrorDetailDisclosure(
                error: DisplayError(
                    line: String(localized: "Couldn't read bucket versioning."),
                    detail: detail
                )
            )
        }
        DisclosureGroup("Detected settings") {
            Text(verbatim: String(describing: detection.settings()))
                .themeText(.mono)
                .textSelection(.enabled)
            ForEach(
                Array(detection.results().enumerated()),
                id: \.offset
            ) { _, result in
                Label {
                    Text(verbatim: "\(result.probe): \(result.detail)")
                        .themeText(.mono)
                } icon: {
                    Image(
                        systemName: result.held
                            ? "checkmark.circle" : "xmark.circle"
                    )
                }
            }
        }
    }
}

extension BridgeS3ProfileName {
    /// The service's own name, shown as is.
    fileprivate var displayName: String {
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

extension BridgeS3BucketVersioning {
    fileprivate var label: String {
        switch self {
        case .disabled: String(localized: "Off")
        case .enabled: String(localized: "On")
        case .suspended: String(localized: "Suspended")
        case .unavailable: String(localized: "Unavailable")
        }
    }
}
