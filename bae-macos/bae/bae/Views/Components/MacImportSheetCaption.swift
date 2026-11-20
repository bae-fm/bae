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

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            place
            // A format name, so it is not translated.
            StatusChip(verbatim: "CUE", tone: .accent)
                .fixedSize()
            nameButton
            // The arrow joins the sheet to its audio for the eye; what
            // follows it says what it is.
            Image(systemName: "arrow.forward")
                .themeIcon(.small)
                .foregroundStyle(.tertiary)
                .fixedSize()
                .accessibilityHidden(true)
            audio
            Spacer(minLength: 0)
        }
    }

    /// The sheet's name opens it in the viewer.
    private var nameButton: some View {
        Button(action: onOpen) {
            Text(verbatim: name)
                .themeText(.mono)
                .foregroundStyle(.primary)
                .underline(hoveringName)
                .cutLine(.fileName)
        }
        .buttonStyle(.accentPlain)
        .layoutPriority(1)
        .onHover { hoveringName = $0 }
    }
}
