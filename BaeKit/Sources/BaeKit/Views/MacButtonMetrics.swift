import SwiftUI

/// The box a push button has at a control size, which the primary,
/// secondary and borderless styles share: 20 points high with 8 either side
/// of its words when small, 24 and 12 when regular, 28 and 14 when large.
struct MacButtonMetrics {
    let controlSize: ControlSize

    var height: CGFloat {
        switch controlSize {
        case .mini, .small: 20
        case .large, .extraLarge: 28
        default: 24
        }
    }

    var padding: CGFloat {
        switch controlSize {
        case .mini, .small: ThemeSpace.related
        case .large, .extraLarge: 14
        default: ThemeSpace.group
        }
    }

    /// A capsule when large, rounded by the design's bar radius when small and its chip radius otherwise.
    var shape: AnyShape {
        switch controlSize {
        case .large, .extraLarge: AnyShape(Capsule())
        case .mini, .small:
            AnyShape(RoundedRectangle(cornerRadius: ThemeRadius.bar))
        default: AnyShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
        }
    }
}
