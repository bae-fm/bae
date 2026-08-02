import BaeKit
import SwiftUI

/// Section 1 of the mapping pane: what the folder is being read as.
///
/// The release ⇄ Unknown control sits here, always visible, because it is the
/// question this section answers — not a link inside the search. Both sides
/// leave a mapping table to work in: a release names the tracklist, and
/// Unknown reads it off the folder's own files.
struct ImportIdentitySection: View {
    let identity: ImportIdentity
    let title: String
    let artist: String
    /// "CD · 1996 · 9 tracks", from what is being edited rather than what was
    /// fetched.
    let metaLine: String
    /// What this import claims to hold and where its metadata came from, as
    /// core derived it. `nil` before a pick, and for Unknown, which claims
    /// nothing.
    let claim: BridgeClaimLine?
    /// Whether a release has been picked — what the change control reads as.
    let hasPick: Bool
    /// Whether a read is in flight. The controls that start one read as pending
    /// rather than replacing the section with a placeholder.
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    /// The album-level fields, behind a disclosure. `nil` until something has
    /// been settled for this folder and there is a release to edit.
    let editor: Binding<BridgeRawReleaseEdit>?
    let onSetIdentity: (ImportIdentity) -> Void
    let onFindRelease: () -> Void
    let onEditCover: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: coreString("ui.import.identity.title"))
            VStack(alignment: .leading, spacing: 10) {
                identityPicker
                ImportReleaseHeader(
                    title: title,
                    artist: artist,
                    metaLine: metaLine,
                    claim: claim,
                    hasPick: hasPick,
                    isReading: isReading,
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
