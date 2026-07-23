import BaeKit
import SwiftUI

/// Step one of the join flow: pick the cloud provider the target library uses.
/// The owner decides what selecting a provider does (an OAuth provider signs in
/// up front; S3/iCloud go straight to generating a code) and shows a spinner
/// while that runs.
struct JoinProviderPicker: View {
    let providers: [BridgeCloudProvider]
    let isAuthorizing: Bool
    let error: String?
    let onSelect: (BridgeCloudProvider) -> Void

    var body: some View {
        List {
            Section {
                if isAuthorizing {
                    HStack(spacing: 8) {
                        ProgressView()
                        Text("Signing in...")
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity)
                }
                else {
                    ForEach(providers, id: \.self) { provider in
                        Button(provider.displayName) {
                            onSelect(provider)
                        }
                    }
                }
            } header: {
                Text("Choose the cloud provider the library you're joining uses.")
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
}

#if DEBUG
#Preview {
    JoinProviderPicker(
        providers: PreviewData.cloudProviders,
        isAuthorizing: false,
        error: nil,
        onSelect: { _ in }
    )
}
#endif
