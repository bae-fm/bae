import BaeKit
import SwiftUI

/// The one status glyph a section header carries, open or collapsed.
enum FindOnlineSectionGlyph: Equatable {
    /// Nothing to report: nothing has run yet, or it ran and found
    /// matches, which are the section's own content.
    case none
    /// A lookup is under way.
    case working
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
        case .found(_, let groups, _, _, _, _, _, _, _):
            self = groups.isEmpty ? .empty : .none
        case .notFoundAnywhere:
            self = .empty
        case .manualOnly:
            self = .nothing
        case .error:
            self = .failed
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
        case .found: self = .none
        case .noMatches: self = .empty
        case .failed: self = .failed
        }
    }

    /// Whether the section has nothing to show, which dims it when collapsed.
    var isVacant: Bool {
        switch self {
        case .empty, .nothing: true
        case .none, .working, .failed: false
        }
    }
}

/// A section's header: its open-or-closed chevron, name, and status glyph.
/// Clicking a collapsed header opens that section.
struct FindOnlineSectionHeader: View {
    let section: BridgeFindOnlineSection
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
            HStack(spacing: ThemeSpace.related) {
                Image(systemName: isOpen ? "chevron.down" : "chevron.right")
                    .themeIcon(.small)
                    .foregroundStyle(dimmed ? .quaternary : .tertiary)
                    .frame(width: ThemeIcon.small.size)
                Eyebrow(section == .automatic ? "Automatic" : "Search")
                    .opacity(dimmed ? 0.55 : 1)
                Spacer(minLength: ThemeSpace.related)
                FindOnlineSectionGlyphView(glyph: glyph)
            }
            .padding(.horizontal, ThemeSpace.group)
            .frame(height: 32)
            .background(
                isHovered && !isOpen ? Theme.hover : Color.clear
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

/// The status glyph, at least a small glyph's square so the header holds still
/// as it changes.
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
            case .empty:
                StatusChip(verbatim: 0.formatted())
            case .failed:
                Image(systemName: "exclamationmark.triangle")
                    .themeIcon(.small)
                    .foregroundStyle(Theme.warning)
            case .nothing:
                IdentifierDash()
            }
        }
        .frame(
            minWidth: ThemeIcon.small.size,
            minHeight: ThemeIcon.small.size
        )
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
