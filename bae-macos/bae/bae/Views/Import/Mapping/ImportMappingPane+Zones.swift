import BaeKit
import SwiftUI

// MARK: - The header and slot zones

extension ImportMappingPane {
    /// Zone 1: the release the folder is being read as, and the release fields
    /// behind a disclosure — the album and pressing values the editor holds.
    /// Its tracks are the slot table below, so the track half of the metadata
    /// form has no place here.
    @ViewBuilder
    var headerZone: some View {
        VStack(alignment: .leading, spacing: 10) {
            ImportReleaseHeader(
                title: headerTitle,
                artist: headerArtist,
                metaLine: headerMetaLine,
                claim: candidate.claim,
                hasPick: candidate.pickedReleaseId != nil,
                coverContent: coverContent,
                hasCoverOptions: hasCoverOptions,
                onEditCover: onEditCover,
                onFindRelease: onFindRelease,
            )
            if let editor {
                DisclosureGroup(String(localized: "Release details")) {
                    ReleaseFieldsForm(form: editor)
                        .padding(.top, 10)
                }
                .font(.system(size: 12))
            }
        }
    }

    /// Zone 3: the slot table, once there is a tracklist to map onto. While the
    /// pick's detail is still loading the zone says so rather than showing an
    /// empty table.
    @ViewBuilder
    var slotsZone: some View {
        switch candidate.mode {
        case .loadingDetail:
            ProgressView("Loading release details...")
                .frame(maxWidth: .infinity)
                .padding(.vertical, 24)
        case .confirming:
            if let editor {
                ImportSlotsTable(
                    rows: model.slotRows,
                    audioChoices: model.audioChoices,
                    reconciliation: model.reconciliation,
                    previewingPath: previewingPath,
                    values: editor,
                    actions: slotActions,
                )
            }
        case .identifying:
            Text("Pick a release to map these files onto its tracks")
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// The album title the header leads with: what the editor holds once there
    /// is one, and the folder's own name before that.
    private var headerTitle: String {
        let title = editor?.wrappedValue.albumTitle ?? ""
        return title.isEmpty ? candidate.displayName : title
    }

    private var headerArtist: String {
        editor?.wrappedValue.albumArtistText ?? ""
    }

    /// "CD · 1996 · 9 tracks" from the live editor, so it tracks what is being
    /// edited. Empty pressing fields drop out rather than leaving stray
    /// separators.
    private var headerMetaLine: String {
        guard let values = editor?.wrappedValue else {
            return candidate.files.formatLabel
        }
        let count = values.tracks.count
        let trackText = String(localized: "\(count) tracks")
        return [values.pressing.format, values.pressing.year, trackText]
            .filter { !$0.isEmpty }
            .joined(separator: " \u{00b7} ")
    }
}
