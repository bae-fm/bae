import SwiftUI

/// The shape a group card and its pressing rows will take, drawn as bars while
/// the lookups run. It holds the result area's height so the pane does not
/// jump when the first matches land.
struct ResultSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            ForEach(0..<3, id: \.self) { _ in
                card
            }
            Spacer(minLength: 0)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityHidden(true)
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                bar(width: 48, height: 48, radius: 7, opacity: 0.05)
                VStack(alignment: .leading, spacing: 6) {
                    bar(width: 160, height: 11, radius: 4, opacity: 0.06)
                    bar(width: 110, height: 9, radius: 4, opacity: 0.045)
                }
            }
            HStack(spacing: 10) {
                bar(width: 40, height: 9, radius: 4, opacity: 0.05)
                bar(width: 120, height: 9, radius: 4, opacity: 0.045)
                bar(width: 70, height: 9, radius: 4, opacity: 0.04)
            }
            .padding(.leading, 22)
            HStack(spacing: 10) {
                bar(width: 40, height: 9, radius: 4, opacity: 0.05)
                bar(width: 90, height: 9, radius: 4, opacity: 0.045)
            }
            .padding(.leading, 22)
        }
    }

    private func bar(
        width: CGFloat,
        height: CGFloat,
        radius: CGFloat,
        opacity: Double
    ) -> some View {
        RoundedRectangle(cornerRadius: radius)
            .fill(.white.opacity(opacity))
            .frame(width: width, height: height)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Result skeleton") {
        ResultSkeleton()
            .frame(width: 620, height: 420)
            .windowBackground()
    }
#endif
