import AppKit
import BaeKit
import OSLog
import SwiftUI
import VisionKit

private let logger = Logger.bae("MacZoomablePicture")

/// The lightbox's picture, which the person pinches to zoom and scrolls to
/// pan. It shows the copy decoded to fit the stage, and once zoomed past it
/// draws the picture from the tiles core cuts, asking for those on screen
/// at the level the zoom needs. Live Text reads what is on screen: the
/// fitted copy's text at rest, and the visible tiles' once zoomed.
struct MacZoomablePicture: NSViewRepresentable {
    /// The picture as core names it for its tiles; nil for one that no
    /// viewer zooms.
    let picture: BridgeCoverImageSource?
    /// The copy decoded to fit the stage.
    let fit: DecodedImage
    /// Live Text's reading of the fitted copy, once there is one.
    let fitAnalysis: ImageAnalysis?
    let images: ImageStore
    let textReader: LiveTextReader
    /// Core could not give the picture's tiles, which the lightbox shows.
    let onFailure: (_ failure: LightboxFailure) -> Void
    /// A click beside the picture, which closes the lightbox.
    let onClickBeside: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(textReader: textReader)
    }

    func makeNSView(context: Context) -> ZoomScrollView {
        let view = ZoomScrollView(fit: fit)
        context.coordinator.attach(
            to: view,
            picture: picture,
            images: images,
            onFailure: onFailure
        )
        update(view, coordinator: context.coordinator)
        return view
    }

    func updateNSView(_ view: ZoomScrollView, context: Context) {
        update(view, coordinator: context.coordinator)
    }

    static func dismantleNSView(_: ZoomScrollView, coordinator: Coordinator) {
        coordinator.close()
    }

    private func update(_ view: ZoomScrollView, coordinator: Coordinator) {
        coordinator.fitAnalysis = fitAnalysis
        view.clickedBeside = onClickBeside
        coordinator.showText()
    }

    @MainActor
    final class Coordinator {
        var fitAnalysis: ImageAnalysis?

        private let textReader: LiveTextReader
        private weak var view: ZoomScrollView?
        private var source: PictureTileSource?
        private var onFailure: (_ failure: LightboxFailure) -> Void = { _ in }
        private var observers: [NSObjectProtocol] = []
        /// The wait for a pan or zoom to stop, and the read of the screen.
        private var settling: Task<Void, Never>?
        private var reading: Task<Void, Never>?
        /// Whether the pan or zoom has stopped, so what is on screen is what
        /// the tiles that arrive complete.
        private var settled = true

        init(textReader: LiveTextReader) {
            self.textReader = textReader
        }

        func attach(
            to view: ZoomScrollView,
            picture: BridgeCoverImageSource?,
            images: ImageStore,
            onFailure: @escaping (_ failure: LightboxFailure) -> Void
        ) {
            self.view = view
            self.onFailure = onFailure
            guard let picture else {
                view.allowsMagnification = false
                return
            }
            let source = PictureTileSource(
                picture: picture,
                images: images,
                levelsArrived: { [weak self] tiles in
                    self?.view?.levelsArrived(tiles)
                    self?.moved()
                },
                tileArrived: { [weak self] key in
                    self?.view?.tileArrived(key)
                    self?.readScreenWhenComplete()
                },
                failed: { [weak self] failure in
                    self?.onFailure(failure)
                }
            )
            self.source = source
            view.picture.tiles.source = source
            let center = NotificationCenter.default
            observers = [
                center.addObserver(
                    forName: NSScrollView.willStartLiveMagnifyNotification,
                    object: view,
                    queue: .main
                ) { _ in
                    source.readLevels()
                },
                center.addObserver(
                    forName: NSView.boundsDidChangeNotification,
                    object: view.contentView,
                    queue: .main
                ) { [weak self] _ in
                    MainActor.assumeIsolated { self?.moved() }
                },
            ]
            view.contentView.postsBoundsChangedNotifications = true
        }

        func close() {
            for observer in observers {
                NotificationCenter.default.removeObserver(observer)
            }
            observers = []
            settling?.cancel()
            reading?.cancel()
            source?.close()
        }

        /// The zoom or the pan changed: keep only the tiles now on screen,
        /// and read the screen's text once the movement stops.
        private func moved() {
            guard let view else { return }
            if view.isZoomed {
                source?.readLevels()
            }
            if let wanted = view.tilesOnScreen() {
                source?.keepOnly(wanted)
            }
            reading?.cancel()
            settling?.cancel()
            guard view.isZoomed else {
                settled = true
                showText()
                return
            }
            settled = false
            // A pan or zoom is input that moves again within moments: its
            // end is the first pause, and the tiles that complete the
            // screen are waited on, not guessed at.
            settling = Task { [weak self] in
                try? await Task.sleep(for: .milliseconds(300))
                guard !Task.isCancelled else { return }
                self?.settled = true
                self?.readScreenWhenComplete()
            }
        }

        /// Show the text Live Text read: the fitted copy's at rest; once
        /// zoomed, the last reading of the screen, which the next one
        /// replaces.
        func showText() {
            guard let view, !view.isZoomed else { return }
            view.picture.showText(fitAnalysis, over: view.picture.bounds)
        }

        /// Read the screen's text once the movement stopped and every tile
        /// it wants is here.
        private func readScreenWhenComplete() {
            guard settled, let view, view.isZoomed,
                let source, !source.isReading
            else { return }
            reading?.cancel()
            reading = Task { [weak self] in
                await self?.readScreen()
            }
        }

        /// Read the text of what is on screen at the current zoom, composed
        /// from the fitted copy and the tiles here.
        private func readScreen() async {
            guard let view, view.isZoomed,
                let (area, screen) = view.composeScreen()
            else { return }
            let analysis = await textReader.read(screen)
            guard !Task.isCancelled, view.isZoomed else { return }
            view.picture.showText(analysis, over: area)
        }
    }
}

/// The scroll view that zooms the picture: it keeps the picture centred
/// while it is smaller than the stage and fits it to the stage at rest.
final class ZoomScrollView: NSScrollView {
    let picture: ZoomedPictureView
    var clickedBeside: () -> Void = {}
    private let aspect: CGSize

    init(fit: DecodedImage) {
        picture = ZoomedPictureView(fit: fit)
        aspect = fit.image.size
        super.init(frame: .zero)
        contentView = CenteringClipView()
        drawsBackground = false
        hasHorizontalScroller = false
        hasVerticalScroller = false
        allowsMagnification = true
        minMagnification = 1
        maxMagnification = ZoomedPictureView.zoomBeforeLevels
        usesPredominantAxisScrolling = false
        documentView = picture
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    /// Whether the picture is zoomed past the stage.
    var isZoomed: Bool { magnification > 1.01 }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        knowBackingScale()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        knowBackingScale()
    }

    /// The picture's zoom limit follows the display it is on, which is known
    /// once the view is in a window and changes when it moves to another.
    private func knowBackingScale() {
        picture.backingScale = window?.backingScaleFactor
        maxMagnification = picture.maxZoom
    }

    override func layout() {
        super.layout()
        let stage = bounds.size
        guard aspect.width > 0, aspect.height > 0, stage.width > 0,
            stage.height > 0
        else { return }
        let scale = min(
            stage.width / aspect.width,
            stage.height / aspect.height
        )
        let size = CGSize(
            width: (aspect.width * scale).rounded(),
            height: (aspect.height * scale).rounded()
        )
        if picture.frame.size != size {
            picture.sized(to: size)
            maxMagnification = picture.maxZoom
        }
    }

    override func mouseUp(with event: NSEvent) {
        let point = picture.convert(event.locationInWindow, from: nil)
        if !picture.bounds.contains(point) {
            clickedBeside()
        }
        else {
            super.mouseUp(with: event)
        }
    }

    func levelsArrived(_ tiles: BridgePictureTiles) {
        picture.levelsArrived(tiles)
        maxMagnification = picture.maxZoom
    }

    func tileArrived(_ key: PictureTileKey) {
        picture.tiles.tileArrived(key)
    }

    /// The tiles on screen at the level the zoom draws, and those a tile
    /// beyond each side; nil before the levels arrive or at rest.
    func tilesOnScreen() -> Set<PictureTileKey>? {
        guard isZoomed, let grid = picture.grid,
            let backing = picture.backingScale
        else { return nil }
        let edge =
            picture.bounds.width / CGFloat(grid.tiles.levels[0].width)
            * CGFloat(grid.tiles.tileEdge)
        let visible = documentVisibleRect.insetBy(dx: -edge, dy: -edge)
        // The pixels per point the tiled layer draws at this zoom: the power
        // of two at or above it, as the layer steps between its levels of
        // detail.
        let drawn = backing * pow(2, ceil(log2(max(magnification, 1))))
        return Set(grid.keys(level: grid.level(forScale: drawn), in: visible))
    }

    /// What is on screen at the current zoom, as one image Live Text reads,
    /// and the area of the picture it shows; nil while nothing of the picture
    /// is on screen.
    func composeScreen() -> (CGRect, CGImage)? {
        let area = documentVisibleRect.intersection(picture.bounds)
        guard let backing = picture.backingScale, !area.isNull, !area.isEmpty
        else { return nil }
        let scale = min(
            magnification * backing,
            4096 / max(area.width, area.height)
        )
        guard let image = picture.compose(area, scale: scale) else {
            return nil
        }
        return (area, image)
    }
}

/// Keeps the picture centred while it is smaller than the stage.
private final class CenteringClipView: NSClipView {
    override func constrainBoundsRect(_ proposedBounds: NSRect) -> NSRect {
        var bounds = super.constrainBoundsRect(proposedBounds)
        guard let document = documentView else { return bounds }
        let size = document.frame.size
        if bounds.width > size.width {
            bounds.origin.x = (size.width - bounds.width) / 2
        }
        if bounds.height > size.height {
            bounds.origin.y = (size.height - bounds.height) / 2
        }
        return bounds
    }
}

/// The picture the scroll view zooms: the fitted copy, the tiles over it,
/// and Live Text's overlay over what it read.
final class ZoomedPictureView: NSView {
    /// How far a zoom goes before the levels arrive and say how far the
    /// picture's own pixels reach.
    static let zoomBeforeLevels: CGFloat = 8

    let tiles = TiledPictureView()
    private let copy = NSImageView()
    private let overlay = ImageAnalysisOverlayView()
    /// The fitted copy's bitmap, which Live Text reads under the tiles.
    private let fitBitmap: CGImage
    private(set) var grid: PictureTileGrid?
    private(set) var maxZoom = zoomBeforeLevels
    /// The pixels the display this view is on draws for a point; nil until
    /// the view is in a window.
    var backingScale: CGFloat? {
        didSet { updateZoomLimit() }
    }

    init(fit: DecodedImage) {
        fitBitmap = fit.bitmap
        super.init(frame: .zero)
        copy.image = fit.image
        copy.imageScaling = .scaleAxesIndependently
        copy.autoresizingMask = [.width, .height]
        tiles.autoresizingMask = [.width, .height]
        overlay.preferredInteractionTypes = .textSelection
        addSubview(copy)
        addSubview(tiles)
        addSubview(overlay)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    override var isFlipped: Bool { true }

    /// The view took its size for the stage.
    func sized(to size: CGSize) {
        setFrameSize(size)
        copy.frame = bounds
        tiles.frame = bounds
        if let levels = grid?.tiles {
            levelsArrived(levels)
        }
    }

    func levelsArrived(_ levels: BridgePictureTiles) {
        let grid = PictureTileGrid(
            tiles: levels,
            size: bounds.size,
            copyPixels: fitBitmap.width
        )
        self.grid = grid
        tiles.grid = grid
        updateZoomLimit()
    }

    /// Zoom until a pixel of the picture covers two of the screen's, once
    /// both the picture's levels and the display are known.
    private func updateZoomLimit() {
        guard let grid, let backingScale, bounds.width > 0 else { return }
        let reach =
            CGFloat(grid.tiles.levels[0].width) / (bounds.width * backingScale)
        maxZoom = max(2, reach * 2)
        tiles.drawLevelsOfDetail(above: Int(ceil(log2(maxZoom))))
    }

    /// Show Live Text's reading of the picture's `area`.
    func showText(_ analysis: ImageAnalysis?, over area: CGRect) {
        overlay.frame = area
        overlay.analysis = analysis
    }

    /// `area` of the picture as one image `scale` pixels per point: the
    /// fitted copy, with the tiles here drawn over it. Nil when the bitmap
    /// to draw them into cannot be made, which is logged.
    func compose(_ area: CGRect, scale: CGFloat) -> CGImage? {
        let width = Int((area.width * scale).rounded())
        let height = Int((area.height * scale).rounded())
        guard width > 0, height > 0 else { return nil }
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
            let context = CGContext(
                data: nil,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: 0,
                space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            )
        else {
            logger.error(
                "No \(width) by \(height) bitmap to compose the zoomed picture into for Live Text"
            )
            return nil
        }
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: scale, y: -scale)
        context.translateBy(x: -area.minX, y: -area.minY)
        drawUpright(fitBitmap, in: bounds, context: context)
        if let grid, let source = tiles.source {
            source.draw(
                grid,
                in: context,
                rect: area,
                scale: scale,
                requesting: false
            )
        }
        return context.makeImage()
    }
}
