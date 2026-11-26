import BaeKit
import SwiftUI

/// One pressing under a release-group card, a row of the pressing table: each
/// fact in a column of its own, a dot in the column of each identifier kind
/// that names it; one already in the library is dimmed.
struct MacPressingRow: View {
    /// The inset from the row's box to its text.
    static let horizontalPadding = ThemeSpace.related

    let pressing: Pressing
    let isImporting: Bool
    /// What the candidate's own text agrees with about this pressing; `nil`
    /// for typed-search results.
    var agreements: BridgeAgreements?
    let isSelected: Bool
    /// The pick being read on this row, while one is: the row names the
    /// network read it waits on.
    var loading: PickReading?
    var failure: ReleaseSelectionFailure?
    /// Identify the candidate again, reading once more the documents a run
    /// could not; `nil` where no run read any, as for a typed search.
    var onRetryUnread: (() -> Void)?
    let onSelect: (Pressing) -> Void

    private var isInLibrary: Bool {
        pressing.inLibrary
    }

    /// Whether the row can be picked: no import is running and its pick is not
    /// already loading.
    var isPickable: Bool {
        !isImporting && loading == nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            resultButton
            CatalogApplyStepLine(apply: followedApply)
                .padding(.horizontal, Self.horizontalPadding)
            unattendedLine
            failureLine
        }
        .background(rowBackground)
    }

    private var resultButton: some View {
        ZStack {
            Button {
                onSelect(pressing)
            } label: {
                Rectangle()
                    .fill(.clear)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.accentPlain)
            .disabled(!isPickable)
            // The text beside it names the row, and its action picks it.
            .accessibilityHidden(true)

            PressingTableRow { column in
                cell(column)
            } details: {
                detailsCell
            } chevron: {
                chevron
            }
            // The cells read in order, as the row's one action picks it.
            .accessibilityElement(children: .contain)
            .accessibilityAction {
                if isPickable { onSelect(pressing) }
            }
            .allowsHitTesting(false)
            // An "In library" pressing is faded whole, marker included.
            .opacity(isInLibrary ? 0.55 : 1)
        }
        .padding(.vertical, ThemeSpace.compact)
        .padding(.horizontal, Self.horizontalPadding)
    }

    /// The apply of the person's pick being read on this row; `nil` while
    /// the row reads no such pick.
    private var followedApply: (any CatalogApplySource)? {
        guard case .followed(let apply) = loading else { return nil }
        return apply
    }

    private var isReadingUnattended: Bool {
        if case .unattended = loading { return true }
        return false
    }

    /// The run's own pick being read in full on this row, kept in the
    /// layout with no height while there is none.
    private var unattendedLine: some View {
        Text(
            "Reading the release in full…",
            comment:
                "Find online, under the pressing identification picked on its own: bae reads the release from its catalogs before it stores the answer."
        )
        .themeText(.body)
        .foregroundStyle(Theme.textSecondary)
        .padding(.horizontal, Self.horizontalPadding)
        .padding(.bottom, ThemeSpace.related)
        .frame(height: isReadingUnattended ? nil : 0, alignment: .top)
        .clipped()
        .opacity(isReadingUnattended ? 1 : 0)
        .accessibilityHidden(!isReadingUnattended)
    }

    /// The row's failure and its retry: picking it again when loading the pick
    /// failed, or identifying again when a document could not be read.
    var rowFailure: (error: DisplayError, retry: () -> Void)? {
        if let failure, failure.matches(pressing) {
            return (failure.error, { onSelect(pressing) })
        }
        if let unread = pressing.documentFailure, let onRetryUnread {
            return (
                DisplayError(line: unread.unreadDocumentLine), onRetryUnread
            )
        }
        return nil
    }

    @ViewBuilder
    private var failureLine: some View {
        if let failure = rowFailure {
            MacNotice(
                error: failure.error,
                action: failure.error.retryAction(
                    isDisabled: !isPickable,
                    perform: failure.retry
                )
            )
            .themeText(.body)
            .padding(.horizontal, Self.horizontalPadding)
            .padding(.bottom, ThemeSpace.related)
        }
    }

    private var rowBackground: some View {
        RoundedRectangle(cornerRadius: ThemeRadius.control)
            .fill(
                isSelected || rowFailure != nil
                    ? Theme.accentSoft : .clear
            )
            .overlay(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .strokeBorder(
                        isSelected || rowFailure != nil
                            ? Theme.accentStrong : .clear,
                        lineWidth: 1
                    )
            )
    }

    // MARK: - What the pressing is

    /// The cell of one column: the year in strong text, the label in
    /// secondary text, then the rest in tertiary text, each cut at its edge;
    /// a dot in the column of each kind that names the pressing.
    @ViewBuilder
    private func cell(_ column: PressingColumn) -> some View {
        switch column {
        case .year:
            PressingCell(column: column) {
                MacSourcedValue(pressing.years)
                    .themeText(.strong)
                    .monospacedDigit()
            }
        case .label:
            PressingCell(column: column) {
                Text(labelNames)
                    .themeText(.body)
                    .foregroundStyle(Theme.textSecondary)
                    // The space after the label's words, inside its column.
                    .padding(.trailing, ThemeSpace.related)
            }
        case .catalog:
            tertiaryCell(
                column,
                pressing.labels.flatMap(\.catalogNumbers).map(\.stated)
            )
        case .country:
            tertiaryCell(column, pressing.areas)
        case .format:
            tertiaryCell(column, pressing.media)
        case .kind(let kind):
            PressingCell(column: column, alignment: .center) {
                // A cell with nothing in it still holds its place in the
                // row, so the columns after it keep theirs.
                if let agreements, kind.names(agreements) {
                    Circle()
                        .fill(Theme.textSecondary)
                        .frame(width: pressingDotSize, height: pressingDotSize)
                        .accessibilityLabel(kind.title)
                        .accessibilityAddTraits(.isImage)
                }
                else {
                    Color.clear.frame(width: 0, height: 0)
                }
            }
        }
    }

    private func tertiaryCell(
        _ column: PressingColumn,
        _ values: [StatedValue]
    ) -> some View {
        PressingCell(column: column) {
            MacSourcedValue(values)
                .themeText(.body)
                .foregroundStyle(Theme.textTertiary)
        }
    }

    /// Every label the records state, which core lists each name once.
    private var labelNames: String {
        pressing.labels.flatMap(\.names)
            .joined(
                separator: localizedCoreString("core.label.list_separator")
            )
    }

    /// The details, then the notes of the records the folder's text names the
    /// pressing by in italics, then "In library".
    private var detailsCell: some View {
        HStack(spacing: ThemeSpace.compact) {
            // Each detail is a word of its own, a related space from the
            // next.
            if !pressing.details.isEmpty {
                HStack(spacing: ThemeSpace.related) {
                    ForEach(
                        Array(pressing.details.enumerated()),
                        id: \.offset
                    ) { _, detail in
                        Text(detail)
                    }
                }
            }
            if let notes = agreements?.notes, !notes.isEmpty {
                MacSourcedValue(notes.map(\.stated))
                    .italic()
            }
            if isInLibrary {
                libraryMarker
                    .fixedSize()
            }
        }
        .themeText(.body)
        .foregroundStyle(Theme.textTertiary)
        .cutLine(.prose)
        // The details end where the next column begins, a group space on.
        .padding(.trailing, ThemeSpace.group)
    }

    /// The "In library" tag, drawn only while the library holds the pressing,
    /// so the pressing's details take the width otherwise.
    private var libraryMarker: some View {
        HStack(spacing: ThemeSpace.inline) {
            // The check only marks the words, so VoiceOver reads the words
            // alone. It takes the detail text's font and not its line, so the
            // line's spare height moves the words and leaves the check.
            StatusGlyph(.success)
                .font(ThemeText.detail.font)
            Text("In library")
        }
    }

    private var chevron: some View {
        Image(systemName: "chevron.forward")
            .themeIcon(.small)
            .foregroundStyle(
                isSelected ? Theme.accent : Theme.textTertiary
            )
            .frame(width: ThemeIcon.small.size)
            .accessibilityHidden(true)
    }
}
