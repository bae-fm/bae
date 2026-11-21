import BaeKit
import SwiftUI

/// S3 setup: the endpoint, region and bucket, then the access key. coven
/// knows a service by its endpoint and fills in how to talk to it. The library
/// takes the whole bucket. A failed setup leaves the fields as typed, with the
/// reason under the field it names.
struct MacS3Form: View {
    let leave: () -> Void
    let onConnected: (BridgeStorageSetup) -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var fields: S3SetupFields
    #if DEBUG
        /// The failure the form opens showing at the top, which a shot stages.
        private var stagedFailure: DisplayError?
    #endif

    init(
        leave: @escaping () -> Void,
        onConnected: @escaping (BridgeStorageSetup) -> Void
    ) {
        _fields = State(initialValue: S3SetupFields())
        self.leave = leave
        self.onConnected = onConnected
    }

    #if DEBUG
        // periphery:ignore - called by baeShots
        /// The form with `fields` typed in, any reason a failed setup gave
        /// under the field it names, and `failure` at the top.
        init(staged fields: S3SetupFields, failure: DisplayError? = nil) {
            self.stagedFailure = failure
            _fields = State(initialValue: fields)
            leave = {}
            onConnected = { _ in }
        }
    #endif

    var body: some View {
        @Bindable
        var fields = fields
        let step = SetupStep(
            ready: fields.ready,
            operation: { [sync, fields] setup in
                fields.clearFailure()
                return try await sync.saveSyncConfig(fields.config, setup)
            },
            leave: leave,
            onConnected: onConnected,
            claim: { fields.place($0) },
            describe: { error in
                S3SetupFields.keyRejection(error)
                    .map {
                        DisplayError(line: $0.words)
                    }
            },
            fields: {
                MacSection("Location") {
                    TextField("Endpoint", text: $fields.endpoint)
                        .textContentType(.URL)
                    field("Region", text: $fields.region, under: .region)
                    field("Bucket", text: $fields.bucket, under: .bucket)
                }
                MacSection("Key") {
                    TextField("Access key ID", text: $fields.accessKey)
                    SecureField("Secret access key", text: $fields.secretKey)
                }
            }
        )
        #if DEBUG
            return step.opening(on: stagedFailure)
        #else
            return step
        #endif
    }

    /// A text field whose row also holds the reason a failed setup gives for
    /// `field`, under the text.
    private func field(
        _ title: LocalizedStringKey,
        text: Binding<String>,
        under field: S3SetupFields.Field
    ) -> some View {
        MacRow(title) {
            VStack(alignment: .trailing, spacing: ThemeSpace.line) {
                TextField(title, text: text)
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                if let failure = fields.fieldFailure, failure.field == field {
                    Text(failure.error.line)
                        .themeText(.detail)
                        .foregroundStyle(Theme.danger)
                        .multilineTextAlignment(.trailing)
                }
            }
        }
    }
}

extension S3SetupFields.KeyRejection {
    /// What the form says at the top about the refused key.
    var words: String {
        switch self {
        case .notAccepted:
            String(
                localized:
                    "This key was not accepted. Check the access key ID and secret access key.",
                comment:
                    "S3 setup, at the top of the form: the service refused the access key."
            )
        case .cannotUseBucket:
            String(
                localized:
                    "This key cannot use the bucket. Give it read and write access, or use another key.",
                comment:
                    "S3 setup, at the top of the form: the key is valid but isn't allowed to use the bucket."
            )
        }
    }
}
