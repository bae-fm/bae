import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ImageView")

/// What an `ImageView` renders: either a one-shot `ImageLoader.Source` (an
/// import-candidate file on disk, a remote cover-art URL, in-memory bytes) or a
/// library image fetched by id and cached by content version.
enum ImageContent: Equatable {
    case source(ImageLoader.Source)
    case library(LibraryImageSource)

    /// Human-readable description for failure logs.
    var description: String {
        switch self {
        case .source(let source):
            return source.description
        case .library(let source):
            return "library image: \(source)"
        }
    }
}

struct ImageView: View {
    let content: ImageContent?
    var contentMode: ContentMode = .fill
    let pointSize: CGFloat

    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var loadState: ImageLoadState

    init(
        content: ImageContent?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.content = content
        self.contentMode = contentMode
        self.pointSize = pointSize
        _loadState = State(
            initialValue: ImageLoadState.initial(content: content)
        )
    }

    var body: some View {
        contentView
            .contentShape(Rectangle())
            .task(id: content) {
                await load()
            }
    }

    /// The image to draw this frame. A completed load wins; while a load is
    /// still pending, an already-decoded library image is served straight from
    /// the cache so the first frame after (re)mount draws the real art — the
    /// async load would otherwise land a frame later and insert the image leaf
    /// mid-flight, snapping it to its final position inside any transition the
    /// view mounted with (e.g. the queue sidebar sliding in).
    private var displayedImage: NSImage? {
        switch loadState {
        case .loaded(let image):
            return image
        case .pending:
            guard case .library(let source) = content else {
                return nil
            }
            return mediaPaths.cachedLibraryImage(
                source,
                pointSize: pointSize,
                displayScale: displayScale
            )
        }
    }

    @ViewBuilder
    private var contentView: some View {
        // `if let` rather than switching on `loadState`: the cache-served image
        // and the one the load lands are the same instance, so the view keeps
        // one structural identity across the pending → loaded flip instead of
        // swapping leaves.
        if let image = displayedImage {
            Image(nsImage: image)
                .resizable()
                .aspectRatio(contentMode: contentMode)
        }
        else if case .pending(let reason) = loadState {
            ImagePlaceholderView(reason: reason, pointSize: pointSize)
        }
    }

    private func load() async {
        loadState = ImageLoadState.initial(content: content)
        guard let content else {
            return
        }
        do {
            switch content {
            case .source(let source):
                loadState = .loaded(
                    try await ImageLoader.load(
                        source: source,
                        size: .fitTo(points: pointSize),
                        displayScale: displayScale,
                        fetchRemoteBytes: mediaPaths.fetchCoverBytes
                    )
                )
            case .library(let source):
                if let image = try await mediaPaths.libraryImage(
                    source,
                    pointSize: pointSize,
                    displayScale: displayScale
                ) {
                    loadState = .loaded(image)
                }
                else {
                    // No such image — render the unavailable placeholder.
                    loadState = .pending(.unavailable)
                }
            }
        }
        catch is CancellationError {
            return
        }
        catch {
            // OSLog redacts interpolated values to `<private>` by default
            // — the content description is a cover-art URL or an image id and
            // the error is a bridge string, neither secret. The whole point of
            // the log is to know which load failed and why.
            logger.warning(
                """
                Failed to load \
                \(content.description): \
                \(error.localizedDescription)
                """
            )
            loadState = .pending(.failed)
        }
    }
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

    var body: some View {
        switch reason {
        case .loading:
            Theme.placeholder.overlay {
                ProgressView().controlSize(.small).scaleEffect(loadingScale)
            }
        case .unavailable:
            Theme.placeholder.overlay { icon("photo", .tertiary) }
        case .failed:
            Theme.accent.opacity(0.16)
                .overlay { icon("exclamationmark.triangle.fill", Theme.accent) }
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
    /// A one-shot `ImageLoader.Source` (import-candidate file, remote cover-art
    /// URL, or in-memory bytes). A nil source renders the default placeholder.
    init(
        source: ImageLoader.Source?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            content: source.map { .source($0) },
            contentMode: contentMode,
            pointSize: pointSize
        )
    }

    /// A library cover, fetched by id and cached by content version. A nil ref
    /// (no cover) renders the default placeholder, so callers don't wrap the
    /// view in their own `if let` / `Theme.placeholder` check.
    init(
        imageRef: BridgeImageRef?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            content: imageRef.map {
                .library(.image($0))
            },
            contentMode: contentMode,
            pointSize: pointSize
        )
    }

    init(
        coverImageId: String?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            content: coverImageId.map {
                .library(.cover(id: $0, version: nil))
            },
            contentMode: contentMode,
            pointSize: pointSize
        )
    }
}
