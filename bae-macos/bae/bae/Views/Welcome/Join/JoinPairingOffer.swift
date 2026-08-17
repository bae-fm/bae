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
                        .font(.system(.body, design: .monospaced))
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
                    .foregroundStyle(.red)
                    .font(.callout)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
            }

            if isAuthorizing {
                progress("Signing in...")
            }
            else if isJoining {
                if let joinProgress {
                    DeviceJoinProgressView(joining: joinProgress)
                        .padding(.bottom, 12)
                }
                else {
                    progress("Starting pairing...")
                }
            }

            HStack(spacing: 12) {
                Button(isAuthorizing || isJoining ? "Cancel" : "Back") {
                    onBack()
                }
                .buttonStyle(.bordered)
                Button("Join") { onJoin() }
                    .buttonStyle(.borderedProminent)
                    .disabled(isJoining || !joinReady)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, 24)
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
                .foregroundStyle(.red)
                .font(.callout)
            }
            if let joiningFingerprint {
                LabeledContent("This device", value: joiningFingerprint)
            }
        }
        else if case .failure(let decodeError) = decodedOffer {
            Text(decodeError.displayLine ?? "")
                .foregroundStyle(.red)
                .font(.callout)
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
        HStack(spacing: 8) {
            ProgressView()
                .controlSize(.small)
            Text(title)
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .padding(.bottom, 12)
    }
}
