import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ImageView")

struct ImageView: View {
    let content: ImageContent?
    var contentMode: ContentMode = .fill
    let pointSize: CGFloat

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
        pointSize: CGFloat
    ) {
        self.content = content
        self.contentMode = contentMode
        self.pointSize = pointSize
        _lastLoad = State(
            initialValue: SlotLoad(
                content: content,
                state: .initial(content: content)
            )
        )
    }

    private var loadState: ImageLoadState {
        lastLoad.content == content
            ? lastLoad.state : .initial(content: content)
    }

    var body: some View {
        contentView
            .contentShape(Rectangle())
            .task(id: LoadRequest(content: content, attempt: attempt)) {
                await load()
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
        lastLoad = SlotLoad(
            content: requested,
            state: .initial(content: requested)
        )
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
                lastLoad = SlotLoad(
                    content: requested,
                    state: .pending(.unavailable)
                )
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
                \(error.localizedDescription)
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

    static func initial(content: ImageContent?) -> Self {
        .pending(content == nil ? .unavailable : .loading)
    }
}

enum PlaceholderReason {
    case loading
    case unavailable
    case failed
}

struct ImagePlaceholderView: View {
    let reason: PlaceholderReason
    let pointSize: CGFloat
    /// Asks for a failed load again.
    let retry: () -> Void

    var body: some View {
        switch reason {
        case .loading:
            Rectangle().fill(Theme.placeholder)
                .overlay {
                    ProgressView().controlSize(.small).scaleEffect(loadingScale)
                }
        case .unavailable:
            Rectangle().fill(Theme.placeholder)
                .overlay { icon("photo", .tertiary) }
        case .failed:
            Button(action: retry) {
                Theme.accentSoft
                    .overlay {
                        icon("arrow.clockwise.circle.fill", Theme.accent)
                    }
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(
                String(
                    localized: "Couldn't load the image. Click to try again."
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

    private var loadingScale: CGFloat {
        usesCompactChrome ? 0.75 : 0.85
    }

    private var iconFont: Font {
        .system(size: usesCompactChrome ? 17 : 22, weight: .medium)
    }

    private func icon<S: ShapeStyle>(
        _ systemName: String,
        _ foregroundStyle: S
    ) -> some View {
        Image(systemName: systemName)
            .font(iconFont)
            .foregroundStyle(foregroundStyle)
    }
}

extension ImageView {
    /// A curated library image; a nil ref renders the default placeholder.
    init(
        imageRef: BridgeImageRef?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            content: imageRef.map { .libraryImage($0) },
            contentMode: contentMode,
            pointSize: pointSize
        )
    }
}

#if DEBUG
    #Preview("Image View") {
        // The stub store resolves no bytes, so every slot shows a placeholder.
        HStack(alignment: .top, spacing: 16) {
            ImageView(imageRef: nil, pointSize: 120)
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 10))
            ImageView(
                imageRef: BridgeImageRef(
                    id: "preview-cover",
                    version: "1",
                    imageType: .cover
                ),
                pointSize: 120
            )
            .frame(width: 120, height: 120)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            ImageView(content: nil, pointSize: 44)
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .padding(28)
        .background(Theme.background)
        .environment(ImageStore.stub())
        .preferredColorScheme(.dark)
    }
#endif
