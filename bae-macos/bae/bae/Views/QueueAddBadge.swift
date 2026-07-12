import SwiftUI

/// Capsule "+N" badge overlaid on the queue button when tracks are added.
/// Renders nothing when `displayedCount` is nil; the parent flips the
/// binding to nil after the hold timer to fade the badge out.
struct QueueAddBadge: View {
    @Binding
    var displayedCount: Int?

    var body: some View {
        if let count = displayedCount {
            // A bare count, not chrome: render with the locale's digits via
            // `.formatted()` and `verbatim` so it doesn't become a catalog key.
            Text(verbatim: "+\(count.formatted())")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background(
                    Capsule(style: .continuous)
                        .fill(Color.accentColor)
                )
                .transition(
                    .scale(scale: 0.6).combined(with: .opacity)
                )
        }
    }
}

#if DEBUG
    #Preview("Single") {
        QueueAddBadge(displayedCount: .constant(1))
            .padding(20)
    }

    #Preview("Many") {
        QueueAddBadge(displayedCount: .constant(12))
            .padding(20)
    }

    #Preview("Hidden") {
        QueueAddBadge(displayedCount: .constant(nil))
            .padding(20)
    }
#endif
