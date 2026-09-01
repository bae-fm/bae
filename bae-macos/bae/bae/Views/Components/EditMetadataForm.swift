import BaeKit
import SwiftUI

/// Pure form over a raw edit-metadata payload (`BridgeRawReleaseEdit`).
/// Two-way binding on the raw text the user types; no async state, no
/// save/cancel buttons — those belong to the surrounding surface. The
/// form does no shaping or validation: it collects raw values and bae-core
/// turns them into a wire edit via `shapeReleaseEdit`.
///
/// Used by `EditMetadataSheet` — the post-commit "Edit metadata..." sheet,
/// which wraps it with a header (Cancel) and footer (Reset, Save). The import's
/// mapping pane edits the release fields alone (`ReleaseFieldsForm`): its
/// tracks are the mapping table, where each row also carries the audio behind
/// it.
///
/// Layout: the release fields are label-left / value-right rows inside grouped
/// inset cards; tracks are one table with a single header row and compact
/// editable cells, so inherited and explicit track artists use the same rows.
/// The view does not scroll —
/// the surrounding surface owns scrolling.
struct EditMetadataForm: View {
    @Binding
    var form: BridgeRawReleaseEdit

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            ReleaseFieldsForm(form: $form)
            tracksGroup
        }
    }
}

// MARK: - Tracks

extension EditMetadataForm {
    fileprivate var tracksGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(
                title: String(localized: "Tracks"),
                trailing: trackCountLabel
            )
            VStack(spacing: 0) {
                trackHeaderRow
                if form.tracks.isEmpty {
                    Text("No tracks")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                }
                else {
                    ForEach(
                        Array(form.tracks.enumerated()),
                        id: \.element.id
                    ) { index, _ in
                        trackRow(
                            index: index,
                            track: $form.tracks[index],
                            isLast: index == form.tracks.count - 1,
                        )
                    }
                }
            }
            .formGroupCard()
        }
    }

    private var trackCountLabel: String {
        String(localized: "\(form.tracks.count) tracks")
    }

    private var trackHeaderRow: some View {
        HStack(spacing: 10) {
            FormEyebrow(text: Text(verbatim: "#"))
                .frame(width: trackOrdinalWidth)
            FormEyebrow(text: Text("Title"))
                .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(text: Text("Artist"))
                .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(text: Text("Disc"))
                .frame(width: trackNumberWidth)
            FormEyebrow(text: Text("Track"))
                .frame(width: trackNumberWidth)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(.white.opacity(0.13))
                .frame(height: 1)
        }
    }

    private func trackRow(
        index: Int,
        track: Binding<BridgeRawTrackEdit>,
        isLast: Bool,
    ) -> some View {
        HStack(spacing: 10) {
            Text(verbatim: (index + 1).formatted())
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(width: trackOrdinalWidth)
            MetadataField(
                placeholder: String(localized: "Title"),
                text: track.title,
                chrome: .inline,
            )
            .frame(maxWidth: .infinity)
            trackArtistsField(track.artistAssignments)
                .frame(maxWidth: .infinity)
            TrackSideCell(value: track.side)
                .frame(width: trackNumberWidth)
            TrackNumberCell(value: track.trackNumber)
                .frame(width: trackNumberWidth)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 38)
        .background(
            index.isMultiple(of: 2) ? Color.clear : .white.opacity(0.02)
        )
        .overlay(alignment: .bottom) {
            // Kept in the tree on every row and hidden on the last, so a
            // row's intrinsic size never changes across the table.
            Rectangle()
                .fill(.white.opacity(0.07))
                .frame(height: 1)
                .opacity(isLast ? 0 : 1)
                .allowsHitTesting(false)
        }
    }

    private func trackArtistsField(
        _ assignments: Binding<BridgeTrackArtistAssignments>
    ) -> some View {
        let current: [BridgeArtistAssignment]
        let inheritsAlbumArtists: Bool
        switch assignments.wrappedValue {
        case .albumArtists:
            current = []
            inheritsAlbumArtists = true
        case .explicit(let explicit):
            current = explicit
            inheritsAlbumArtists = false
        }
        return ArtistAssignmentsField(
            assignments: current,
            placeholder: String(localized: "Artist"),
            inheritsAlbumArtists: inheritsAlbumArtists,
            onUseAlbumArtists: {
                assignments.wrappedValue = .albumArtists
            },
            onChange: {
                assignments.wrappedValue = .explicit(assignments: $0)
            },
        )
    }

    private var trackOrdinalWidth: CGFloat { 34 }
    private var trackNumberWidth: CGFloat { 58 }
}

#if DEBUG
    #Preview("Edit Metadata Form") {
        @Previewable
        @State
        var form = PreviewData.editMetadataDraft(trackCount: 13)
        ScrollView {
            EditMetadataForm(form: $form)
                .padding(20)
        }
        .frame(width: 640, height: 720)
        .background(Theme.background)
        .preferredColorScheme(.dark)
        .environment(Library.stub())
        .environment(UiStore())
    }
#endif
