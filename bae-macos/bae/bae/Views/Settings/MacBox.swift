import BaeKit
import SwiftUI

/// The measures of a settings box's rows and of the space between sections.
enum RowMetrics {
    /// The least height of a row, the design system's `size-row`.
    static let height: CGFloat = 40
    /// A row's side inset.
    static let inset = ThemeSpace.group
    /// The space over and under a row's content.
    static let padding = ThemeSpace.related + ThemeSpace.line
    /// The space between a section and the one above.
    static let sectionGap: CGFloat = 14
}

extension ContainerValues {
    /// Whether the row belongs to the row above it, as Resume automatically
    /// belongs to the switch above it: set by `MacNested`.
    @Entry
    var joinsRowAbove = false

    /// The space over and under a row's content; a row holding a field is
    /// as tall as its field, so it takes less than a row of text.
    @Entry
    var rowVerticalInset: CGFloat = RowMetrics.padding
}

extension View {
    /// Places a filled notice in a settings box as the boards do: no space
    /// above it, 12 below, and the line above it kept; the notice has its own
    /// padding.
    func settingsNoticeRow() -> some View {
        settingsRowInset(0).padding(.bottom, ThemeSpace.group)
    }

    /// Sets the space over and under this row's content, for a row that holds
    /// a field.
    func settingsRowInset(_ inset: CGFloat) -> some View {
        containerValue(\.rowVerticalInset, inset)
    }
}

extension View {
    /// Draws the rows of a settings box as the boards do: a label with its
    /// value or control at the right. A form takes its rows' form look from
    /// its style, so the style sets it; a box outside a form sets it itself.
    func settingsRowControls() -> some View {
        labeledContentStyle(SettingsLabeledContentStyle())
    }
}

/// Rows on the surface with a line between each and the next, as a settings
/// form draws a section's, for a box that is not part of a form: a popover's.
struct MacBox<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        Group(subviews: content) { rows in
            MacBoxRows(rows: rows)
        }
        .settingsRowControls()
    }
}

/// A box's title: secondary text, so it reads apart from the rows under it.
extension View {
    func settingsSectionTitle() -> some View {
        themeText(.strong)
            .foregroundStyle(.secondary)
            .padding(.horizontal, ThemeSpace.inline)
            .padding(.bottom, ThemeSpace.compact)
    }
}

/// A `MacBox` under its title, for a section that is not part of a form:
/// a popover's.
struct SettingsTitledBox<Content: View>: View {
    let title: Text?
    @ViewBuilder
    let content: Content

    /// The space between a section and the one above.
    static var sectionGap: CGFloat { RowMetrics.sectionGap }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let title {
                title.settingsSectionTitle()
            }
            MacBox { content }
        }
    }
}

/// Rows on the surface, a line between each and the next unless
/// the next belongs to it.
struct MacBoxRows: View {
    let rows: SubviewsCollection

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                // The first row has none above it to join.
                let joins = index > 0 && row.containerValues.joinsRowAbove
                if index > 0, !joins {
                    MacHairline()
                        .padding(.horizontal, RowMetrics.inset)
                }
                row
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, RowMetrics.inset)
                    .padding(
                        .top,
                        joins ? 0 : row.containerValues.rowVerticalInset
                    )
                    .padding(.bottom, row.containerValues.rowVerticalInset)
                    .frame(
                        minHeight: joins ? 0 : RowMetrics.height
                    )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            Theme.surface,
            in: RoundedRectangle(cornerRadius: ThemeRadius.card)
        )
    }
}

/// A label at the left and its value or control at the right, in the rows of
/// a settings box.
private struct SettingsLabeledContentStyle: LabeledContentStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: RowMetrics.inset) {
            configuration.label
            Spacer(minLength: RowMetrics.inset)
            configuration.content
        }
    }
}
