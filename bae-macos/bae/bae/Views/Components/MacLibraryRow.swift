import BaeKit
import SwiftUI

/// One row of a library list: a cover, a title over a second line, and an
/// optional trailing label. Search's results, the artist and composer lists,
/// and a composer's or work's page all draw it.
///
/// The row is padded 8pt. `bleed` is for a row on a page: the fill reaches
/// 8pt past the layout on each side, so the text lines up with the page's
/// headings. `credit` is the text-only row of a credit or recording, which
/// opens nothing. `part` indents a work under its group's heading work.
/// `hover` fills the row, `selected` tints it while its page is open beside
/// the list, and `loading` is the row before it has arrived.
struct MacLibraryRow: View {
    var picture: BridgeImageRef?
    var title: String?
    var subtitle: String?
    var trailing: String?
    var hover = false
    var selected = false
    var loading = false
    var bleed = false
    var credit = false
    var part = false

    var body: some View {
        content
            .padding(.horizontal, ThemeSpace.related)
            .padding(.vertical, credit ? ThemeSpace.inline : ThemeSpace.related)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(fill)
            )
            .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
            .padding(.horizontal, bleed ? -ThemeSpace.related : 0)
            .padding(.leading, part ? ThemeSpace.edge : 0)
    }

    @ViewBuilder
    private var content: some View {
        if credit {
            lines
        }
        else {
            HStack(spacing: ThemeSpace.group) {
                cover
                lines
                Spacer(minLength: ThemeSpace.related)
                if let trailing {
                    Text(trailing)
                        .themeText(.detail)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private var cover: some View {
        if loading {
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(Theme.placeholder)
                .frame(
                    width: ThemeSize.rowArtwork,
                    height: ThemeSize.rowArtwork
                )
        }
        else {
            ImageView(imageRef: picture, pointSize: ThemeSize.rowArtwork)
                .frame(
                    width: ThemeSize.rowArtwork,
                    height: ThemeSize.rowArtwork
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
        }
    }

    @ViewBuilder
    private var lines: some View {
        if loading {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 140, height: 11)
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 80, height: 10)
            }
        }
        else {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                StableOptionalText(
                    text: title,
                    font: ThemeText.rowTitle.font,
                    foreground: .primary,
                    lineHeight: 17,
                    lineLimit: 1
                )
                StableOptionalText(
                    text: subtitle,
                    font: ThemeText.detail.font,
                    foreground: .secondary,
                    lineHeight: 14,
                    lineLimit: 1
                )
            }
        }
    }

    /// A selected row keeps its accent fill under hover. A credit opens
    /// nothing, so nothing fills it.
    private var fill: Color {
        if credit || loading {
            return .clear
        }
        if selected {
            return Theme.accentSoft
        }
        return hover ? Theme.hover : .clear
    }
}

/// A `MacLibraryRow` that opens something, and so fills under the pointer.
/// A loading row cannot be pressed.
struct MacLibraryRowButton: View {
    var picture: BridgeImageRef?
    var title: String?
    var subtitle: String?
    var trailing: String?
    var selected = false
    var loading = false
    var bleed = false
    var part = false
    let action: () -> Void

    @State
    private var hovering = false

    var body: some View {
        Button(action: action) {
            MacLibraryRow(
                picture: picture,
                title: title,
                subtitle: subtitle,
                trailing: trailing,
                hover: hovering,
                selected: selected,
                loading: loading,
                bleed: bleed,
                part: part
            )
        }
        .buttonStyle(.plain)
        .disabled(loading)
        .onHover { hovering = $0 }
    }
}

#if DEBUG
    #Preview("Mac Library Row") {
        VStack(spacing: ThemeSpace.hairline) {
            MacLibraryRow(title: "Album Title", subtitle: "Artist Name (1992)")
            MacLibraryRow(
                title: "Track Title",
                subtitle: "Artist Name - Album Title",
                trailing: "4:32"
            )
            MacLibraryRow(
                title: "Artist Name",
                subtitle: "3 albums",
                hover: true
            )
            MacLibraryRow(
                title: "Artist Name",
                subtitle: "3 albums",
                selected: true
            )
            MacLibraryRow(loading: true)
            MacLibraryRow(
                title: "Work Title",
                subtitle: "Composer Name",
                bleed: true
            )
            MacLibraryRow(
                title: "Part Title",
                subtitle: "Composer Name",
                bleed: true,
                part: true
            )
            MacLibraryRow(
                title: "Album Title",
                subtitle: "Arranger",
                bleed: true,
                credit: true
            )
        }
        .padding()
        .frame(width: 380)
        .environment(ImageStore.stub())
    }
#endif
