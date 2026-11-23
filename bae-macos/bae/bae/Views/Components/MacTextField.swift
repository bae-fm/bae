import BaeKit
import SwiftUI

/// One row of a `FieldColumn`: the label, then the field, with the note
/// under it that starts where the field's text starts.
struct MacTextField<Field: View>: View {
    let label: LocalizedStringKey
    let note: LocalizedStringKey?
    /// What is wrong with the field, which replaces the note.
    let error: LocalizedStringKey?
    /// The field takes no typing, and its label dims with it.
    let off: Bool
    /// The label only names the field to assistive technology, and the field
    /// takes the label's column too, as the board draws a field whose label
    /// is its placeholder.
    let labelHidden: Bool
    private let field: Field
    @Environment(\.fieldLabelWidth)
    private var labelWidth

    init(
        _ label: LocalizedStringKey,
        note: LocalizedStringKey? = nil,
        error: LocalizedStringKey? = nil,
        off: Bool = false,
        labelHidden: Bool = false,
        @ViewBuilder field: () -> Field
    ) {
        self.label = label
        self.note = note
        self.error = error
        self.off = off
        self.labelHidden = labelHidden
        self.field = field()
    }

    var body: some View {
        GridRow(alignment: .fieldLine) {
            if !labelHidden {
                Text(label)
                    .frame(width: labelWidth, alignment: .trailing)
                    .opacity(off ? ThemeOpacity.off : 1)
                    .gridColumnAlignment(.trailing)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                field
                    .accessibilityLabel(Text(label))
                    .disabled(off)
                    // The board dims the whole field by `off`, text and box
                    // together, so the field's own native dim is not what
                    // the person sees.
                    .opacity(off ? ThemeOpacity.off : 1)
                    .alignmentGuide(.fieldLine) { $0[VerticalAlignment.center] }
                if let error {
                    FieldNote(error, isError: true)
                }
                else if let note {
                    FieldNote(note)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .gridCellColumns(labelHidden ? 2 : 1)
        }
    }
}

extension VerticalAlignment {
    private enum FieldLine: AlignmentID {
        static func defaultValue(in context: ViewDimensions) -> CGFloat {
            context[VerticalAlignment.center]
        }
    }

    /// The middle of a field's own line, which its label sits on, whatever
    /// the field's height and the note under it.
    static let fieldLine = VerticalAlignment(FieldLine.self)
}
