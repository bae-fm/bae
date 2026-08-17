import BaeKit
import SwiftUI

/// Step two of the join flow: show this device's generated code for an existing
/// member to scan or paste, and take the invite code that member hands back.
/// The owner owns the generation, decode, and scan actions; this screen renders
/// their current results and forwards input.
struct JoinCodeExchange: View {
    let joinRequest: Result<BridgeJoinRequest, Error>?
    @Binding
    var deviceInvitation: String
    let decodedInvitation: Result<BridgeDeviceInviteInfo, Error>?
    /// The OAuth token captured when the provider was picked, or `nil` for
    /// S3/iCloud. A decoded invite that needs OAuth is only joinable once this
    /// is held, so its absence drives the "go back and choose the provider"
    /// message below.
    let joinTokenJson: String?
    let error: String?
    let onRetryGenerate: () -> Void
    let onScanInvite: () -> Void
    let onInvitationChanged: (String) -> Void

    var body: some View {
        List {
            Section("This device's code") {
                joinRequestRow
            }
            Section("Invite code") {
                inviteCodeRow
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

    @ViewBuilder
    private var joinRequestRow: some View {
        switch joinRequest {
        case nil:
            ProgressView()
                .frame(maxWidth: .infinity)
        case .success(let generated):
            VStack(spacing: 12) {
                Text(
                    "On a device already in your library, open Settings \u{2192} Members \u{2192} Add a device and scan or paste this."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                CodeShareBlock(
                    code: generated.code,
                    contentDescription: "This device's join code",
                    qrSize: 180
                )

                // The approving device shows this same fingerprint; matching
                // them confirms the right device is being added.
                Text("This device: \(generated.fingerprint)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
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
            TextField("Paste invite code", text: $deviceInvitation)
                .font(.body.monospaced())
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Scan") {
                onScanInvite()
            }
        }
        .onChange(of: deviceInvitation) { _, newInput in
            onInvitationChanged(newInput)
        }

        if case .success(let info) = decodedInvitation {
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
            if info.needsOauth && joinTokenJson == nil {
                Text(
                    "This library needs a signed-in provider. Go back and choose the provider it uses."
                )
                .foregroundStyle(.red)
                .font(.callout)
            }
            #else
            if info.needsOauth {
                Text(
                    "This library uses a provider this build can't connect to."
                )
                .foregroundStyle(.red)
                .font(.callout)
            }
            #endif
        }
        else if case .failure(let decodeError) = decodedInvitation {
            Text(decodeError.displayLine ?? "")
                .foregroundStyle(.red)
                .font(.callout)
        }
    }
}

#if DEBUG
#Preview {
    JoinCodeExchange(
        joinRequest: .success(PreviewData.joinRequest),
        deviceInvitation: .constant(""),
        decodedInvitation: .success(PreviewData.inviteInfo),
        joinTokenJson: nil,
        error: nil,
        onRetryGenerate: {},
        onScanInvite: {},
        onInvitationChanged: { _ in }
    )
}
#endif
