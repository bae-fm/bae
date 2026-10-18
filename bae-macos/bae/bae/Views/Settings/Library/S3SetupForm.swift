import BaeKit
import SwiftUI

/// S3 setup: the bucket, region, endpoint and access key, and nothing else.
/// coven knows a service by its endpoint and fills in how to talk to it;
/// Advanced holds the key prefix, the profile, and a detection of the
/// settings for a service coven doesn't know.
struct S3SetupForm: View {
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var fields: S3SetupFields
    @State
    private var showAdvanced = false

    init(
        detectSettings:
            @escaping @Sendable (BridgeStorageLocation, BridgeS3Credentials)
            async throws -> BridgeS3Detection,
        onConnected: @escaping (BridgeStorageSetup) -> Void
    ) {
        _fields = State(
            initialValue: S3SetupFields(detectSettings: detectSettings)
        )
        self.onConnected = onConnected
    }

    var body: some View {
        @Bindable
        var fields = fields
        SetupStep(
            actionTitle: "Connect",
            ready: fields.ready,
            operation: { [sync, fields] in
                try await sync.saveSyncConfig(fields.config)
            },
            onConnected: onConnected
        ) {
            Section {
                TextField("Bucket", text: $fields.bucket)
                TextField("Region", text: $fields.region)
                TextField("Endpoint", text: $fields.endpoint)
                    .textContentType(.URL)
            } footer: {
                Text("Leave the endpoint empty for AWS.")
            }
            Section {
                TextField("Access Key", text: $fields.accessKey)
                SecureField("Secret Key", text: $fields.secretKey)
            }
            Section {
                DisclosureGroup("Advanced", isExpanded: $showAdvanced) {
                    advanced
                }
            }
        }
        .onDisappear { fields.cancelDetection() }
    }

    @ViewBuilder
    private var advanced: some View {
        @Bindable
        var fields = fields
        TextField("Key Prefix", text: $fields.keyPrefix)
        Picker("Profile", selection: $fields.profile) {
            Text("Automatic").tag(S3SetupFields.ProfileChoice.automatic)
            ForEach(S3SetupFields.profileNames, id: \.self) { name in
                Text(verbatim: name.displayName)
                    .tag(S3SetupFields.ProfileChoice.named(name))
            }
            Text("Detected settings")
                .tag(S3SetupFields.ProfileChoice.detected)
                .selectionDisabled(fields.detection == nil)
        }
        HStack {
            Button("Detect Settings") { fields.detect() }
                .disabled(fields.isDetecting || !fields.ready)
            if fields.isDetecting {
                ProgressView()
                    .controlSize(.small)
            }
        }
        if let detectError = fields.detectError {
            ErrorDetailDisclosure(error: detectError)
        }
        if let detection = fields.detection {
            S3DetectedSettings(detection: detection)
        }
    }
}
