import BaeKit
import SwiftUI

/// The standing notice that Discogs is not among the sources Find online can
/// ask. It sits under the pane's header, above both section headers, because
/// it is equally true of the automatic run and the typed search: neither one
/// asked Discogs, and nothing else on the pane says why its checkbox in the
/// header cannot be ticked.
///
/// Whether Discogs is usable is core's answer, read live from the config the
/// app already observes — adding a token in Settings takes the bar away
/// without the pane asking again.
struct FindOnlineDiscogsBar: View {
    /// Open Settings on the Discogs page.
    let onOpenSettings: () -> Void
    /// Put the bar away for the rest of this run of the app.
    let onDismiss: () -> Void

    private var discogs: String {
        bridgeMetadataSourceName(source: .discogs)
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "info.circle")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                Text("\(discogs) not configured")
                    .font(.system(size: 12, weight: .semibold))
                Text("Add a \(discogs) token to look up its releases too.")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            Button("Open Settings", action: onOpenSettings)
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 16, height: 16)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Dismiss"))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Theme.surface)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Discogs not configured") {
        FindOnlineDiscogsBar(onOpenSettings: {}, onDismiss: {})
            .frame(width: 660)
            .windowBackground()
    }
#endif
