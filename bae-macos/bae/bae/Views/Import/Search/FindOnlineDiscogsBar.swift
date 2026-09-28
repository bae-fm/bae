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
        HStack(alignment: .center, spacing: ThemeSpace.related) {
            HStack(alignment: .top, spacing: ThemeSpace.related) {
                if let symbol = StatusTone.info.symbol {
                    Image(systemName: symbol)
                        .themeIcon(.medium)
                        .foregroundStyle(StatusTone.info.color)
                        .padding(.top, ThemeSpace.hairline)
                }
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
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
            Spacer(minLength: ThemeSpace.group)
            Button("Open Settings", action: onOpenSettings)
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
                    .themeIcon(.small)
                    .foregroundStyle(.secondary)
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Dismiss"))
        }
        .notice(.info)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
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
