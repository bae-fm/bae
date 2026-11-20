import BaeKit
import SwiftUI

extension FormStyle where Self == SettingsFormStyle {
    /// The form the settings panes are drawn in: each section a box on the
    /// surface colour under its heading, with its footer below.
    static var settings: SettingsFormStyle { SettingsFormStyle() }

    // periphery:ignore - called by baeShots
    /// The settings form without the page margin a pane puts at its sides,
    /// for a form that a component's frame holds.
    static var settingsWithoutMargin: SettingsFormStyle {
        SettingsFormStyle(sideInset: 0)
    }
}

/// A form drawn the way the settings boards draw a pane: boxes in `surface`
/// on the page, rows of one height with a line between them, a heading above
/// each box and its footer under it.
///
/// The system's grouped form fixes its boxes' fill, row height and spacing, so
/// none of them can be set to the boards' values; this style lays the
/// sections out itself. A form's rows take their form look from the style
/// they are given, so it also sets the one the panes use: a label with its
/// value at the right.
struct SettingsFormStyle: FormStyle {
    /// The page margin at each side of the sections.
    var sideInset = SettingsPaneMetrics.sideInset

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
            .padding(.horizontal, sideInset)
            .padding(.bottom, SettingsPaneMetrics.topInset)
        }
        .settingsRowControls()
    }
}

/// One section: its heading, its rows in a box, and under it its footer: a
/// note, or a row of buttons (`MacSheetFooter`).
private struct SettingsFormSection: View {
    let section: SectionConfiguration

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !section.header.isEmpty {
                // A form's section header reads as a heading; the design
                // system draws a section's title as plain text, as a box's
                // title outside a form reads.
                section.header.settingsSectionTitle()
                    .accessibilityRemoveTraits(.isHeader)
            }
            MacBoxRows(rows: section.content)
            ForEach(section.footer) { footer in
                if footer.containerValues.isSectionButtons {
                    // The section's space between its parts; the buttons
                    // keep their own space above them.
                    footer.padding(.top, ThemeSpace.compact)
                }
                else {
                    footer
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        // The note's text starts where the rows' labels start,
                        // 8pt under the box.
                        .padding(.horizontal, RowMetrics.inset)
                        .padding(.top, ThemeSpace.related)
                }
            }
        }
    }
}
