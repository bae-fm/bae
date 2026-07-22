import BaeKit
import SwiftUI

/// The manual restore form's fields as one value. One value answers both
/// questions the form asks — whether Restore may be pressed, and what to
/// restore — so the two cannot disagree about what a provider needs. An OAuth
/// provider carries a `nil` token until the user authorizes; the core rule
/// reads that as "not ready yet".
struct ManualRestoreDraft {
    var provider: BridgeCloudProvider = .s3
    var libraryId = ""
    var libraryName = ""
    var encryptionKey = ""
    // S3
    var bucket = ""
    var region = ""
    var endpoint = ""
    var accessKey = ""
    var secretKey = ""
    /// Google Drive
    var googleDriveFolderId = ""
    /// Dropbox
    var dropboxFolderPath = ""
    // OneDrive
    var oneDriveDriveId = ""
    var oneDriveFolderId = ""

    /// The restore configuration the form currently describes, with the OAuth
    /// token the flow view holds folded in where the provider needs one.
    func buildRestoreConfig(oauthTokenJson: String?) -> BridgeRestoreConfig {
        BridgeRestoreConfig(
            libraryId: libraryId,
            encryptionKey: encryptionKey,
            home: buildRestoreHome(oauthTokenJson: oauthTokenJson),
        )
    }

    func buildRestoreHome(oauthTokenJson: String?) -> BridgeRestoreHome {
        switch provider {
        case .s3:
            .s3(
                bucket: bucket,
                region: region,
                endpoint: endpoint.isEmpty ? nil : endpoint,
                accessKey: accessKey,
                secretKey: secretKey,
            )
        case .cloudKit:
            .cloudKit
        case .googleDrive:
            .googleDrive(
                folderId: googleDriveFolderId,
                oauthTokenJson: oauthTokenJson,
            )
        case .dropbox:
            .dropbox(
                folderPath: dropboxFolderPath,
                oauthTokenJson: oauthTokenJson,
            )
        case .oneDrive:
            .oneDrive(
                driveId: oneDriveDriveId,
                folderId: oneDriveFolderId,
                oauthTokenJson: oauthTokenJson,
            )
        }
    }
}

/// The manual-entry fields for restoring a library: provider picker, the
/// library id / key / name, and the provider-specific fields (S3 credentials,
/// or the OAuth connect row plus its folder id). The flow view owns the draft
/// and the OAuth state; this renders the fields into the enclosing `Form`.
struct ManualRestoreForm: View {
    @Binding
    var draft: ManualRestoreDraft
    let oauthConnected: Bool
    let isAuthorizing: Bool
    let onConnect: (BridgeCloudProvider) -> Void
    let onCancelAuth: () -> Void

    var body: some View {
        // The provider choices come from the compiled-in set, so a baeium
        // (S3-only) build offers just S3 and never references an OAuth/CloudKit
        // bridge symbol that isn't there.
        Picker("Cloud provider", selection: $draft.provider) {
            ForEach(availableCloudProviders(), id: \.self) { provider in
                Text(provider.displayName).tag(provider)
            }
        }
        TextField("Library ID", text: $draft.libraryId)
            .textContentType(.none)
            .help("The UUID from your other device's library")
        SecureField("Encryption Key", text: $draft.encryptionKey)
            .help("64-character hex-encoded encryption key")
        TextField("Library Name (optional)", text: $draft.libraryName)
        providerFields
    }

    @ViewBuilder
    private var providerFields: some View {
        switch draft.provider {
        case .s3:
            TextField("Bucket", text: $draft.bucket)
            TextField("Region", text: $draft.region)
            TextField("Endpoint (optional)", text: $draft.endpoint)
                .help("Leave empty for standard AWS S3")
            SecureField("Access Key", text: $draft.accessKey)
            SecureField("Secret Key", text: $draft.secretKey)
        case .cloudKit:
            EmptyView()
        case .googleDrive:
            #if BAE_OAUTH_PROVIDERS
                oauthConnectRow
                if oauthConnected {
                    TextField("Folder ID", text: $draft.googleDriveFolderId)
                        .help(
                            "The Google Drive folder ID containing your library"
                        )
                }
            #else
                EmptyView()
            #endif
        case .dropbox:
            #if BAE_OAUTH_PROVIDERS
                oauthConnectRow
                if oauthConnected {
                    TextField("Folder Path", text: $draft.dropboxFolderPath)
                        .help("e.g. /Apps/bae/My Library")
                }
            #else
                EmptyView()
            #endif
        case .oneDrive:
            #if BAE_OAUTH_PROVIDERS
                oauthConnectRow
                if oauthConnected {
                    TextField("Drive ID", text: $draft.oneDriveDriveId)
                    TextField("Folder ID", text: $draft.oneDriveFolderId)
                }
            #else
                EmptyView()
            #endif
        }
    }

    #if BAE_OAUTH_PROVIDERS
        private var oauthConnectRow: some View {
            OauthConnectRow(
                provider: draft.provider,
                isConnected: oauthConnected,
                isAuthorizing: isAuthorizing,
                onConnect: { onConnect(draft.provider) },
                onCancelAuth: onCancelAuth,
            )
        }
    #endif
}

#if DEBUG
    #Preview {
        @Previewable
        @State
        var draft = ManualRestoreDraft()
        Form {
            ManualRestoreForm(
                draft: $draft,
                oauthConnected: false,
                isAuthorizing: false,
                onConnect: { _ in },
                onCancelAuth: {},
            )
        }
        .formStyle(.grouped)
    }
#endif
