import BaeKit
import SwiftUI

struct JoinPairingOffer: View {
    @Binding
    var pairingCode: String
    let decodedOffer: Result<BridgeDevicePairingOffer, Error>?
    let isAuthorizing: Bool
    let error: String?
    let onScan: () -> Void
    let onCodeChanged: (String) -> Void

    var body: some View {
        List {
            Section("Pairing code") {
                Text("Scan the code shown by a device already in your library.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                HStack(spacing: 8) {
                    TextField("Paste pairing code", text: $pairingCode)
                        .font(.body.monospaced())
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    Button("Scan") { onScan() }
                }
                .onChange(of: pairingCode) { _, value in
                    onCodeChanged(value)
                }

                if case .success(let offer) = decodedOffer {
                    LabeledContent("Library", value: offer.libraryName)
                    LabeledContent("Provider", value: offer.cloudProvider.displayName)
                    if offer.needsOauth && !oauthProvidersAvailable {
                        Text("This library uses a provider this build can't connect to.")
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                }
                // Unwrapped in the pattern rather than defaulted to "": a
                // decode is a synchronous parse, so core never reports it as a
                // cancellation and a line is always there — and if that
                // changed, this shows nothing rather than a blank red line.
                else if case .failure(let decodeError) = decodedOffer,
                    let line = decodeError.displayLine
                {
                    Text(line)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            }

            if isAuthorizing {
                Section {
                    ProgressView("Signing in...")
                }
            }
            if let error {
                Section {
                    Text(error)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            }
        }
    }

    private var oauthProvidersAvailable: Bool {
        #if BAE_OAUTH_PROVIDERS
        true
        #else
        false
        #endif
    }
}
