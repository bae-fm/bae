import BaeKit
import SwiftUI

/// The two sections of the Find online accordion. One is open at a time;
/// each section's results render under the header that produced them.
enum FindOnlineSection: Equatable {
    /// Identification: the run's ledger and what it matched.
    case automatic
    /// The typed search: its form and what it turned up.
    case search
}

/// The one status glyph a section header carries, open or collapsed.
enum FindOnlineSectionGlyph: Equatable {
    /// Nothing has run yet.
    case none
    /// A lookup is under way.
    case working
    /// Done, with matches.
    case matched
    /// Done, and nothing matched.
    case empty
    /// A lookup failed.
    case failed
    /// Nothing to run.
    case nothing

    /// What identification has to say about itself.
    init(identifyState: IdentifyState) {
        switch identifyState {
        case .idle:
            self = .none
        case .triangulating:
            self = .working
        case .found(_, let groups, _, _, _, _, _):
            self = groups.isEmpty ? .empty : .matched
        case .notFoundAnywhere:
            self = .empty
        case .manualOnly:
            self = .nothing
        case .failed:
            self = .failed
        }
    }

    /// What the typed search has to say about itself; nothing before one is
    /// submitted.
    init(search: BridgeCandidateSearch?) {
        guard let search else {
            self = .none
            return
        }
        switch search.status {
        case .searching: self = .working
        case .found: self = .matched
        case .noMatches: self = .empty
        case .failed: self = .failed
        }
    }

    /// Whether the glyph says the section has nothing to show: a collapsed
    /// section reading this dims, so the open one beside it reads as the
    /// place to look.
    var isVacant: Bool {
        switch self {
        case .empty, .nothing: true
        case .none, .working, .matched, .failed: false
        }
    }
}

/// One section's header row: the chevron saying whether it is open, the
/// section's name in caps, and its status glyph at the right edge. Clicking
/// a collapsed header opens the section and collapses the other.
struct FindOnlineSectionHeader: View {
    let section: FindOnlineSection
    let isOpen: Bool
    let glyph: FindOnlineSectionGlyph
    let onOpen: () -> Void

    @State
    private var isHovered = false

    private var dimmed: Bool {
        !isOpen && glyph.isVacant
    }

    var body: some View {
        // The open section's header is not a control: clicking it changes
        // nothing, and it stays fully drawn rather than dimming as disabled.
        Button(action: { if !isOpen { onOpen() } }) {
            HStack(spacing: 8) {
                Image(systemName: isOpen ? "chevron.down" : "chevron.right")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(Color.primary.opacity(dimmed ? 0.3 : 0.45))
                    .frame(width: 8)
                FindOnlineCapsLabel(
                    section == .automatic ? "Automatic" : "Search"
                )
                .opacity(dimmed ? 0.55 : 1)
                Spacer(minLength: 8)
                FindOnlineSectionGlyphView(glyph: glyph)
            }
            .padding(.horizontal, 14)
            .frame(height: 32)
            .background(
                Theme.hover.opacity(isHovered && !isOpen ? 0.5 : 0)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
        .accessibilityLabel(
            section == .automatic
                ? Text("Automatic") : Text("Search")
        )
        .accessibilityAddTraits(isOpen ? [.isSelected] : [])
    }
}

/// The glyph itself, 12 points square. Every case stays the same size so the
/// header holds still as a run goes from spinning to settled.
struct FindOnlineSectionGlyphView: View {
    let glyph: FindOnlineSectionGlyph

    var body: some View {
        ZStack {
            switch glyph {
            case .none:
                EmptyView()
            case .working:
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.6)
            case .matched:
                Image(systemName: "checkmark.circle")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.green)
            case .empty:
                CountCapsule(count: 0)
            case .failed:
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
            case .nothing:
                RoundedRectangle(cornerRadius: 1)
                    .fill(Color.primary.opacity(0.28))
                    .frame(width: 8, height: 1.5)
            }
        }
        .frame(minWidth: 12, minHeight: 12)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Section headers") {
        VStack(spacing: 0) {
            FindOnlineSectionHeader(
                section: .automatic,
                isOpen: true,
                glyph: .working,
                onOpen: {}
            )
            Divider()
            FindOnlineSectionHeader(
                section: .search,
                isOpen: false,
                glyph: .none,
                onOpen: {}
            )
            Divider()
            FindOnlineSectionHeader(
                section: .automatic,
                isOpen: false,
                glyph: .empty,
                onOpen: {}
            )
            Divider()
            FindOnlineSectionHeader(
                section: .automatic,
                isOpen: false,
                glyph: .matched,
                onOpen: {}
            )
            Divider()
            FindOnlineSectionHeader(
                section: .automatic,
                isOpen: false,
                glyph: .failed,
                onOpen: {}
            )
            Divider()
            FindOnlineSectionHeader(
                section: .automatic,
                isOpen: false,
                glyph: .nothing,
                onOpen: {}
            )
        }
        .frame(width: 660)
        .windowBackground()
    }
#endif
