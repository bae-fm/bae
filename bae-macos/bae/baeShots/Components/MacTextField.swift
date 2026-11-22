import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTextField`, a label beside a boxed field, with the note under it.
    static let macTextFieldScenes: [ShotScene] = [
        textField("mac.component.text-field.resting"),
        textField("mac.component.text-field.focused", state: .focused),
        textField("mac.component.text-field.empty", text: ""),
        textField(
            "mac.component.text-field.note",
            note: "People in this library see this name."
        ),
        textField(
            "mac.component.text-field.error",
            error: "Enter a name.",
            text: "",
            state: .invalid
        ),
        textField("mac.component.text-field.off", state: .off),
        textField("mac.component.text-field.secret", state: .secret),
        component(
            "mac.component.text-field.label-inside",
            width: previewWidth,
            height: 90
        ) {
            FieldColumn {
                GridRow {
                    BoxedField(placeholder: "Name", text: .constant(""))
                        .gridCellColumns(2)
                }
            }
            .frame(width: previewWidth, alignment: .leading)
        },
        codeField("mac.component.text-field.code"),
        textField("mac.component.text-field.label-column", labelColumn: true),
    ]

    /// The code field, as the Enter code screen draws it: a typed code in
    /// groups of four. A component's window is not the active app, so the
    /// field's request for the keyboard leaves it at rest, as the board draws
    /// it.
    private static func codeField(_ id: String) -> ShotScene {
        component(
            id,
            width: previewWidth,
            height: 90,
            // AppKit hands a key window's first text field the keyboard, and
            // the board draws this row's field with nobody on it.
            isDrawn: { host in
                host.window?.makeFirstResponder(nil)
                return !(host.window?.firstResponder is NSText)
            },
            view: { codeFieldRow() }
        )
    }

    @MainActor
    private static func codeFieldRow() -> some View {
        let form = enterCodeForm()
        form.edit(code: "Home music")
        return FieldColumn {
            MacTextField("Name") {
                CodeField(form: form)
            }
        }
        .frame(width: previewWidth, alignment: .leading)
    }

    /// How a text field's box stands.
    private enum FieldState {
        case resting, focused, invalid, secret, off
    }

    private static func textField(
        _ id: String,
        note: LocalizedStringKey? = nil,
        error: LocalizedStringKey? = nil,
        text: String = "Home music",
        state: FieldState = .resting,
        labelColumn: Bool = false
    ) -> ShotScene {
        component(id, width: previewWidth, height: 90) {
            FieldColumn {
                MacTextField(
                    "Name",
                    note: note,
                    error: error,
                    off: state == .off
                ) {
                    BoxedField(
                        text: .constant(text),
                        secret: state == .secret,
                        invalid: state == .invalid,
                        focusesOnAppear: state == .focused
                    )
                }
                if labelColumn {
                    // A column of fields holds every label at the widest
                    // one's width; the preview's is 90.
                    GridRow {
                        Color.clear.frame(width: 90, height: 0)
                    }
                }
            }
            .frame(width: previewWidth, alignment: .leading)
        }
    }

    /// The width of a wide row of the preview, which the field fills.
    private static let previewWidth: CGFloat = 842
}
