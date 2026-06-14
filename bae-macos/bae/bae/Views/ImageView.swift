import AppKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ImageView")

struct ImageView: View {
    let source: ImageLoader.Source?
    var contentMode: ContentMode = .fill
    let pointSize: CGFloat

    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var loadState: ImageLoadState

    init(
        source: ImageLoader.Source?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.source = source
        self.contentMode = contentMode
        self.pointSize = pointSize
        _loadState = State(initialValue: ImageLoadState.initial(source: source))
    }

    var body: some View {
        content
            .contentShape(Rectangle())
            .task(id: source) {
                await load()
            }
    }

    @ViewBuilder
    private var content: some View {
        switch loadState {
        case .loaded(let image):
            Image(nsImage: image)
                .resizable()
                .aspectRatio(contentMode: contentMode)
        case .pending(let reason):
            ImagePlaceholderView(reason: reason, pointSize: pointSize)
        }
    }

    private func load() async {
        loadState = ImageLoadState.initial(source: source)
        guard let source else {
            return
        }
        do {
            loadState = .loaded(
                try await ImageLoader.load(
                    source: source,
                    size: .fitTo(points: pointSize),
                    displayScale: displayScale,
                    fetchRemoteBytes: mediaPaths.fetchCoverBytes
                )
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            // OSLog redacts interpolated values to `<private>` by default
            // — the source description is a cover-art URL and the error
            // is a bridge string, neither secret. The whole point of the
            // log is to know which load failed and why.
            logger.warning(
                """
                Failed to load \
                \(source.description, privacy: .public): \
                \(error.localizedDescription, privacy: .public)
                """
            )
            loadState = .pending(.failed)
        }
    }
}

enum ImageLoadState {
    case pending(PlaceholderReason)
    case loaded(NSImage)

    static func initial(source: ImageLoader.Source?) -> Self {
        .pending(source == nil ? .unavailable : .loading)
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
    /// A local image whose path may be absent. A nil path renders the default
    /// placeholder, so callers don't wrap the view in their own `if let path` /
    /// `Theme.placeholder` check.
    init(
        localPath: String?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            source: localPath.map { .local(path: $0) },
            contentMode: contentMode,
            pointSize: pointSize
        )
    }
}
