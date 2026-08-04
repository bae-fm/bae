import BaeKit
import SwiftUI

/// The two facts an import records, stated as a sentence.
///
/// The first line is what you claim to physically hold — this pressing, or the
/// album with the pressing left open — with a muted note saying what identified
/// it. The second line names the release the metadata was read from, and shows
/// exactly when that is not the release being claimed. The claim itself is set
/// beside the action that commits it, by [`ImportClaimExactToggle`].
struct ImportClaimLine: View {
    let claim: BridgeClaimLine

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(claimSentence)
                    .font(.system(size: 12.5, weight: .medium))
                Spacer(minLength: 8)
                Text(evidenceNote)
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
            if claim.level == .approximate {
                Text(metadataSourceLine)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(Theme.field, in: RoundedRectangle(cornerRadius: 6))
    }

    /// "You have this pressing — CD · 2004 · UK · CAT-1234", or the album-level
    /// claim, which names no pressing because none is being claimed.
    private var claimSentence: String {
        guard claim.level == .exact else {
            return coreString("ui.import.claim.album")
        }
        guard let release = claim.release else {
            return coreString("ui.import.claim.pressing_undescribed")
        }
        return coreString("ui.import.claim.pressing", release)
    }

    private var evidenceNote: String {
        switch claim.evidence {
        case .discIdAlone:
            coreString("ui.import.claim.evidence.disc_id")
        case .discIdShared(let matchCount):
            coreString(
                "ui.import.claim.evidence.disc_id_shared",
                Int(matchCount)
            )
        case .barcode:
            coreString("ui.import.claim.evidence.barcode")
        case .search:
            coreString("ui.import.claim.evidence.search")
        }
    }

    /// "Metadata from 2015 · US · CD · 14 tracks". The description and the
    /// track count are separate messages so each stays a whole sentence its
    /// translators control; the `·` join is the same one bae-core used inside
    /// the description.
    ///
    /// The description alone decides which sentence is used. A track count is
    /// not a name for a release — "Metadata from 14 tracks" would name nothing
    /// — so a release with no description takes the undescribed sentence and
    /// drops the count, however well known it is.
    private var metadataSourceLine: String {
        guard let release = claim.release else {
            return coreString("ui.import.claim.metadata_from_undescribed")
        }
        let described = [
            release,
            claim.trackCount.map {
                coreString("ui.import.claim.track_count", Int($0))
            },
        ]
        .compactMap { $0 }
        .joined(separator: " \u{00b7} ")
        return coreString("ui.import.claim.metadata_from", described)
    }
}

/// The claim itself, as the one control that moves it: checked says the record
/// in the room is exactly this pressing, and clearing it says you hold the
/// album with the pressing left open.
///
/// It sits beside the action it qualifies, because it is a statement about what
/// that action commits. Setting it re-picks the same release at the level
/// chosen, which is what stores the claim and puts the release's own pressing
/// fields back — or blanks them, since an album claim states none.
struct ImportClaimExactToggle: View {
    let level: BridgeClaimLevel
    /// Whether the control takes input. Off while a read is in flight, since
    /// the level it would set is what the read is settling.
    let isReading: Bool
    let onSetLevel: (BridgeClaimLevel) -> Void

    var body: some View {
        ImportCheckboxToggle(
            core: coreString("ui.import.claim.level.exact"),
            isOn: Binding(
                get: { level == .exact },
                set: { onSetLevel($0 ? .exact : .approximate) },
            )
        )
        .disabled(isReading)
        .fixedSize()
    }
}

/// Resolve a `ui.*` catalog key out of the generated `Core` table. The claim
/// sentences live in the shared catalog because all three desktop surfaces
/// render the same words from the same `BridgeClaimLine`.
#if DEBUG
    // MARK: - Previews

    #Preview("Claim line") {
        VStack(alignment: .leading, spacing: 12) {
            ImportClaimLine(
                claim: BridgeClaimLine(
                    choice: .exact(
                        releaseId: "rel-1",
                        source: .musicBrainz
                    ),
                    level: .exact,
                    evidence: .discIdAlone,
                    release: "CD · 2004 · UK · CAT-1234",
                    trackCount: 11
                )
            )
            ImportClaimExactToggle(
                level: .exact,
                isReading: false,
                onSetLevel: { _ in },
            )
            ImportClaimLine(
                claim: BridgeClaimLine(
                    choice: .approximate(
                        releaseId: "rel-1",
                        source: .musicBrainz
                    ),
                    level: .approximate,
                    evidence: .discIdShared(matchCount: 2),
                    release: "CD · 2004 · UK",
                    trackCount: 11
                )
            )
            ImportClaimLine(
                claim: BridgeClaimLine(
                    choice: .approximate(
                        releaseId: "rel-2",
                        source: .musicBrainz
                    ),
                    level: .approximate,
                    evidence: .search,
                    release: "CD · 2015 · US",
                    trackCount: 14
                )
            )
        }
        .padding()
        .frame(width: 520)
        .windowBackground()
    }
#endif
