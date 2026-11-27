import AppKit
import Foundation

/// The picture's tiles, which a tiled layer draws on its own threads as they
/// come on screen at the level of detail the zoom needs.
final class TiledPictureView: NSView {
    private let drawer = TileDrawer()
    private let tiled = CATiledLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        tiled.delegate = drawer
        tiled.tileSize = CGSize(width: 512, height: 512)
        drawLevelsOfDetail(
            above: Int(ceil(log2(ZoomedPictureView.zoomBeforeLevels)))
        )
        layer = tiled
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard let window else { return }
        tiled.contentsScale = window.backingScaleFactor
    }

    var source: PictureTileSource? {
        get { drawer.state.source }
        set { drawer.update { $0.source = newValue } }
    }

    var grid: PictureTileGrid? {
        get { drawer.state.grid }
        set {
            drawer.update { $0.grid = newValue }
            tiled.setNeedsDisplay()
        }
    }

    /// Draw `levels` levels of detail above the stage's, and the stage's.
    func drawLevelsOfDetail(above levels: Int) {
        tiled.levelsOfDetail = levels + 1
        tiled.levelsOfDetailBias = levels
    }

    func tileArrived(_ key: PictureTileKey) {
        guard let grid else { return }
        tiled.setNeedsDisplay(
            drawer.layerRect(grid.rect(of: key), height: tiled.bounds.height)
        )
    }
}

/// Draws the tiled layer's tiles, on the layer's threads.
private final class TileDrawer: NSObject, CALayerDelegate, @unchecked Sendable {
    struct State {
        var source: PictureTileSource?
        var grid: PictureTileGrid?
        /// Whether the layer's drawing space runs down from the top, as the
        /// last draw found it.
        var drawsDown = false
    }

    private let lock = NSLock()
    private var current = State()

    var state: State { lock.withLock { current } }

    func update(_ change: (inout State) -> Void) {
        lock.withLock { change(&current) }
    }

    /// `rect`, in the picture's points from the top left, in the layer's
    /// own space.
    func layerRect(_ rect: CGRect, height: CGFloat) -> CGRect {
        if state.drawsDown { return rect }
        return CGRect(
            x: rect.minX,
            y: height - rect.maxY,
            width: rect.width,
            height: rect.height
        )
    }

    func draw(_ layer: CALayer, in context: CGContext) {
        let drawsDown = context.ctm.d < 0
        update { $0.drawsDown = drawsDown }
        let state = state
        guard let source = state.source, let grid = state.grid else { return }
        if !drawsDown {
            context.translateBy(x: 0, y: layer.bounds.height)
            context.scaleBy(x: 1, y: -1)
        }
        source.draw(
            grid,
            in: context,
            rect: context.boundingBoxOfClipPath,
            scale: abs(context.ctm.a),
            requesting: true
        )
    }
}
