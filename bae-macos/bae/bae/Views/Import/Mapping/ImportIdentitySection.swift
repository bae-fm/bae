import BaeKit
import SwiftUI

/// The pressings identification matched, offered inline while nothing is
/// picked — the question the section is asking, so the answer choices render
/// right under it rather than behind the search sheet.
struct ImportMatchOptions {
    /// One card per release group the matches fall into, in match order.
    let groups: [ReleaseGroup]
    let libraryStatuses: [String: BridgeLibraryStatus]
    let provenance: [String: BridgeResultProvenance]
    let isImporting: Bool
    /// The pressing whose pick is being read right now, carrying the row
    /// spinner. `nil` when nothing is in flight.
    let loadingReleaseId: String?
    let onSelect: (BridgeMetadataResult) -> Void
}

/// Section 1 of the mapping pane: what the folder is being read as.
///
/// The release ⇄ Unknown control sits here, always visible, because it is the
/// question this section answers — not a link inside the search. Both sides
/// leave a mapping table to work in: a release names the tracklist, and
/// Unknown reads it off the folder's own files.
struct ImportIdentitySection: View {
    let identity: ImportIdentity
    /// The folder on disk — its own line, not the release title: what the
    /// folder is called and what the release is are different facts, and the
    /// card leads with the release.
    let folderName: String
    /// The folder's audio shape ("FLAC", "CUE+FLAC"), shown beside its name.
    let formatLabel: String
    let title: String
    let artist: String
    /// "CD · 1996 · 9 tracks", from what is being edited rather than what was
    /// fetched.
    let metaLine: String
    /// What identified the picked release. `nil` before a pick, and for a
    /// folder read as its own tags, which nothing looked up.
    let evidence: BridgeClaimEvidence?
    /// Whether a release has been picked — what the change control reads as.
    let hasPick: Bool
    /// Whether a read is in flight. The controls that start one read as pending
    /// rather than replacing the section with a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    /// The album-level fields, behind the card's own disclosure. `nil` until
    /// something has been settled for this folder and there is a release to
    /// edit.
    let editValues: BridgeRawReleaseEdit?
    /// Where a typed field's value goes.
    let editActions: ReleaseFieldWriter
    /// Identification's unresolved matches, offered inline. `nil` once a pick
    /// is in, when the folder reads as Unknown, or when there is nothing
    /// matched to offer.
    let matchOptions: ImportMatchOptions?
    /// Whether anything has been settled for this folder — a release picked or
    /// its own tags read. Nothing settled and no matches to offer leaves only
    /// the one thing to do: find the release.
    let hasSettled: Bool
    /// The card's commit row, once there is something to commit.
    let commit: ImportCommitControls?
    let onSetIdentity: (ImportIdentity) -> Void
    let onFindRelease: () -> Void
    let onEditCover: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: coreString("ui.import.identity.title"))
            VStack(alignment: .leading, spacing: 10) {
                folderLine
                identityPicker
                if let matchOptions {
                    // The matched release groups with their pressing rows —
                    // the release header would only repeat the folder name
                    // over them. Manual search stays a step away.
                    ForEach(matchOptions.groups) { group in
                        ReleaseGroupSection(
                            group: group,
                            isImporting: matchOptions.isImporting,
                            libraryStatuses: matchOptions.libraryStatuses,
                            provenance: matchOptions.provenance,
                            selectedReleaseId: nil,
                            loadingReleaseId: matchOptions.loadingReleaseId,
                            onSelect: matchOptions.onSelect,
                        )
                    }
                    Button(coreString("ui.import.header.find_release")) {
                        onFindRelease()
                    }
                    .buttonStyle(.bordered)
                    .disabled(isReading)
                }
                else if hasSettled {
                    ImportReleaseHeader(
                        title: title,
                        artist: artist,
                        metaLine: metaLine,
                        evidence: evidence,
                        hasPick: hasPick,
                        isReading: isReading,
                        coverContent: coverContent,
                        hasCoverOptions: hasCoverOptions,
                        editValues: editValues,
                        editActions: editActions,
                        commit: commit,
                        onEditCover: onEditCover,
                        onFindRelease: onFindRelease,
                    )
                }
                else {
                    // Nothing settled and nothing matched: the folder line
                    // above already says what this is, so no card — just the
                    // one thing left to do.
                    HStack(spacing: 8) {
                        ProgressView()
                            .controlSize(.small)
                            .opacity(isReading ? 1 : 0)
                        Button(coreString("ui.import.header.find_release")) {
                            onFindRelease()
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .disabled(isReading)
                }
            }
        }
    }

    /// The folder itself: name in mono, its audio shape beside it. Always
    /// present, whatever the release side is showing.
    private var folderLine: some View {
        HStack(spacing: 6) {
            Image(systemName: "folder")
                .font(.caption)
                .foregroundStyle(.tertiary)
            Text(folderName)
                .font(.system(size: 11.5, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            if !formatLabel.isEmpty {
                Text(formatLabel)
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
        }
    }

    /// The one control that switches sides. Picking Unknown reads the folder's
    /// own tags; picking Release re-picks the release the candidate already
    /// holds, or opens the search when it holds none.
    private var identityPicker: some View {
        Picker(
            coreString("ui.import.identity.title"),
            selection: Binding(
                get: { identity },
                set: { onSetIdentity($0) },
            )
        ) {
            Text(coreString("ui.import.identity.release"))
                .tag(ImportIdentity.release)
            Text(coreString("ui.import.identity.unknown"))
                .tag(ImportIdentity.unknown)
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .disabled(isReading)
        .fixedSize()
    }
}
