import AppKit
import BaeKit
import OSLog
import SwiftUI
import VisionKit

private let logger = Logger.bae("LightboxView")
private let navButtonDiameter: CGFloat = 48

/// An image the lightbox shows; the strip and the stage each draw it at their
/// own size.
protocol LightboxImage: Identifiable, Equatable {
    var label: String { get }
    var sourceLabel: String { get }
    var image: ImageContent { get }
}

struct LightboxItem: LightboxImage {
    let id: String
    let label: String
    let image: ImageContent

    init(label: String, file: BridgeFileVersion) {
        self.init(
            id: file.path,
            label: label,
            image: .localFile(file)
        )
    }

    init(id: String, label: String, image: ImageContent) {
        self.id = id
        self.label = label
        self.image = image
    }

    var sourceLabel: String { String(localized: "Release Files") }
}

struct LightboxView<Item: LightboxImage>: View {
    let cursor: Cursor<Item>
    let onUpdate: (Cursor<Item>) -> Void
    let onDismiss: () -> Void
    var onBrowseAll: (() -> Void)?

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var magnification: CGFloat = 1.0
    @State
    private var magnifyAnchor: UnitPoint = .center
    @State
    private var loadedImage: NSImage?
    @State
    private var fullResImage: NSImage?
    /// The current item's decode source, kept so the full-resolution decode on
    /// zoom never crosses the bridge again.
    @State
    private var decodeSource: ImageLoader.Source?
    @State
    private var fullResImageUpgradeTask: Task<Void, Never>?
    @State
    private var imageAnalysis: ImageAnalysis?
    @State
    private var loadFailed = false
    /// How many times the person retried a failed load; part of the load's
    /// identity, so a retry restarts it.
    @State
    private var attempt = 0
    @FocusState
    private var focused: Bool

    @State
    private var analyzer = ImageAnalyzer()

    var body: some View {
        ZStack {
            Theme.backdrop
                .ignoresSafeArea()
                .contentShape(Rectangle())
                .onTapGesture { onDismiss() }

            VStack(spacing: 0) {
                GeometryReader { geo in
                    ZStack {
                        imageContent
                        if cursor.canCycle {
                            cycleNavButtons
                        }
                        closeButtonOverlay
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .task(
                        id: LightboxLoad(
                            image: cursor.current.image,
                            attempt: attempt
                        )
                    ) {
                        await loadCurrentImage(containerSize: geo.size)
                    }
                }

                labelView

                if cursor.canCycle {
                    ThumbnailStrip(
                        cursor: cursor,
                        centered: true,
                        onSelect: { id in
                            var next = cursor
                            next.select(id: id)
                            onUpdate(next)
                        },
                        stroke: { _, isActive in
                            (
                                isActive
                                    ? Theme.onFill : Theme.onFillSecondary,
                                isActive ? 2 : 1
                            )
                        }
                    ) { item in
                        ImageView(
                            content: item.image,
                            pointSize: ThumbnailStripLayout.thumbnailSize
                        )
                    }
                    .padding(.bottom, ThemeSpace.group)
                    .fadesWhenZoomed(at: magnification)
                }
            }
            .allowsHitTesting(true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.escape) {
            onDismiss()
            return .handled
        }
        .onKeyPress(.leftArrow) {
            goPrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            goNext()
            return .handled
        }
        .onAppear { focused = true }
        .onChange(of: cursor.current.image) { _, _ in
            magnification = 1.0
            fullResImageUpgradeTask?.cancel()
            fullResImageUpgradeTask = nil
        }
    }

    private func loadCurrentImage(containerSize: CGSize) async {
        loadedImage = nil
        fullResImage = nil
        imageAnalysis = nil
        loadFailed = false
        decodeSource = nil

        guard let source = await resolveDecodeSource() else {
            return
        }
        decodeSource = source

        let loaded: NSImage
        do {
            loaded = try await ImageLoader.load(
                source: source,
                size: .fitTo(
                    points: max(containerSize.width, containerSize.height)
                ),
                displayScale: displayScale
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "Failed to decode lightbox image \(cursor.current.image.description): \(error)"
            )
            loadFailed = true
            return
        }

        loadedImage = loaded

        let configuration = ImageAnalyzer.Configuration([.text])
        do {
            let analysis = try await analyzer.analyze(
                loaded,
                orientation: .up,
                configuration: configuration
            )
            if !Task.isCancelled {
                imageAnalysis = analysis
            }
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.debug("Live Text analysis failed: \(error)")
        }
    }

    private func loadFullResImage() async {
        defer { fullResImageUpgradeTask = nil }
        guard let source = decodeSource else {
            return
        }
        let loaded: NSImage
        do {
            loaded = try await ImageLoader.load(
                source: source,
                size: .native,
                displayScale: displayScale
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning("Failed to load full-res image: \(error)")
            return
        }
        guard !Task.isCancelled else {
            return
        }
        fullResImage = loaded
    }

    @ViewBuilder
    private var imageContent: some View {
        if let nsImage = fullResImage ?? loadedImage {
            loadedImageView(nsImage)
        }
        else if loadFailed {
            loadFailedView
        }
        else {
            ProgressView()
                .controlSize(.large)
        }
    }

    private func loadedImageView(_ nsImage: NSImage) -> some View {
        Image(nsImage: nsImage)
            .resizable()
            .scaledToFit()
            .overlay {
                if let analysis = imageAnalysis {
                    LiveTextOverlay(analysis: analysis)
                }
            }
            .scaleEffect(magnification, anchor: magnifyAnchor)
            .gesture(
                MagnifyGesture()
                    .onChanged { value in
                        magnifyAnchor = value.startAnchor
                        magnification = max(value.magnification, 1.0)
                        if value.magnification > 1.01,
                            fullResImage == nil,
                            fullResImageUpgradeTask == nil
                        {
                            fullResImageUpgradeTask = Task {
                                await loadFullResImage()
                            }
                        }
                    }
                    .onEnded { _ in
                        withAnimation(.easeOut(duration: 0.25)) {
                            magnification = 1.0
                        }
                    },
            )
            .padding(.horizontal, ThemeSpace.page)
            .padding(.top, ThemeSpace.page)
            .padding(.bottom, ThemeSpace.edge)
            .shadow(color: Theme.shadow, radius: 20)
    }

}

// MARK: - Overlay chrome

extension LightboxView {
    fileprivate func goPrevious() {
        var next = cursor
        next.goToPrevious()
        onUpdate(next)
    }

    fileprivate func goNext() {
        var next = cursor
        next.goToNext()
        onUpdate(next)
    }

    fileprivate var labelView: some View {
        VStack(spacing: ThemeSpace.line) {
            Text(verbatim: cursor.current.label)
                .themeText(.body)
                .foregroundStyle(Theme.onFill)
                .lineLimit(2)
            Text(verbatim: cursor.current.sourceLabel)
                .themeText(.detail)
                .foregroundStyle(Theme.onFillSecondary)
        }
        .multilineTextAlignment(.center)
        .padding(.horizontal, ThemeSpace.section)
        .padding(.bottom, ThemeSpace.related)
        .fadesWhenZoomed(at: magnification)
    }

    fileprivate var cycleNavButtons: some View {
        HStack {
            circleIconButton(
                systemName: "chevron.left",
                diameter: navButtonDiameter,
                icon: .large,
                action: goPrevious
            )
            Spacer()
            circleIconButton(
                systemName: "chevron.right",
                diameter: navButtonDiameter,
                icon: .large,
                action: goNext
            )
        }
        .padding(.horizontal, ThemeSpace.edge)
        .fadesWhenZoomed(at: magnification)
    }

    fileprivate var closeButtonOverlay: some View {
        VStack {
            HStack {
                if let onBrowseAll {
                    Button(action: onBrowseAll) {
                        Label(
                            "Browse all images",
                            systemImage: "square.grid.2x2"
                        )
                    }
                    .buttonStyle(.borderless)
                    .foregroundStyle(Theme.onFill)
                    .padding(ThemeSpace.related)
                    .background(Theme.scrim, in: Capsule())
                    .padding(ThemeSpace.group)
                }
                Spacer()
                circleIconButton(
                    systemName: "xmark",
                    diameter: 36,
                    icon: .medium
                ) {
                    onDismiss()
                }
                .padding(ThemeSpace.group)
            }
            Spacer()
        }
        .fadesWhenZoomed(at: magnification)
    }

    fileprivate func circleIconButton(
        systemName: String,
        diameter: CGFloat,
        icon: ThemeIcon,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            ZStack {
                Circle()
                    .fill(Theme.scrim)
                    .frame(width: diameter, height: diameter)
                Image(systemName: systemName)
                    .themeIcon(icon)
                    .foregroundStyle(Theme.onFillSecondary)
            }
        }
        .buttonStyle(.plain)
    }

}

extension LightboxView {
    /// Where the current item's bytes come from, or nil after a failure (logged
    /// and shown) or a cancellation (not shown).
    private func resolveDecodeSource() async -> ImageLoader.Source? {
        do {
            guard
                let source = try await imageStore.decodeSource(
                    for: cursor.current.image,
                    at: .nativeResolution
                )
            else {
                logger.warning(
                    "No bytes for lightbox image \(cursor.current.image.description)"
                )
                loadFailed = true
                return nil
            }
            return source
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to fetch lightbox image \(cursor.current.image.description): \(error)"
            )
            loadFailed = true
            return nil
        }
    }

    /// The failed-load state, with a retry.
    fileprivate var loadFailedView: some View {
        VStack(spacing: ThemeSpace.related) {
            Image(systemName: "exclamationmark.triangle")
                .themeIcon(.hero)
                .foregroundStyle(Theme.onFillSecondary)
            Text("Couldn't load image")
                .themeText(.body)
                .foregroundStyle(Theme.onFillSecondary)
            Button("Try again") { attempt += 1 }
        }
    }
}

extension View {
    fileprivate func fadesWhenZoomed(at magnification: CGFloat) -> some View {
        opacity(magnification > 1.01 ? 0 : 1)
    }
}

private struct LiveTextOverlay: NSViewRepresentable {
    let analysis: ImageAnalysis

    func makeNSView(context _: Context) -> ImageAnalysisOverlayView {
        let overlayView = ImageAnalysisOverlayView()
        overlayView.preferredInteractionTypes = .textSelection
        overlayView.analysis = analysis
        return overlayView
    }

    func updateNSView(
        _ overlayView: ImageAnalysisOverlayView,
        context _: Context
    ) {
        overlayView.analysis = analysis
    }
}

#if DEBUG
    #Preview {
        if let cursor = Cursor(items: [
            LightboxItem(
                label: "Front.jpg",
                file: PreviewData.previewArtFile("Front")
            ),
            LightboxItem(
                label: "Back.jpg",
                file: PreviewData.previewArtFile("Back")
            ),
        ]) {
            LightboxView(cursor: cursor, onUpdate: { _ in }, onDismiss: {})
                .environment(ImageStore.stub())
        }
    }
#endif

// periphery:ignore - a `.task(id:)` identity: compared, never read.
/// One load of the lightbox's image: which image, and which time of asking.
private struct LightboxLoad: Equatable {
    let image: ImageContent
    let attempt: Int
}
