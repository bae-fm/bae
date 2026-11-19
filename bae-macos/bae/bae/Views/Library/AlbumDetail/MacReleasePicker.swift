import BaeKit
import SwiftUI

/// The choice of an album's release in its header: one segment per release,
/// with a speaker on the release that is playing.
struct MacReleasePicker: View {
    /// The album and its releases as its detail read returned them.
    let detail: AlbumDetail
    /// The release of the track playing now, which gets the speaker.
    let currentReleaseId: String?
    /// Show the release with this id.
    let onSelect: (String) -> Void

    var body: some View {
        // The segments and the binding read the same releases, so a clicked
        // segment's index names one of them.
        let releases = detail.releases
        MacSegmentedControl(
            selectedIndex: Binding(
                get: { detail.shownReleaseIndex },
                set: { newIndex in onSelect(releases[newIndex].id) },
            ),
            segments: releases.map { release in
                MacSegmentedControl.Segment(
                    label: release.displayName,
                    systemImage: release.id == currentReleaseId
                        ? "speaker.fill" : nil,
                )
            },
        )
        .padding(.top, ThemeSpace.inline)
        .padding(.bottom, ThemeSpace.line)
    }
}
