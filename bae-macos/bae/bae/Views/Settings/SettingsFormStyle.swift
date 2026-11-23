import BaeKit
import SwiftUI

extension FormStyle where Self == SettingsFormStyle {
    /// The form the settings panes are drawn in: each section a box on the
    /// surface colour under its heading, with its footer below.
    static var settings: SettingsFormStyle { SettingsFormStyle() }

    /// The settings form inside a sheet, which has its own margin at the
    /// sides and between its parts: a section keeps the space the boards put
    /// over it, even the first, and the form ends the 10 points that a
    /// sheet's parts have between them above what follows.
    static var settingsInSheet: SettingsFormStyle {
        SettingsFormStyle(
            sideInset: 0,
            topInset: RowMetrics.sectionGap,
            bottomInset: ThemeSpace.related + ThemeSpace.line
        )
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
    /// The space above the first section.
    var topInset: CGFloat = 0
    /// The space below the last section.
    var bottomInset = SettingsPaneMetrics.topInset

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
            .padding(.top, topInset)
            .padding(.bottom, bottomInset)
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
                    // 8pt under the box, in line with the rows' labels.
                    footer
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .noteText(above: ThemeSpace.related)
                }
            }
        }
    }
}
