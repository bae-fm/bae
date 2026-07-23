import BaeKit
import SwiftUI

/// Step one of the join flow: pick the cloud provider the target library uses.
/// For an OAuth provider the flow view authenticates up front so the account
/// email is baked into the join-request; for S3/iCloud it generates the code
/// with no email. Prop-driven — the flow view owns the selection and the
/// authorizing state; this only offers the buttons.
struct JoinProviderPicker: View {
    let providers: [BridgeCloudProvider]
    let isAuthorizing: Bool
    let error: String?
    let onSelect: (BridgeCloudProvider) -> Void
    let onCancel: () -> Void
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Text(
                "Choose the cloud provider the library you're joining uses."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .padding(.bottom, 16)

            if isAuthorizing {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Signing in...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Button("Cancel") {
                        onCancel()
                    }
                    .buttonStyle(.borderless)
                    .font(.callout)
                }
                .padding(.bottom, 12)
            }
            else {
                VStack(spacing: 8) {
                    ForEach(providers, id: \.self) { provider in
                        Button(provider.displayName) {
                            onSelect(provider)
                        }
                        .buttonStyle(.bordered)
                        .frame(maxWidth: .infinity)
                    }
                }
                .padding(.bottom, 8)
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
            }

            Button("Back") {
                onBack()
            }
            .buttonStyle(.bordered)
            .disabled(isAuthorizing)
            .padding(.bottom, 24)
        }
    }
}

#if DEBUG
    #Preview("Choose provider") {
        JoinProviderPicker(
            providers: availableCloudProviders(),
            isAuthorizing: false,
            error: nil,
            onSelect: { _ in },
            onCancel: {},
            onBack: {},
        )
        .padding()
    }

    #Preview("Signing in") {
        JoinProviderPicker(
            providers: availableCloudProviders(),
            isAuthorizing: true,
            error: nil,
            onSelect: { _ in },
            onCancel: {},
            onBack: {},
        )
        .padding()
    }

    #Preview("Error") {
        JoinProviderPicker(
            providers: availableCloudProviders(),
            isAuthorizing: false,
            error: "Sign-in failed. Try again.",
            onSelect: { _ in },
            onCancel: {},
            onBack: {},
        )
        .padding()
    }
#endif
