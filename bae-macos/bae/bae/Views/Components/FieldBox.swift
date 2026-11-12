import BaeKit
import SwiftUI

/// A box of labelled field rows on the sheet: the design's box, with a
/// `FieldBoxDivider` between rows.
struct FieldBox<Rows: View>: View {
    @ViewBuilder
    let rows: Rows

    var body: some View {
        VStack(spacing: 0) {
            rows
        }
        .card()
    }
}

/// The line between two rows of a `FieldBox`, inset to the rows' text.
struct FieldBoxDivider: View {
    var body: some View {
        Divider()
            .padding(.horizontal, ThemeSpace.group)
            .opacity(0.6)
    }
}

/// One row of a `FieldBox`: its label in a column `labelWidth` wide with the
/// text at its leading edge, then the field and, under the field, the note.
struct FieldBoxRow<Field: View>: View {
    let label: LocalizedStringKey
    let labelWidth: CGFloat
    let note: LocalizedStringKey?
    private let field: Field

    init(
        _ label: LocalizedStringKey,
        labelWidth: CGFloat,
        note: LocalizedStringKey? = nil,
        @ViewBuilder field: () -> Field
    ) {
        self.label = label
        self.labelWidth = labelWidth
        self.note = note
        self.field = field()
    }

    var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            Text(label)
                .frame(width: labelWidth, alignment: .leading)
                .padding(.top, ThemeSpace.related)
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                field
                if let note {
                    FieldNote(note)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, ThemeSpace.group)
        .padding(.vertical, ThemeSpace.compact)
        .frame(minHeight: 40, alignment: .top)
    }
}

/// The line under a field: what the field is for, 11.5 over 15, in a colour
/// between secondary and tertiary text, starting where the field's text does.
struct FieldNote: View {
    private let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .themeText(.detail)
            .lineSpacing(ThemeSpace.line / 2)
            .foregroundStyle(Color.secondary.opacity(0.7))
            .padding(.leading, ThemeSpace.fieldInset)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// A loose paragraph between boxes: 11.5 over 15 in secondary text, set in
/// from the boxes' edges.
struct MacNote: View {
    private let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .themeText(.detail)
            .lineSpacing(ThemeSpace.line / 2)
            .foregroundStyle(.secondary)
            .padding(.horizontal, ThemeSpace.inline)
            .padding(.top, ThemeSpace.compact)
            .fixedSize(horizontal: false, vertical: true)
    }
}
