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
                    .themeText(.detail)
                    .foregroundStyle(.secondary)

                HStack(spacing: ThemeSpace.related) {
                    TextField("Paste pairing code", text: $pairingCode)
                        .themeText(.mono)
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
                        ErrorText(
                            String(
                                localized:
                                    "This library uses a provider this build can't connect to."
                            )
                        )
                    }
                }
                // A decode never reports a cancellation, so this always has a
                // line; if not, it shows nothing instead of a blank error.
                else if case .failure(let decodeError) = decodedOffer,
                    let displayed = DisplayError(decodeError)
                {
                    ErrorDetailDisclosure(error: displayed)
                }
            }

            if isAuthorizing {
                Section {
                    ProgressView("Signing in...")
                }
            }
            if let error {
                Section {
                    ErrorText(error)
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
