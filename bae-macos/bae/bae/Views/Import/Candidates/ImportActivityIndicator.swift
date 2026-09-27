import BaeKit
import SwiftUI

/// The filter row's signal that imports are waiting or running, whoever
/// started them. Its popover says how many and offers to cancel them all; an
/// import already writing its release completes regardless.
struct ImportActivityIndicator: View {
    let count: UInt32
    /// Cancel every import that has not begun writing its release.
    let onCancelAll: () -> Void

    @State
    private var detailsShown = false

    var body: some View {
        Button {
            detailsShown = true
        } label: {
            Image(systemName: "square.and.arrow.down")
                .themeIcon(ImportFilterBarLayout.glyph)
                .foregroundStyle(.secondary)
                .filterBarControl()
        }
        .buttonStyle(.plain)
        .help(String(localized: "Importing\u{2026}"))
        .popover(isPresented: $detailsShown, arrowEdge: .bottom) {
            VStack(alignment: .trailing, spacing: ThemeSpace.related) {
                HStack {
                    Text("Importing\u{2026}")
                    Spacer(minLength: ThemeSpace.group)
                    Text(verbatim: count.formatted())
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                Button("Cancel All", role: .destructive) {
                    detailsShown = false
                    onCancelAll()
                }
                .controlSize(.small)
                .help(
                    "Cancel every import that has not begun writing to the library"
                )
            }
            .themeText(.body)
            .frame(width: 220)
            .padding(ThemeSpace.group)
            .background { PopoverBehavior() }
        }
    }
}

#if DEBUG
    #Preview("Import activity") {
        ImportActivityIndicator(count: 3, onCancelAll: {})
            .padding()
            .windowBackground()
    }
#endif
