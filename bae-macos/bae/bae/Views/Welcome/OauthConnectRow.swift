import BaeKit
import SwiftUI

#if BAE_OAUTH_PROVIDERS
    /// The connect/authorizing/connected row an OAuth provider shows in the
    /// restore screen — both the restore-code path and the manual form use it.
    /// Prop-driven: the flow view owns the token and the authorizing flag and
    /// does the work; this only renders the current state.
    struct OauthConnectRow: View {
        let provider: BridgeCloudProvider
        let isConnected: Bool
        let isAuthorizing: Bool
        let onConnect: () -> Void
        let onCancelAuth: () -> Void

        var body: some View {
            HStack {
                if isConnected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                    Text("Connected")
                        .foregroundStyle(.secondary)
                }
                else if isAuthorizing {
                    ProgressView()
                        .controlSize(.small)
                    Text("Authorizing...")
                        .foregroundStyle(.secondary)
                    Button("Cancel") {
                        onCancelAuth()
                    }
                    .buttonStyle(.borderless)
                    .font(.callout)
                }
                else {
                    Button("Connect \(provider.displayName)") {
                        onConnect()
                    }
                }
            }
        }
    }

    #if DEBUG
        #Preview("Disconnected") {
            OauthConnectRow(
                provider: .googleDrive,
                isConnected: false,
                isAuthorizing: false,
                onConnect: {},
                onCancelAuth: {},
            )
            .padding()
        }

        #Preview("Authorizing") {
            OauthConnectRow(
                provider: .googleDrive,
                isConnected: false,
                isAuthorizing: true,
                onConnect: {},
                onCancelAuth: {},
            )
            .padding()
        }

        #Preview("Connected") {
            OauthConnectRow(
                provider: .googleDrive,
                isConnected: true,
                isAuthorizing: false,
                onConnect: {},
                onCancelAuth: {},
            )
            .padding()
        }
    #endif
#endif
