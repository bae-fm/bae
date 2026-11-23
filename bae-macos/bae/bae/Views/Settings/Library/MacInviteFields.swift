import BaeKit
import SwiftUI
import os.log

/// The invite form: one field for the account email, or on S3 the two key
/// fields with a line saying why, the role and how long the invite lasts, and
/// Create invite. On S3 a button beside
/// the footer switches between a key made for the invited person and this
/// device's own.
struct MacInviteFields: View {
    let storage: InviteStorage
    @Binding
    var email: String
    @Binding
    var accessKeyId: String
    @Binding
    var secretAccessKey: String
    @Binding
    var usesOwnKey: Bool
    @Binding
    var role: BridgeMemberRole
    @Binding
    var lifetime: BridgeInviteLifetime
    let error: DisplayError?
    let canCreate: Bool
    let onCreate: () -> Void
    let onCancel: () -> Void

    @FocusState
    private var focused: Bool

    var body: some View {
        MacSheet("Invite a person", width: 440) {
            VStack(
                alignment: .leading,
                spacing: ThemeSpace.related + ThemeSpace.line
            ) {
                fields
                choices
                if let error {
                    MacNotice(error: error)
                }
            }
        } footer: {
            MacSheetFooter(
                lead: {
                    if storage == .s3 {
                        if usesOwnKey {
                            MacButton("Use their own key instead") {
                                usesOwnKey = false
                            }
                        }
                        else {
                            MacButton("Use my key instead") {
                                usesOwnKey = true
                            }
                        }
                    }
                },
                trailing: {
                    MacButton("Cancel", action: onCancel)
                        .keyboardShortcut(.cancelAction)
                    MacButton("Create invite", kind: .primary, action: onCreate)
                        .keyboardShortcut(.defaultAction)
                        .disabled(!canCreate)
                }
            )
        }
        .onAppear { focused = true }
    }

    @ViewBuilder
    private var fields: some View {
        switch storage {
        case .dropbox:
            emailField(
                "Dropbox email",
                why:
                    "The library is shared with this Dropbox account so they can join."
            )
        case .oneDrive:
            emailField(
                "Microsoft account email",
                why:
                    "The library is shared with this Microsoft account so they can join."
            )
        case .iCloud:
            emailField(
                "Apple Account email",
                why:
                    "The library is shared with this Apple Account so they can join."
            )
        case .s3:
            if usesOwnKey {
                MacNote(
                    "They will use your key. To remove them later, you make a new key for yourself and enter it on each of your devices."
                )
            }
            else {
                FieldBox {
                    FieldBoxRow("Access key ID", labelWidth: labelWidth) {
                        BoxedField(
                            text: $accessKeyId,
                            focus: $focused,
                            equals: true
                        )
                        .accessibilityLabel("Access key ID")
                    }
                    FieldBoxDivider()
                    FieldBoxRow(
                        "Secret access key",
                        labelWidth: labelWidth,
                        note:
                            "Their own key to the S3 storage, so they can join."
                    ) {
                        BoxedField(
                            text: $secretAccessKey,
                            secret: true
                        )
                        .accessibilityLabel("Secret access key")
                    }
                }
            }
        }
    }

    /// The width of the labels' column: the S3 labels are the shorter.
    private var labelWidth: CGFloat { storage == .s3 ? 120 : 150 }

    private var choices: some View {
        let roles: [BridgeMemberRole] = [.member, .admin]
        let lifetimes: [BridgeInviteLifetime] = [.sevenDays, .thirtyDays]
        return FieldBox {
            FieldBoxRow("Role", labelWidth: labelWidth) {
                MacSegmentedControl(
                    selectedIndex: Binding(
                        get: { roles.firstIndex(of: role) ?? 0 },
                        set: { role = roles[$0] }
                    ),
                    segments: [
                        .init(
                            label: String(localized: "Member"),
                            systemImage: nil
                        ),
                        .init(
                            label: String(localized: "Admin"),
                            systemImage: nil
                        ),
                    ],
                    fillsWidth: true
                )
            }
            FieldBoxDivider()
            FieldBoxRow("Invite lasts", labelWidth: labelWidth) {
                MacSegmentedControl(
                    selectedIndex: Binding(
                        get: { lifetimes.firstIndex(of: lifetime) ?? 0 },
                        set: { lifetime = lifetimes[$0] }
                    ),
                    segments: [
                        .init(
                            label: String(localized: "7 days"),
                            systemImage: nil
                        ),
                        .init(
                            label: String(localized: "30 days"),
                            systemImage: nil
                        ),
                    ],
                    fillsWidth: true
                )
            }
        }
    }

    private func emailField(
        _ label: LocalizedStringKey,
        why: LocalizedStringKey
    ) -> some View {
        FieldBox {
            FieldBoxRow(label, labelWidth: labelWidth, note: why) {
                BoxedField(
                    text: $email,
                    focus: $focused,
                    equals: true
                )
                .accessibilityLabel(Text(label))
                .textContentType(.emailAddress)
            }
        }
    }
}
