import BaeKit
import SwiftUI

/// A CUE sheet's caption, the design system's MacImportSheetCaption: the
/// side or disc the sheet is on, CUE, the sheet's name as a button that
/// opens it in the viewer, then the audio its FILE lines are bound to. Each
/// surface gives the place and the audio as it offers them.
struct MacImportSheetCaption<Place: View, Audio: View>: View {
    let name: String
    let onOpen: () -> Void
    @ViewBuilder
    let place: Place
    @ViewBuilder
    let audio: Audio

    @State
    private var hoveringName = false
    @Environment(\.layoutDirection)
    private var layoutDirection

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            place
            // A format name, so it is not translated.
            StatusChip(verbatim: "CUE", tone: .accent)
                .fixedSize()
            nameButton
            // The arrow joins the sheet to its audio for the eye; what
            // follows it says what it is.
            Text(verbatim: "→")
                .themeText(.body)
                .foregroundStyle(Theme.textTertiary)
                // The arrow points from the sheet to its audio, so it turns
                // round where the row reads right to left.
                .scaleEffect(x: layoutDirection == .rightToLeft ? -1 : 1)
                .fixedSize()
                .accessibilityHidden(true)
            audio
            Spacer(minLength: 0)
        }
        .padding(.vertical, ThemeSpace.related)
        // The board's rule is a border, so it takes its own height above the
        // caption's padding.
        .padding(.top, ThemeSpace.hairline)
        .overlay(alignment: .top) { MacHairline() }
    }

    /// The sheet's name opens it in the viewer. It has no layout priority, so
    /// a long name and a long audio name share a narrow caption's room.
    private var nameButton: some View {
        Button(action: onOpen) {
            Text(verbatim: name)
                .themeText(.mono)
                .foregroundStyle(.primary)
                .underline(hoveringName)
                .cutLine(.fileName)
        }
        .buttonStyle(.accentPlain)
        .onHover { hoveringName = $0 }
    }
}
