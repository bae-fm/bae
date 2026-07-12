import Combine
import SwiftUI

public struct QueueAddBadgeStyle {
    fileprivate let textFont: Font
    fileprivate let symbolFont: Font
    fileprivate let padding: EdgeInsets
    fileprivate let fill: Color
    fileprivate let offset: CGSize

    public init(
        textFont: Font,
        symbolFont: Font,
        padding: EdgeInsets,
        fill: Color,
        offset: CGSize
    ) {
        self.textFont = textFont
        self.symbolFont = symbolFont
        self.padding = padding
        self.fill = fill
        self.offset = offset
    }
}

public struct QueueAddBadge: View {
    private let events: AnyPublisher<Int, Never>
    private let scheduler: RunLoop
    private let style: QueueAddBadgeStyle

    @State
    private var displayedCount: Int?
    @State
    private var hideCancellable: AnyCancellable?

    public init(
        events: AnyPublisher<Int, Never>,
        scheduler: RunLoop,
        style: QueueAddBadgeStyle
    ) {
        self.events = events
        self.scheduler = scheduler
        self.style = style
    }

    public var body: some View {
        badge
            .offset(style.offset)
            .allowsHitTesting(false)
            .onReceive(events) { count in
                show(count: count)
            }
            .onDisappear {
                hideCancellable?.cancel()
                hideCancellable = nil
            }
    }

    @ViewBuilder
    private var badge: some View {
        if let displayedCount {
            HStack(spacing: 1) {
                Image(systemName: "plus")
                    .font(style.symbolFont)
                    .accessibilityHidden(true)
                Text(displayedCount, format: .number)
            }
            .font(style.textFont)
            .foregroundStyle(.white)
            .lineLimit(1)
            .fixedSize()
            .padding(style.padding)
            .background(
                Capsule(style: .continuous)
                    .fill(style.fill)
            )
            .transition(.scale(scale: 0.6).combined(with: .opacity))
        }
    }

    private func show(count: Int) {
        hideCancellable?.cancel()
        withAnimation(.spring(response: 0.3, dampingFraction: 0.6)) {
            displayedCount = count
        }
        hideCancellable =
            Just(())
            .delay(for: .milliseconds(1400), scheduler: scheduler)
            .sink { _ in
                withAnimation(.easeIn(duration: 0.2)) {
                    displayedCount = nil
                }
            }
    }
}
