import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macInlineFieldScenes: [ShotScene] = [
        inlineFieldScene(
            "mac.component.inline-field.value",
            placeholder: "—",
            value: "Harbor Records"
        ),
        inlineFieldScene(
            "mac.component.inline-field.empty",
            placeholder: "—",
            value: ""
        ),
        inlineFieldScene(
            "mac.component.inline-field.long-value",
            placeholder: "—",
            value: "Harbor Records and the Northern Sound Company"
        ),
        inlineFieldScene(
            "mac.component.inline-field.large",
            placeholder: "Album title",
            value: "Low Tide Hours",
            large: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.large-empty",
            placeholder: "Album title",
            value: "",
            large: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.quiet",
            placeholder: "Album year",
            value: "1992",
            quiet: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.quiet-empty",
            placeholder: "Album year",
            value: "",
            quiet: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.fits-its-value",
            placeholder: "—",
            value: "Harbor Records",
            fits: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.fits-empty",
            placeholder: "—",
            value: "",
            fits: true
        ),
        inlineFieldScene(
            "mac.component.inline-field.mono",
            placeholder: "—",
            value: "Harbor Records",
            mono: true
        ),
    ]

    /// One `MacInlineField` in the preview's 260pt column; `large` is the
    /// album title's size, `quiet` the album year's tertiary colour and
    /// `fits` a field as wide as its value. The field's name is its label
    /// on the board, which is the placeholder for the two that name it so.
    private static func inlineFieldScene(
        _ id: String,
        placeholder: String,
        value: String,
        large: Bool = false,
        quiet: Bool = false,
        mono: Bool = false,
        fits: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 260,
            height: large ? 46 : 38
        ) {
            MacInlineField(
                placeholder: placeholder,
                value: value,
                name: placeholder == "—" ? "Label" : placeholder,
                monospaced: mono,
                fillsWidth: !fits,
                fitsValue: fits,
                font: large ? ThemeText.title.nsFont : ThemeText.body.nsFont,
                textColor: quiet
                    ? NSColor(Theme.textTertiary) : NSColor(Theme.textPrimary),
                placeholderRole: large || quiet ? .hint : .emptyMark,
                editingCommands: EditingCommitCommands(),
                onCommit: { _ in }
            )
            // The column holds the field's words at its edge, so the field
            // reaches out by its padding, as its callers lay it out.
            .padding(.leading, -FieldChrome.inlineHorizontalPadding)
            .frame(width: 260, alignment: .leading)
        }
    }
}
