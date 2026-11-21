import BaeKit
import SwiftUI

/// The release identity shown throughout import: title, artist, the edition
/// and copy lines, and the source-audio facts observed by the scan.
struct ImportReleaseSummary {
    let title: String
    let titleIsPlaceholder: Bool
    let artist: String?
    /// What the catalog's release prints: its year, each label with its
    /// catalog number, then its country; empty for a draft from the files'
    /// own tags.
    let editionLine: String
    /// What the person has: the media and details; empty for a draft from the
    /// files' own tags.
    let copyLine: String
    let sourceAudio: BridgeSourceAudioSummary?

    /// `candidate`'s draft, `draft`, with the folder's audio.
    init(candidate: Candidate, draft: BridgeCandidateDraftPane) {
        let values = draft.metadataDraft
        let provenance = draft.metadataProvenance
        titleIsPlaceholder = values.albumTitle.isEmpty
        title =
            values.albumTitle.isEmpty
            ? Self.placeholderTitle : values.albumTitle
        artist =
            values.albumArtistAssignments.isEmpty
            ? nil
            : bridgeCreditLine(credits: values.albumArtistAssignments).text
        switch provenance {
        case .externalRelease:
            let pressing = values.pressing
            editionLine = FactList.joined([
                pressing.year,
                FactList.joined(
                    pressing.labels.map { row in
                        FactList.joined([
                            row.label.name, row.label.catalogNumber,
                        ])
                    }
                ),
                pressing.facts.area?.editionText,
            ])
            copyLine = FactList.joined([
                PressingText.media(pressing.facts.media),
                PressingText.line(
                    bridgePressingDetails(facts: pressing.facts)
                ),
            ])
        case .fileMetadata, nil:
            editionLine = ""
            copyLine = ""
        }
        sourceAudio = candidate.files.sourceAudio
    }

    /// The row's applied draft; `nil` for an `unidentified` row, which has
    /// none.
    init?(row: BridgeTriageRow) {
        guard let summary = row.metadataSummary else { return nil }
        titleIsPlaceholder = summary.albumTitle.isEmpty
        title =
            summary.albumTitle.isEmpty
            ? Self.placeholderTitle : summary.albumTitle
        artist =
            summary.albumArtists.names.isEmpty
            ? nil : summary.albumArtists.text
        editionLine = ""
        copyLine = ""
        sourceAudio = nil
    }

    /// A folder that failed validation, as its tags call the release: the
    /// album title, and the artist under it.
    init(tags: BridgeSkippedTags) {
        titleIsPlaceholder = false
        title = tags.albumTitle
        artist = tags.artist?.text
        editionLine = ""
        copyLine = ""
        sourceAudio = nil
    }

    /// The title shown in place of a release's empty album title.
    private static var placeholderTitle: String {
        String(localized: "Album title")
    }
}

/// An import release summary, scaled for the sidebar or the card, with the
/// caller's accessory after the title.
struct MacImportReleaseSummary<TitleAccessory: View>: View {
    enum Style {
        case sidebar
        case card
    }

    let summary: ImportReleaseSummary
    let style: Style
    @ViewBuilder
    let titleAccessory: () -> TitleAccessory

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            titleLine
            artistLine
            if style.showsFacts {
                factLine(summary.editionLine)
                    .padding(.top, style.factsTopPadding)
                factLine(summary.copyLine)
            }
            if style.showsFacts, let sourceAudio = summary.sourceAudio {
                SourceAudioLine(sourceAudio: sourceAudio)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The title and the caller's accessory; the title truncates first.
    private var titleLine: some View {
        HStack(spacing: ThemeSpace.compact) {
            Text(summary.title)
                .themeText(style.titleText)
                .foregroundStyle(
                    summary.titleIsPlaceholder
                        ? Theme.textSecondary : Theme.textPrimary
                )
                .cutLine(.prose)
            titleAccessory()
                .layoutPriority(1)
        }
    }

    @ViewBuilder
    private func factLine(_ line: String) -> some View {
        if !line.isEmpty {
            Text(line)
                .themeText(.detail)
                .foregroundStyle(Theme.textTertiary)
                .lineLimit(1)
        }
    }

    @ViewBuilder
    private var artistLine: some View {
        if let artist = summary.artist {
            Text(artist)
                .themeText(style.artistText)
                .foregroundStyle(Theme.textSecondary)
                .cutLine(.prose)
        }
    }
}

extension MacImportReleaseSummary where TitleAccessory == EmptyView {
    init(summary: ImportReleaseSummary, style: Style) {
        self.init(summary: summary, style: style) { EmptyView() }
    }
}

extension MacImportReleaseSummary.Style {
    fileprivate var titleText: ThemeText {
        switch self {
        case .sidebar: .rowTitle
        case .card: .heading
        }
    }

    fileprivate var artistText: ThemeText {
        switch self {
        case .sidebar: .detail
        case .card: .body
        }
    }

    fileprivate var factsTopPadding: CGFloat {
        switch self {
        case .sidebar: ThemeSpace.hairline
        case .card: ThemeSpace.inline
        }
    }

    fileprivate var showsFacts: Bool {
        switch self {
        case .sidebar: false
        case .card: true
        }
    }
}

/// The audio the scan read, as one line: the format, or the facts the files
/// disagree on.
struct SourceAudioLine: View {
    let sourceAudio: BridgeSourceAudioSummary

    var body: some View {
        Text(sourceAudio.text)
            .themeText(.detail)
            .foregroundStyle(Theme.textTertiary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityLabel(localizedCoreString("core.audio.label"))
            .accessibilityValue(sourceAudio.spokenText)
    }
}
