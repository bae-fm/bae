import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("Artwork")

struct Artwork: View {
    enum Appearance {
        case plain
        case albumTile(open: Bool, dimmed: Bool)
    }

    let content: ImageContent?
    var contentMode: ContentMode = .fill
    let pointSize: CGFloat
    let appearance: Appearance

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
    /// The last load's outcome and its content; a slot whose content has since
    /// changed reads as not loaded yet.
    @State
    private var lastLoad: SlotLoad
    /// How many times the person retried a failed load; part of the load's
    /// identity, so a retry restarts it.
    @State
    private var attempt = 0

    init(
        content: ImageContent?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat,
        appearance: Appearance = .plain
    ) {
        self.content = content
        self.contentMode = contentMode
        self.pointSize = pointSize
        self.appearance = appearance
        _lastLoad = State(
            initialValue: SlotLoad(content: content, state: .pending(.empty))
        )
    }

    private var loadState: ImageLoadState {
        lastLoad.content == content ? lastLoad.state : .pending(.empty)
    }

    var body: some View {
        artwork
            .contentShape(Rectangle())
            .task(id: LoadRequest(content: content, attempt: attempt)) {
                await load()
            }
    }

    @ViewBuilder
    private var artwork: some View {
        switch appearance {
        case .plain:
            contentView
        case .albumTile(let open, let dimmed):
            albumTile(open: open, dimmed: dimmed)
        }
    }

    /// The cover, shadow and open ring that the album grid draws together.
    private func albumTile(open: Bool, dimmed: Bool) -> some View {
        let radius = ThemeRadius.cover
        let ringGap: CGFloat = 4.5
        let ringWidth: CGFloat = 3
        let outset = ringGap + ringWidth / 2
        return
            contentView
            .frame(width: pointSize, height: pointSize)
            .overlay {
                Rectangle()
                    .fill(Theme.background)
                    .opacity(dimmed ? 1 - ThemeOpacity.dimmed : 0)
                    .allowsHitTesting(false)
            }
            .clipShape(RoundedRectangle(cornerRadius: radius, style: .circular))
            .shadow(color: open ? .clear : Theme.shadow, radius: 14, y: 9)
            .background {
                RoundedRectangle(
                    cornerRadius: radius + ringGap,
                    style: .circular
                )
                .inset(by: -ringGap)
                .fill(Theme.background)
                .opacity(open ? 1 : 0)
            }
            .overlay {
                RoundedRectangle(
                    cornerRadius: radius + outset,
                    style: .circular
                )
                .inset(by: -outset)
                .stroke(Theme.accent, lineWidth: ringWidth)
                .opacity(open ? 1 : 0)
                .allowsHitTesting(false)
            }
    }

    /// A completed load wins; while pending, an already-decoded image comes
    /// straight from the store so the first frame after mount draws the art.
    private var displayedImage: NSImage? {
        switch loadState {
        case .loaded(let image):
            return image
        case .pending:
            guard let content else {
                return nil
            }
            return imageStore.cachedImage(
                content,
                pointSize: pointSize,
                displayScale: displayScale
            )
        }
    }

    /// Drawn before a bitmap is available; nonzero so `aspectRatio` never sees
    /// 0/0.
    private static let emptyImage = NSImage(size: NSSize(width: 1, height: 1))

    private var contentView: some View {
        let image = displayedImage
        // The art view must exist from the first frame, since a view inserted
        // mid-animation snaps to its final position.
        return ZStack {
            placeholderView
                .opacity(image == nil ? 1 : 0)
            Image(nsImage: image ?? Self.emptyImage)
                .resizable()
                .aspectRatio(contentMode: contentMode)
                // Without art the layer is only a stand-in; clicks reach the
                // placeholder beneath it, whose failed state is a button.
                .allowsHitTesting(image != nil)
        }
    }

    @ViewBuilder
    private var placeholderView: some View {
        if case .pending(let reason) = loadState {
            ImagePlaceholderView(
                reason: reason,
                pointSize: pointSize,
                retry: { attempt += 1 }
            )
        }
    }

    private func load() async {
        let requested = content
        lastLoad = SlotLoad(content: requested, state: .pending(.empty))
        guard let requested else {
            return
        }
        do {
            if let image = try await imageStore.image(
                requested,
                pointSize: pointSize,
                displayScale: displayScale
            ) {
                lastLoad = SlotLoad(content: requested, state: .loaded(image))
            }
            else {
                lastLoad = SlotLoad(content: requested, state: .pending(.empty))
            }
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                """
                Failed to load \
                \(requested.description): \
                \(String(reflecting: error))
                """
            )
            lastLoad = SlotLoad(content: requested, state: .pending(.failed))
        }
    }
}

// periphery:ignore - a `.task(id:)` identity: compared, never read.
/// One load of a slot: the content, and which time of asking.
private struct LoadRequest: Equatable {
    let content: ImageContent?
    let attempt: Int
}

/// A load's outcome and the content it was for.
private struct SlotLoad {
    let content: ImageContent?
    let state: ImageLoadState
}

enum ImageLoadState {
    case pending(PlaceholderReason)
    case loaded(NSImage)
}

enum PlaceholderReason {
    /// No image to draw yet, or none to load: a plain tile either way.
    case empty
    case failed
}

struct ImagePlaceholderView: View {
    let reason: PlaceholderReason
    let pointSize: CGFloat
    /// Asks for a failed load again.
    let retry: () -> Void

    var body: some View {
        switch reason {
        case .empty:
            Rectangle().fill(Theme.missingArt)
        case .failed:
            Button(action: retry) {
                Theme.accentSoft
                    .overlay {
                        Image(systemName: "arrow.clockwise.circle.fill")
                            .font(retryIconFont)
                            .foregroundStyle(Theme.accent)
                    }
                    .contentShape(Rectangle())
            }
            .buttonStyle(.accentPlain)
            .help(
                String(
                    localized: "Could not load the image. Click to try again."
                )
            )
            .accessibilityLabel(
                String(localized: "Try loading the image again")
            )
        }
    }

    private var usesCompactChrome: Bool {
        pointSize < 56
    }

    private var retryIconFont: Font {
        (usesCompactChrome ? ThemeIcon.medium : ThemeIcon.large).font
    }
}

extension Artwork {
    /// A curated library image; a nil ref renders the default placeholder.
    init(
        imageRef: BridgeImageRef?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat,
        appearance: Appearance = .plain
    ) {
        self.init(
            content: imageRef.map { .libraryImage($0) },
            contentMode: contentMode,
            pointSize: pointSize,
            appearance: appearance
        )
    }
}
