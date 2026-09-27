import BaeKit
import SwiftUI

struct JoinPairingOffer: View {
    @Binding
    var pairingCodeInput: String
    let decodedOffer: Result<BridgeDevicePairingOffer, Error>?
    let isAuthorizing: Bool
    let isJoining: Bool
    let joiningFingerprint: String?
    let joinProgress: BridgeJoiningDeviceJoinProgress?
    let error: String?
    let joinReady: Bool
    let onScan: () -> Void
    let onJoin: () -> Void
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section("Pairing code") {
                    TextField("Paste pairing code", text: $pairingCodeInput)
                        .themeText(.mono)
                        .disabled(isAuthorizing || isJoining)
                    Button("Scan") { onScan() }
                        .disabled(isAuthorizing || isJoining)

                    offerPreview
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)

            if let error {
                Text(error)
                    .foregroundStyle(Theme.danger)
                    .themeText(.body)
                    .padding(.horizontal)
                    .padding(.bottom, ThemeSpace.related)
            }

            if isAuthorizing {
                progress("Signing in...")
            }
            else if isJoining {
                if let joinProgress {
                    DeviceJoinProgressView(joining: joinProgress)
                        .padding(.bottom, ThemeSpace.group)
                }
                else {
                    progress("Starting pairing...")
                }
            }

            HStack(spacing: ThemeSpace.group) {
                Button(isAuthorizing || isJoining ? "Cancel" : "Back") {
                    onBack()
                }
                .buttonStyle(.bordered)
                Button("Join") { onJoin() }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(isJoining || !joinReady)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, ThemeSpace.section)
        }
    }

    @ViewBuilder
    private var offerPreview: some View {
        if case .success(let offer) = decodedOffer {
            LabeledContent("Library", value: offer.libraryName)
            LabeledContent("Provider", value: offer.cloudProvider.displayName)
            if offer.needsOauth && !oauthProvidersAvailable {
                Text(
                    "This library uses a provider this build can't connect to."
                )
                .foregroundStyle(Theme.danger)
                .themeText(.body)
            }
            if let joiningFingerprint {
                LabeledContent("This device", value: joiningFingerprint)
            }
        }
        // A decode is never cancelled, so it always has a line.
        else if case .failure(let decodeError) = decodedOffer,
            let line = decodeError.displayLine
        {
            Text(line)
                .foregroundStyle(Theme.danger)
                .themeText(.body)
        }
    }

    private var oauthProvidersAvailable: Bool {
        #if BAE_OAUTH_PROVIDERS
            true
        #else
            false
        #endif
    }

    private func progress(_ title: LocalizedStringKey) -> some View {
        HStack(spacing: ThemeSpace.related) {
            ProgressView()
                .controlSize(.small)
            Text(title)
                .themeText(.body)
                .foregroundStyle(.secondary)
        }
        .padding(.bottom, ThemeSpace.group)
    }
}
