import BaeKit
import SwiftUI

/// The keychain-restore rows on the choose screen: one row per library whose
/// restore code was found in this Mac's keychain but isn't on the device yet.
/// Prop-driven — the flow view owns the restore/authorize state and does the
/// work; this only renders the rows and holds the delete-confirmation dialog,
/// which is local UI state.
struct KeychainRestoreSection: View {
    let entries: [(code: String, info: BridgeRestoreCodeInfo)]
    let isRestoring: Bool
    let isAuthorizing: Bool
    let oauthConnected: Bool
    let onRestore: ((code: String, info: BridgeRestoreCodeInfo)) -> Void
    let onConnect: (BridgeRestoreCodeInfo) -> Void
    let onCancelAuth: () -> Void
    let onDelete: (String) -> Void

    @State
    private var deleteConfirmCode: String?

    var body: some View {
        VStack(spacing: 12) {
            WelcomeSectionHeader(
                title: "Restore from this Mac's keychain",
                infoTip: InfoTip(
                    text: "Found from a previous setup on this Mac.",
                    learnMoreURL: URL(string: "https://bae.fm/sync/restore"),
                ),
            )
            ForEach(Array(entries.enumerated()), id: \.offset) {
                _,
                entry in
                VStack(spacing: 8) {
                    HStack(spacing: 8) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(entry.info.libraryName)
                                .font(.body.bold())
                            Text(entry.info.cloudProvider.displayName)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        // Restoring, authorizing, and idle controls all stay in
                        // the row's layout tree, toggled by opacity, so the row
                        // height doesn't change as the keychain entry's state
                        // flips between them.
                        let needsConnect =
                            entry.info.needsOauth && !oauthConnected
                        let idle = !isRestoring && !isAuthorizing
                        ZStack(alignment: .trailing) {
                            ProgressView()
                                .controlSize(.small)
                                .opacity(isRestoring ? 1 : 0)
                                .allowsHitTesting(false)

                            HStack(spacing: 8) {
                                ProgressView()
                                    .controlSize(.small)
                                Button("Cancel") {
                                    onCancelAuth()
                                }
                                .buttonStyle(.borderless)
                                .font(.callout)
                            }
                            .opacity(isAuthorizing ? 1 : 0)
                            .allowsHitTesting(isAuthorizing)

                            HStack(spacing: 8) {
                                ZStack(alignment: .trailing) {
                                    // The provider is named in the row's
                                    // caption, so the button doesn't repeat it
                                    // — the long form was what truncated the
                                    // library name at the section's width.
                                    Button("Connect") {
                                        onConnect(entry.info)
                                    }
                                    // Disabled (not just hidden) when it isn't
                                    // the active control, so it can't take Tab
                                    // focus while invisible.
                                    .disabled(!needsConnect)
                                    .opacity(needsConnect ? 1 : 0)
                                    .allowsHitTesting(needsConnect)

                                    Button("Restore") {
                                        onRestore(entry)
                                    }
                                    .buttonStyle(PrimaryButtonStyle())
                                    .keyboardShortcut(.defaultAction)
                                    // Disabled (not just hidden) when Connect is
                                    // the active control or a restore is running,
                                    // so the default-action shortcut can't fire
                                    // the hidden button on Enter.
                                    .disabled(needsConnect || !idle)
                                    .opacity(needsConnect ? 0 : 1)
                                    .allowsHitTesting(!needsConnect)
                                }
                                Button(role: .destructive) {
                                    deleteConfirmCode = entry.code
                                } label: {
                                    Image(systemName: "xmark")
                                        .font(.callout)
                                }
                                .buttonStyle(.borderless)
                            }
                            .opacity(idle ? 1 : 0)
                            .allowsHitTesting(idle)
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Color.secondary.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
        .confirmationDialog(
            "Remove this library from your keychain?",
            isPresented: Binding(
                get: { deleteConfirmCode != nil },
                set: { if !$0 { deleteConfirmCode = nil } },
            ),
            titleVisibility: .visible,
        ) {
            Button("Remove", role: .destructive) {
                if let code = deleteConfirmCode {
                    onDelete(code)
                }
                deleteConfirmCode = nil
            }
        } message: {
            Text(
                "You will not be able to recover this library without a restore code."
            )
        }
    }
}

#if DEBUG
    #Preview("Idle") {
        KeychainRestoreSection(
            entries: PreviewData.welcomeKeychainEntries,
            isRestoring: false,
            isAuthorizing: false,
            oauthConnected: false,
            onRestore: { _ in },
            onConnect: { _ in },
            onCancelAuth: {},
            onDelete: { _ in },
        )
        .padding()
    }

    #Preview("Authorizing") {
        KeychainRestoreSection(
            entries: PreviewData.welcomeKeychainEntries,
            isRestoring: false,
            isAuthorizing: true,
            oauthConnected: false,
            onRestore: { _ in },
            onConnect: { _ in },
            onCancelAuth: {},
            onDelete: { _ in },
        )
        .padding()
    }

    #Preview("Restoring") {
        KeychainRestoreSection(
            entries: PreviewData.welcomeKeychainEntries,
            isRestoring: true,
            isAuthorizing: false,
            oauthConnected: false,
            onRestore: { _ in },
            onConnect: { _ in },
            onCancelAuth: {},
            onDelete: { _ in },
        )
        .padding()
    }
#endif
