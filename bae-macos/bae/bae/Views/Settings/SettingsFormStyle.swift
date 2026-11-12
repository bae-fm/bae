import BaeKit
import SwiftUI

/// The measures of a settings box's rows and of the space between sections.
private enum RowMetrics {
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
    /// belongs to the switch above it: set with `joiningRowAbove()`.
    @Entry
    fileprivate var joinsRowAbove = false
}

extension View {
    /// Makes this row part of the row above it in a settings box: no line
    /// between them and none of the space a row of its own has above it, so
    /// the two read as one setting with a second line.
    func joiningRowAbove() -> some View {
        containerValue(\.joinsRowAbove, true)
    }
}

extension FormStyle where Self == SettingsFormStyle {
    /// The form the settings panes are drawn in: each section a box on the
    /// surface colour under its heading, with its footer below.
    static var settings: SettingsFormStyle { SettingsFormStyle() }
}

/// A form drawn the way the settings boards draw a pane: boxes in `surface`
/// on the page, rows of one height with a line between them, a heading above
/// each box and its footer under it.
///
/// The system's grouped form fixes its boxes' fill, row height and spacing, so
/// none of them can be set to the boards' values; this style lays the
/// sections out itself. The controls a form's rows hold take their form look
/// from the style they are given, so it also sets the ones the panes use: a
/// switch with its label to the left, a label with its value at the right.
struct SettingsFormStyle: FormStyle {
    func makeBody(configuration: Configuration) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Group(sections: configuration.content) { sections in
                    ForEach(Array(sections.enumerated()), id: \.offset) {
                        index,
                        section in
                        SettingsFormSection(section: section)
                            .padding(
                                .top,
                                index == 0 ? 0 : RowMetrics.sectionGap
                            )
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, SettingsPaneMetrics.sideInset)
            .padding(.bottom, SettingsPaneMetrics.topInset)
        }
        .settingsRowControls()
    }
}

extension View {
    /// Draws the controls in a settings box's rows as the boards do: a switch
    /// with its label to the left, a label with its value or control at the
    /// right. A form takes its controls' form look from its style, so the
    /// style sets these; a box outside a form sets them itself.
    func settingsRowControls() -> some View {
        toggleStyle(SettingsSwitchStyle())
            .labeledContentStyle(SettingsLabeledContentStyle())
    }
}

/// Rows on the surface with a line between each and the next, as a settings
/// form draws a section's, for a box that is not part of a form: a popover's.
struct SettingsBox<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        Group(subviews: content) { rows in
            SettingsFormBox(rows: rows)
        }
        .settingsRowControls()
    }
}

/// One section: its heading, its rows in a box, and its footer.
private struct SettingsFormSection: View {
    let section: SectionConfiguration

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !section.header.isEmpty {
                section.header
                    .themeText(.strong)
                    .padding(.horizontal, ThemeSpace.inline)
                    .padding(.bottom, ThemeSpace.compact)
            }
            SettingsFormBox(rows: section.content)
            if !section.footer.isEmpty {
                section.footer
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, ThemeSpace.inline)
                    .padding(.top, ThemeSpace.compact)
            }
        }
    }
}

/// A section's rows on the surface, a line between each and the next unless
/// the next belongs to it.
private struct SettingsFormBox: View {
    let rows: SubviewsCollection

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                // The first row has none above it to join.
                let joins = index > 0 && row.containerValues.joinsRowAbove
                if index > 0, !joins {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(height: ThemeSpace.hairline)
                        .padding(.horizontal, RowMetrics.inset)
                }
                row
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, RowMetrics.inset)
                    .padding(.top, joins ? 0 : RowMetrics.padding)
                    .padding(.bottom, RowMetrics.padding)
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

/// A switch the size the boards draw (the mini one), its label to the left in
/// the row's own text size: a control size also sets the label's.
private struct SettingsSwitchStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Toggle(configuration)
            .toggleStyle(.switch)
            .controlSize(.mini)
            .themeText(.body)
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
