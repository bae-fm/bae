import BaeKit
import SwiftUI

/// One row per library whose restore code is in this Mac's keychain but which
/// isn't on this Mac yet.
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
        VStack(spacing: ThemeSpace.group) {
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
                VStack(spacing: ThemeSpace.related) {
                    HStack(spacing: ThemeSpace.related) {
                        VStack(alignment: .leading, spacing: ThemeSpace.line) {
                            Text(entry.info.libraryName)
                                .themeText(.rowTitle)
                            Text(entry.info.cloudProvider.displayName)
                                .themeText(.detail)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        // Every state's controls stay in the layout, toggled by
                        // opacity, so the row height never changes.
                        let needsConnect =
                            entry.info.needsOauth && !oauthConnected
                        let idle = !isRestoring && !isAuthorizing
                        ZStack(alignment: .trailing) {
                            ProgressView()
                                .controlSize(.small)
                                .opacity(isRestoring ? 1 : 0)
                                .allowsHitTesting(false)

                            HStack(spacing: ThemeSpace.related) {
                                ProgressView()
                                    .controlSize(.small)
                                Button("Cancel") {
                                    onCancelAuth()
                                }
                                .buttonStyle(.borderless)
                                .themeText(.body)
                            }
                            .opacity(isAuthorizing ? 1 : 0)
                            .allowsHitTesting(isAuthorizing)

                            HStack(spacing: ThemeSpace.related) {
                                ZStack(alignment: .trailing) {
                                    Button("Connect") {
                                        onConnect(entry.info)
                                    }
                                    // Disabled so it can't take focus while
                                    // hidden.
                                    .disabled(!needsConnect)
                                    .opacity(needsConnect ? 1 : 0)
                                    .allowsHitTesting(needsConnect)

                                    Button("Restore") {
                                        onRestore(entry)
                                    }
                                    .buttonStyle(PrimaryButtonStyle())
                                    .keyboardShortcut(.defaultAction)
                                    // Disabled so Enter can't fire it while
                                    // hidden or busy.
                                    .disabled(needsConnect || !idle)
                                    .opacity(needsConnect ? 0 : 1)
                                    .allowsHitTesting(!needsConnect)
                                }
                                Button(role: .destructive) {
                                    deleteConfirmCode = entry.code
                                } label: {
                                    Image(systemName: "xmark")
                                        .themeIcon(.medium)
                                }
                                .buttonStyle(.borderless)
                            }
                            .opacity(idle ? 1 : 0)
                            .allowsHitTesting(idle)
                        }
                    }
                }
                .padding(.horizontal, ThemeSpace.edge)
                .padding(.vertical, ThemeSpace.related)
                .background(Color.secondary.opacity(ThemeOpacity.tint))
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
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
