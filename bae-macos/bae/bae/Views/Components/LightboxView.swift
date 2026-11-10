import AppKit
import BaeKit
import SwiftUI
import VisionKit

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

    init(id: String, label: String, image: ImageContent) {
        self.id = id
        self.label = label
        self.image = image
    }

    var sourceLabel: String { String(localized: "Release files") }
}

struct LightboxView<Item: LightboxImage>: View {
    let cursor: Cursor<Item>
    /// The current item's image as it loads, owned by whoever shows the
    /// lightbox.
    let load: LightboxImageLoad
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
    /// How many times the person retried a failed load; part of the load's
    /// identity, so a retry restarts it.
    @State
    private var attempt = 0
    @FocusState
    private var focused: Bool

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
                        id: LightboxLoadRequest(
                            image: cursor.current.image,
                            attempt: attempt
                        )
                    ) {
                        await load.load(
                            cursor.current.image,
                            from: imageStore,
                            fitting: max(geo.size.width, geo.size.height),
                            displayScale: displayScale
                        )
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
        .onDisappear { load.close() }
        .onChange(of: cursor.current.image) { _, _ in
            magnification = 1.0
        }
    }

    @ViewBuilder
    private var imageContent: some View {
        if let nsImage = load.fullResolution ?? load.phase.image {
            loadedImageView(nsImage)
        }
        else if case .failed = load.phase {
            loadFailedView
        }
        else {
            ProgressView {
                Text(
                    "Loading the image…",
                    comment:
                        "Lightbox, while bae reads the image shown large, which can come from the library's storage."
                )
                .themeText(.body)
                .foregroundStyle(Theme.onFillSecondary)
            }
            .controlSize(.large)
        }
    }

    private func loadedImageView(_ nsImage: NSImage) -> some View {
        Image(nsImage: nsImage)
            .resizable()
            .scaledToFit()
            .overlay {
                if let analysis = load.phase.analysis {
                    LiveTextOverlay(analysis: analysis)
                }
            }
            .scaleEffect(magnification, anchor: magnifyAnchor)
            .gesture(
                MagnifyGesture()
                    .onChanged { value in
                        magnifyAnchor = value.startAnchor
                        magnification = max(value.magnification, 1.0)
                        if value.magnification > 1.01 {
                            load.loadFullResolution(displayScale: displayScale)
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
    /// The failed-load state, with a retry. The load keeps no error, so the
    /// notice has no Details.
    fileprivate var loadFailedView: some View {
        Notice(
            error: DisplayError(
                line: String(localized: "Couldn’t load the image")
            ),
            look: .centeredOnStage,
            action: ErrorAction { attempt += 1 }
        )
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
                id: "Front.jpg",
                label: "Front.jpg",
                image: .localFile(PreviewData.previewArtFile("Front"))
            ),
            LightboxItem(
                id: "Back.jpg",
                label: "Back.jpg",
                image: .localFile(PreviewData.previewArtFile("Back"))
            ),
        ]) {
            LightboxView(
                cursor: cursor,
                load: LightboxImageLoad(),
                onUpdate: { _ in },
                onDismiss: {}
            )
            .environment(ImageStore.stub())
        }
    }

    #Preview("Loading") {
        LightboxView<LightboxItem>.loadingPreview()
    }

    extension LightboxView where Item == LightboxItem {
        /// A remote image whose bytes never arrive, which keeps the lightbox
        /// on its load.
        static func loadingPreview() -> some View {
            let image = BridgeRemoteImageSet(
                url: "https://images.example/front-original.png",
                downscaled: []
            )
            let item = LightboxItem(
                id: "front",
                label: "Front",
                image: .remote(image)
            )
            let images = ImageStore(fetchRemoteImage: { _, _ in
                try await Task.neverAnswers()
            })
            return Cursor(items: [item], preferring: "front")
                .map { cursor in
                    LightboxView(
                        cursor: cursor,
                        load: LightboxImageLoad(),
                        onUpdate: { _ in },
                        onDismiss: {}
                    )
                    .environment(images)
                }
        }
    }
#endif

// periphery:ignore - a `.task(id:)` identity: compared, never read.
/// One load of the lightbox's image: which image, and which time of asking.
private struct LightboxLoadRequest: Equatable {
    let image: ImageContent
    let attempt: Int
}
