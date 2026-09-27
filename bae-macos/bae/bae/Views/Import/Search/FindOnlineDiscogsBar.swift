import BaeKit
import SwiftUI

/// The notice that Discogs is not among the sources Find online can ask,
/// shown above both sections because it holds for each of them.
struct FindOnlineDiscogsBar: View {
    /// Open Settings on the Discogs page.
    let onOpenSettings: () -> Void
    /// Put the bar away for the rest of this run of the app.
    let onDismiss: () -> Void

    private var discogs: String {
        bridgeCatalogName(catalog: .discogs)
    }

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "info.circle.fill")
                    .font(.system(size: 13))
                    .foregroundStyle(NoticeTone.info.tint)
                    .padding(.top, 1)
                VStack(alignment: .leading, spacing: 3) {
                    Text("Add \(discogs) to find more pressings")
                        .themeText(.strong)
                    Text(
                        "Many regional releases, reissues and vinyl pressings are only on \(discogs). Tokens are free: create one in your \(discogs) account settings."
                    )
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                }
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
        .padding(.horizontal, 18)
        .padding(.vertical, 14)
        .background(NoticeTone.info.fill)
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
