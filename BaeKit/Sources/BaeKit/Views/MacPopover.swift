#if canImport(AppKit)
    import SwiftUI

    /// Sizes of a bae popover, from the design system's `MacPopover`: a
    /// face 10 points rounded, its arrow 14 wide and 7 long.
    enum MacPopoverMetrics {
        static let cornerRadius: CGFloat = 10
        static let arrowWidth: CGFloat = 14
        static let arrowLength: CGFloat = 7
        /// Room around the face, for its shadow.
        static let shadowRoom: CGFloat = 40
        /// The gap between the anchor and the arrow's tip.
        static let gap: CGFloat = 2
    }

    /// Where a popover sits and where its arrow points, which change as the
    /// content grows and the popover moves to stay on the screen.
    @MainActor
    @Observable
    final class MacPopoverModel {
        /// The side of the popover that carries the arrow.
        var arrowSide: Edge
        /// The arrow's centre, along that side from its top or left end.
        var arrowOffset: CGFloat = 0
        /// What the popover shows, drawn again when the view that opened it
        /// has new state to show.
        var content: AnyView

        init(arrowSide: Edge, content: AnyView) {
            self.arrowSide = arrowSide
            self.content = content
        }
    }

    /// The popover's outline: a rounded rectangle with an arrow on one side.
    struct MacPopoverOutline: Shape {
        let arrowSide: Edge
        let arrowOffset: CGFloat

        func path(in rect: CGRect) -> Path {
            let length = MacPopoverMetrics.arrowLength
            let half = MacPopoverMetrics.arrowWidth / 2
            var body = rect
            switch arrowSide {
            case .top:
                body.origin.y += length
                body.size.height -= length
            case .bottom: body.size.height -= length
            case .leading:
                body.origin.x += length
                body.size.width -= length
            case .trailing: body.size.width -= length
            }
            let rounded = Path(
                roundedRect: body,
                cornerRadius: MacPopoverMetrics.cornerRadius,
                style: .continuous
            )
            var arrow = Path()
            switch arrowSide {
            case .top:
                let x = rect.minX + arrowOffset
                arrow.move(to: CGPoint(x: x - half, y: body.minY + 1))
                arrow.addLine(to: CGPoint(x: x, y: rect.minY))
                arrow.addLine(to: CGPoint(x: x + half, y: body.minY + 1))
            case .bottom:
                let x = rect.minX + arrowOffset
                arrow.move(to: CGPoint(x: x - half, y: body.maxY - 1))
                arrow.addLine(to: CGPoint(x: x, y: rect.maxY))
                arrow.addLine(to: CGPoint(x: x + half, y: body.maxY - 1))
            case .leading:
                let y = rect.minY + arrowOffset
                arrow.move(to: CGPoint(x: body.minX + 1, y: y - half))
                arrow.addLine(to: CGPoint(x: rect.minX, y: y))
                arrow.addLine(to: CGPoint(x: body.minX + 1, y: y + half))
            case .trailing:
                let y = rect.minY + arrowOffset
                arrow.move(to: CGPoint(x: body.maxX - 1, y: y - half))
                arrow.addLine(to: CGPoint(x: rect.maxX, y: y))
                arrow.addLine(to: CGPoint(x: body.maxX - 1, y: y + half))
            }
            arrow.closeSubpath()
            return rounded.union(arrow)
        }
    }

    /// A popover's face: bae's raised colour with a hairline edge and an
    /// arrow, around the content.
    struct MacPopoverFace: View {
        let model: MacPopoverModel

        var body: some View {
            let outline = MacPopoverOutline(
                arrowSide: model.arrowSide,
                arrowOffset: model.arrowOffset
            )
            model.content
                .clipShape(
                    RoundedRectangle(
                        cornerRadius: MacPopoverMetrics.cornerRadius,
                        style: .continuous
                    )
                )
                .padding(arrowEdges, MacPopoverMetrics.arrowLength)
                .background(
                    outline.fill(Theme.surfaceElevated)
                        .shadow(color: Theme.shadow, radius: 15, y: 6)
                )
                .overlay(outline.stroke(Theme.hairline, lineWidth: 1))
                .padding(MacPopoverMetrics.shadowRoom)
        }

        private var arrowEdges: Edge.Set {
            switch model.arrowSide {
            case .top: .top
            case .bottom: .bottom
            case .leading: .leading
            case .trailing: .trailing
            }
        }
    }
#endif

#if canImport(AppKit)
    /// A popover's face drawn in place, for captures and previews; the app's
    /// popovers draw it in a panel.
    public struct MacPopoverPreview<Content: View>: View {
        let arrowSide: Edge
        let arrowOffset: CGFloat
        let content: Content

        public init(
            arrowSide: Edge = .top,
            arrowOffset: CGFloat = 40,
            @ViewBuilder content: () -> Content
        ) {
            self.arrowSide = arrowSide
            self.arrowOffset = arrowOffset
            self.content = content()
        }

        public var body: some View {
            let model = MacPopoverModel(
                arrowSide: arrowSide,
                content: AnyView(content)
            )
            model.arrowOffset = arrowOffset
            return MacPopoverFace(model: model)
        }
    }
#endif
