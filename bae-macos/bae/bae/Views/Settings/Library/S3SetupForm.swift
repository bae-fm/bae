import BaeKit
import SwiftUI

/// S3 setup: the endpoint, region and bucket, then the access key. coven
/// knows a service by its endpoint and fills in how to talk to it. The library
/// takes the whole bucket. A failed setup leaves the fields as typed, with the
/// reason under the field it names.
struct S3SetupForm: View {
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var fields: S3SetupFields

    init(onConnected: @escaping (BridgeStorageSetup) -> Void) {
        _fields = State(initialValue: S3SetupFields())
        self.onConnected = onConnected
    }

    #if DEBUG
        /// A form whose fields are staged, for a preview or a shot.
        init(
            staged fields: S3SetupFields,
            onConnected: @escaping (BridgeStorageSetup) -> Void
        ) {
            _fields = State(initialValue: fields)
            self.onConnected = onConnected
        }
    #endif

    var body: some View {
        @Bindable
        var fields = fields
        SetupStep(
            actionTitle: "Connect",
            ready: fields.ready,
            operation: { [sync, fields] setup in
                fields.clearFailure()
                return try await sync.saveSyncConfig(fields.config, setup)
            },
            onConnected: onConnected,
            claim: { fields.place($0) },
            fields: {
                Section("Location") {
                    TextField("Endpoint", text: $fields.endpoint)
                        .textContentType(.URL)
                    TextField("Region", text: $fields.region)
                    failure(under: .region)
                    TextField("Bucket", text: $fields.bucket)
                    failure(under: .bucket)
                }
                Section("Key") {
                    TextField("Access key ID", text: $fields.accessKey)
                    SecureField("Secret access key", text: $fields.secretKey)
                }
            }
        )
    }

    /// The reason a failed setup gives for `field`, under it.
    @ViewBuilder
    private func failure(under field: S3SetupFields.Field) -> some View {
        if let failure = fields.fieldFailure, failure.field == field {
            Text(failure.error.line)
                .themeText(.detail)
                .foregroundStyle(Theme.danger)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
