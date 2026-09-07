import AppKit
import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("ArtworkCropView")

/// The part of an image a value was read off, cut to the region the
/// detector drew around it: a barcode's stripes, a line of text. With no
/// region, the whole image. Decoded through the shared image store, so the
/// same scan cropped for several values is decoded once.
struct ArtworkCropView: View {
    let content: ImageContent
    let region: BridgeImageRegion?
    let width: CGFloat
    /// Fixed height, or `nil` to take the crop's own proportions at `width`.
    let height: CGFloat?

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var crop: NSImage?

    /// The crop is cut out of a decode large enough that a small region of a
    /// full scan still has pixels in it at the sizes it is shown.
    private static let decodePointSize: CGFloat = 1024

    var body: some View {
        ZStack {
            Rectangle().fill(Theme.placeholder)
            if let crop {
                Image(nsImage: crop)
                    .resizable()
                    .aspectRatio(contentMode: height == nil ? .fit : .fill)
            }
        }
        .frame(width: width, height: height ?? fittedHeight)
        .clipShape(RoundedRectangle(cornerRadius: height == nil ? 4 : 2))
        .task(id: CropKey(content: content, region: region)) {
            await load()
        }
    }

    /// The height the crop's own proportions give it at `width`, once it is
    /// decoded; a landscape placeholder until then.
    private var fittedHeight: CGFloat {
        guard let crop, crop.size.width > 0 else { return width * 0.75 }
        return width * crop.size.height / crop.size.width
    }

    private func load() async {
        do {
            guard
                let image = try await imageStore.image(
                    content,
                    pointSize: Self.decodePointSize,
                    displayScale: displayScale
                )
            else {
                logger.warning("no image to crop at \(content.description)")
                crop = nil
                return
            }
            crop = Self.cut(image, to: region)
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "failed to decode \(content.description) for a crop: \(error.localizedDescription)"
            )
            crop = nil
        }
    }

    /// The region of `image`, as its own image. The region names fractions
    /// of the image from its top-left corner, which is also where a bitmap's
    /// pixel space starts.
    static func cut(_ image: NSImage, to region: BridgeImageRegion?) -> NSImage
    {
        guard let region,
            let bitmap = image.cgImage(
                forProposedRect: nil,
                context: nil,
                hints: nil
            )
        else {
            return image
        }
        let pixelWidth = CGFloat(bitmap.width)
        let pixelHeight = CGFloat(bitmap.height)
        let rect = CGRect(
            x: CGFloat(region.x) * pixelWidth,
            y: CGFloat(region.y) * pixelHeight,
            width: CGFloat(region.width) * pixelWidth,
            height: CGFloat(region.height) * pixelHeight
        )
        .integral
        guard let cropped = bitmap.cropping(to: rect) else {
            logger.warning(
                "region \(rect.debugDescription) is outside its image"
            )
            return image
        }
        return NSImage(
            cgImage: cropped,
            size: NSSize(width: cropped.width, height: cropped.height)
        )
    }
}

/// What a crop is of: the image and the region, so the task that cuts it
/// re-runs when either changes.
private struct CropKey: Equatable {
    let content: ImageContent
    let region: BridgeImageRegion?
}
