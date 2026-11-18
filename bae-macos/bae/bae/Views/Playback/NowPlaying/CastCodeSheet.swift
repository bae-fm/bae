import BaeKit
import SwiftUI

/// The sheet that asks for the PIN or password an AirPlay receiver wants
/// while a cast to it connects: core's line under the title, the PIN's boxes
/// or a password field, and core's line under the field when the last code
/// was wrong. Once the person presses Pair, the field and Pair turn off and
/// the sheet shows Pairing… until core asks again or the cast ends.
struct CastCodeSheet: View {
    let sheet: CastStart.CodeSheet
    let onPair: (String) -> Void
    let onCancel: () -> Void

    /// What the person typed. A wrong code stays in the field, under core's
    /// line saying it was wrong.
    @State
    private var code = ""
    /// Whether the person changed the code since core last asked, which
    /// takes away core's line about the code it refused.
    @State
    private var editedSinceAsked = false

    /// The sheet for `sheet`, its field holding `typed`.
    init(
        sheet: CastStart.CodeSheet,
        typed: String = "",
        onPair: @escaping (String) -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.sheet = sheet
        self.onPair = onPair
        self.onCancel = onCancel
        _code = State(initialValue: typed)
    }

    var body: some View {
        MacSheet(
            title: Text("Pair with \(sheet.receiver.name)"),
            message: Text(
                verbatim: sheet.request.localizedMessage(
                    receiver: sheet.receiver.name
                )
            ),
            width: 420
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                field
                    .disabled(sheet.pairing)
                if sheet.pairing {
                    MacSpinner("Pairing…")
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Pair") { onPair(code) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canPair)
            }
        }
        // Core asks again once the code it was sent fails.
        .onChange(of: sheet.pairing) { editedSinceAsked = false }
        .onChange(of: code) { editedSinceAsked = true }
    }

    @ViewBuilder
    private var field: some View {
        if let digits = sheet.request.digits {
            MacPinField(
                digits: Int(digits),
                value: $code,
                error: fieldLine,
                focusesOnAppear: true
            )
        }
        else {
            FieldColumn {
                MacTextField("Password") {
                    VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                        SecureField("Password", text: $code)
                            .labelsHidden()
                            .boxedTextField()
                        if let fieldLine {
                            Text(verbatim: fieldLine)
                                .themeText(.detail)
                                .foregroundStyle(Theme.danger)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
        }
    }

    /// Core's line about the code it refused, until the person edits it or
    /// sends another.
    private var fieldLine: String? {
        editedSinceAsked || sheet.pairing
            ? nil : sheet.request.localizedFieldLine
    }

    /// A PIN with all its digits, or any password, while no code is out.
    private var canPair: Bool {
        guard !sheet.pairing else { return false }
        if let digits = sheet.request.digits {
            return code.count == Int(digits)
        }
        return !code.isEmpty
    }
}
