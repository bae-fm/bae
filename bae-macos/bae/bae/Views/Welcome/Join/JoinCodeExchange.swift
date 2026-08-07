import BaeKit
import SwiftUI

/// Step two of the join flow: show this device's code and take the invite code
/// the approving device hands back. Reached only after a provider is picked, so
/// the code is already generated (with the account email for an OAuth provider).
/// Prop-driven — the flow view owns the generated code, the decode, and the
/// join; this renders them and reports the button taps.
struct JoinCodeExchange: View {
    /// What a provider mismatch tells the joiner, per build flavor. Both
    /// sentences compile in every flavor — only the selection is gated — so
    /// single-flavor builds still extract (and the string gate still sees)
    /// both catalog keys.
    private static let oauthMismatchLine: LocalizedStringKey =
        "This library needs a signed-in provider. Go back and choose the provider it uses."
    private static let noProviderLine: LocalizedStringKey =
        "This library uses a provider this build can't connect to."
    #if BAE_OAUTH_PROVIDERS
        private static let hasOauthProviders = true
    #else
        private static let hasOauthProviders = false
    #endif
    private static let providerMismatchLine =
        hasOauthProviders ? oauthMismatchLine : noProviderLine

    let joinRequest: Result<BridgeJoinRequest, Error>?
    @Binding
    var inviteCodeInput: String
    let decodedInvite: Result<BridgeInviteCodeInfo, Error>?
    let oauthConnected: Bool
    let isJoining: Bool
    let error: String?
    let joinReady: Bool
    let onRetryGenerate: () -> Void
    let onScan: () -> Void
    let onJoin: () -> Void
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section("This device's code") {
                    joinRequestRow
                }
                Section("Invite code") {
                    inviteCodeRow
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
            if isJoining {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Joining library...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 12)
            }
            HStack(spacing: 12) {
                Button("Back") {
                    onBack()
                }
                .buttonStyle(.bordered)
                .disabled(isJoining)
                Button("Join") { onJoin() }
                    .buttonStyle(.borderedProminent)
                    .disabled(isJoining || !joinReady)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.bottom, 24)
        }
    }

    @ViewBuilder
    private var joinRequestRow: some View {
        switch joinRequest {
        case nil:
            ProgressView()
                .frame(maxWidth: .infinity)
        case .success(let generated):
            VStack(spacing: 12) {
                Text(
                    "On a device already in your library, open Settings → Members → Add a device and scan or paste this."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                CodeDisplay(
                    code: generated.code,
                    qrSize: 160,
                    deviceFingerprint: generated.fingerprint
                )
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 4)
        case .failure(let genError):
            VStack(spacing: 8) {
                Text(genError.displayLine ?? "")
                    .foregroundStyle(.red)
                    .font(.callout)
                Button("Try again") {
                    onRetryGenerate()
                }
            }
            .frame(maxWidth: .infinity)
        }
    }

    @ViewBuilder
    private var inviteCodeRow: some View {
        Text(
            "Once that device approves this one, enter the invite code it shows."
        )
        .font(.caption)
        .foregroundStyle(.secondary)

        HStack(spacing: 8) {
            TextField("Paste invite code", text: $inviteCodeInput)
                .font(.system(.body, design: .monospaced))
            Button("Scan") { onScan() }
        }

        if case .success(let info) = decodedInvite {
            LabeledContent("Library", value: info.libraryName)
            LabeledContent(
                "Provider",
                value: info.cloudProvider.displayName
            )
            LabeledContent(
                "Owner",
                value: info.ownerFingerprint
            )
            #if BAE_OAUTH_PROVIDERS
                // OAuth is done up front at provider selection, so a token is
                // already held. A missing token here means the joiner picked a
                // provider that doesn't match this library — send them back.
                if info.needsOauth && !oauthConnected {
                    Text(Self.providerMismatchLine)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            #else
                if info.needsOauth {
                    Text(Self.providerMismatchLine)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            #endif
        }
        else if case .failure(let decodeError) = decodedInvite {
            Text(decodeError.displayLine ?? "")
                .foregroundStyle(.red)
                .font(.callout)
        }
    }
}

#if DEBUG
    #Preview("Ready to join") {
        @Previewable
        @State
        var inviteCodeInput = ""
        JoinCodeExchange(
            joinRequest: .success(PreviewData.welcomeJoinRequest),
            inviteCodeInput: $inviteCodeInput,
            decodedInvite: .success(PreviewData.welcomeInviteInfo),
            oauthConnected: false,
            isJoining: false,
            error: nil,
            joinReady: true,
            onRetryGenerate: {},
            onScan: {},
            onJoin: {},
            onBack: {},
        )
    }

    #Preview("Joining") {
        @Previewable
        @State
        var inviteCodeInput = "invite-code"
        JoinCodeExchange(
            joinRequest: .success(PreviewData.welcomeJoinRequest),
            inviteCodeInput: $inviteCodeInput,
            decodedInvite: .success(PreviewData.welcomeInviteInfo),
            oauthConnected: false,
            isJoining: true,
            error: nil,
            joinReady: true,
            onRetryGenerate: {},
            onScan: {},
            onJoin: {},
            onBack: {},
        )
    }

    #Preview("Code generation failed") {
        @Previewable
        @State
        var inviteCodeInput = ""
        JoinCodeExchange(
            joinRequest: .failure(
                PreviewData.welcomeFailure(
                    "Could not reach the cloud provider."
                )
            ),
            inviteCodeInput: $inviteCodeInput,
            decodedInvite: nil,
            oauthConnected: false,
            isJoining: false,
            error: nil,
            joinReady: false,
            onRetryGenerate: {},
            onScan: {},
            onJoin: {},
            onBack: {},
        )
    }

    #Preview("Invalid invite code") {
        @Previewable
        @State
        var inviteCodeInput = "not-a-code"
        JoinCodeExchange(
            joinRequest: .success(PreviewData.welcomeJoinRequest),
            inviteCodeInput: $inviteCodeInput,
            decodedInvite: .failure(
                PreviewData.welcomeFailure("This code isn't an invite code.")
            ),
            oauthConnected: false,
            isJoining: false,
            error: nil,
            joinReady: false,
            onRetryGenerate: {},
            onScan: {},
            onJoin: {},
            onBack: {},
        )
    }

    #Preview("Provider not signed in") {
        @Previewable
        @State
        var inviteCodeInput = "invite-code"
        JoinCodeExchange(
            joinRequest: .success(PreviewData.welcomeJoinRequest),
            inviteCodeInput: $inviteCodeInput,
            decodedInvite: .success(PreviewData.welcomeInviteInfoOauth),
            oauthConnected: false,
            isJoining: false,
            error: nil,
            joinReady: false,
            onRetryGenerate: {},
            onScan: {},
            onJoin: {},
            onBack: {},
        )
    }
#endif
