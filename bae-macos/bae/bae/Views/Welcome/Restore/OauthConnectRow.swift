import BaeKit
import SwiftUI

#if BAE_OAUTH_PROVIDERS
    /// An OAuth provider's connect, authorizing, or connected state on the
    /// restore screen.
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
                        .foregroundStyle(Theme.success)
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
                    .themeText(.body)
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
