import BaeKit
import SwiftUI

/// One signal badge: a type icon, a label, the value (mono, middle-truncated),
/// and a trailing status. An active badge carries an accent ring; an excluded
/// badge dims and strikes through but stays in place. Hovering shows a popover
/// explaining the signal's origin, role, full value, state, and the
/// click-to-toggle hint.
struct SignalBadge: View {
    let signal: BridgeToolbarSignal
    let onToggle: () -> Void

    @State
    private var hovering = false

    /// A confirming catalog (a filter that matched a pressing) pins the badge
    /// with an accent ring + check.
    private var isPinned: Bool {
        if case .confirms(let count) = signal.state {
            return count > 0 && !signal.excluded
        }
        return false
    }

    /// The accent ring shows on an active (not excluded) badge. A pinned
    /// catalog gets a stronger ring (handled in the overlay).
    private var isActive: Bool {
        !signal.excluded
    }

    var body: some View {
        Button(action: onToggle) {
            HStack(spacing: 7) {
                Image(systemName: SignalBadgeStyle.icon(for: signal.kind))
                    .font(.system(size: 12))
                    .foregroundStyle(iconColor)
                Text(SignalBadgeStyle.label(for: signal.kind))
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(.primary)
                    .strikethrough(signal.excluded, color: .secondary)
                if let value = signal.value {
                    Text(value)
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .strikethrough(signal.excluded, color: .secondary)
                        .frame(maxWidth: 160, alignment: .leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
                status
            }
            .padding(.leading, 9)
            .padding(.trailing, 8)
            .frame(height: 30)
            .background(badgeBackground)
            .overlay(badgeRing)
            .opacity(signal.excluded ? 0.45 : 1)
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .popover(isPresented: $hovering, arrowEdge: .bottom) {
            SignalBadgePopover(signal: signal)
                // Chips sit above the popover, which grows downward — its
                // visual anchor is its top edge. Exit stays instant, like a
                // tooltip's.
                .popoverEntrance(anchor: .top)
                .background { PopoverBehavior() }
        }
    }

    private var iconColor: Color {
        if signal.excluded {
            return .secondary
        }
        return isPinned || isActive ? Theme.accent : .secondary
    }

    private var badgeBackground: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(Color.white.opacity(0.04))
    }

    private var badgeRing: some View {
        RoundedRectangle(cornerRadius: 8)
            .stroke(
                isPinned
                    ? Theme.accent
                    : (isActive
                        ? Theme.accent.opacity(0.35) : .white.opacity(0.08)),
                lineWidth: isPinned ? 1.2 : 1
            )
    }

    @ViewBuilder
    private var status: some View {
        if signal.excluded {
            // Excluded: a hollow off-dot with an X.
            Image(systemName: "xmark")
                .font(.system(size: 8, weight: .bold))
                .foregroundStyle(.tertiary)
                .frame(width: 14, height: 14)
                .overlay(Circle().stroke(.tertiary, lineWidth: 1.5))
        }
        else {
            switch signal.state {
            case .lookingUp:
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.6)
                    .frame(width: 16, height: 16)
            case .found(let count):
                countCapsule(count.formatted(), tone: .green)
            case .confirms(let count):
                if count > 0 {
                    Image(systemName: "checkmark")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(Theme.accent)
                        .frame(minWidth: 19, minHeight: 19)
                        .background(
                            Theme.accent.opacity(0.18),
                            in: RoundedRectangle(cornerRadius: 6)
                        )
                }
                else {
                    countCapsule(0.formatted(), tone: .muted)
                }
            case .noMatch:
                countCapsule(0.formatted(), tone: .muted)
            case .skipped:
                Image(systemName: "minus")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .frame(width: 19, height: 19)
            case .failed:
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
                    .frame(width: 19, height: 19)
            }
        }
    }

    private enum StatusTone {
        case green
        case muted
    }

    private func countCapsule(_ text: String, tone: StatusTone) -> some View {
        Text(text)
            .font(.system(size: 11.5, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(tone == .green ? Color.green : Color.secondary)
            .frame(minWidth: 19, minHeight: 19)
            .padding(.horizontal, 6)
            .background(
                tone == .green
                    ? Color.green.opacity(0.14) : .white.opacity(0.05),
                in: RoundedRectangle(cornerRadius: 6)
            )
    }
}
