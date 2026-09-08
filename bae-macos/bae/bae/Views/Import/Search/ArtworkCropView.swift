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
    private var cropped: NSImage?

    /// What this view is showing a crop of.
    private var crop: Crop {
        Crop(content: content, region: region)
    }

    var body: some View {
        ZStack {
            Rectangle().fill(Theme.placeholder)
            if let cropped {
                Image(nsImage: cropped)
                    .resizable()
                    .aspectRatio(contentMode: height == nil ? .fit : .fill)
            }
        }
        .frame(width: width, height: height ?? fittedHeight)
        .clipShape(RoundedRectangle(cornerRadius: height == nil ? 4 : 2))
        .task(id: crop) {
            await load(crop)
        }
    }

    /// The height the crop's own proportions give it at `width`, once it is
    /// decoded; a landscape placeholder until then.
    private var fittedHeight: CGFloat {
        guard let cropped, cropped.size.width > 0 else { return width * 0.75 }
        return width * cropped.size.height / cropped.size.width
    }

    private func load(_ crop: Crop) async {
        do {
            guard
                let image = try await crop.cut(
                    with: imageStore,
                    displayScale: displayScale
                )
            else {
                logger.warning("no image to crop at \(content.description)")
                cropped = nil
                return
            }
            cropped = image
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "failed to decode \(content.description) for a crop: \(error.localizedDescription)"
            )
            cropped = nil
        }
    }
}

/// What a crop is of: the image and the region of it. Equatable so the task
/// that cuts it re-runs when either changes, and it cuts its own image
/// because these two values are the whole of what that takes.
private struct Crop: Equatable {
    let content: ImageContent
    let region: BridgeImageRegion?

    /// The crop is cut out of a decode large enough that a small region of a
    /// full scan still has pixels in it at the sizes it is shown.
    private static let decodePointSize: CGFloat = 1024

    /// The region as its own image, or `nil` when nothing decodes.
    @MainActor
    func cut(
        with imageStore: ImageStore,
        displayScale: CGFloat
    ) async throws -> NSImage? {
        guard
            let image = try await imageStore.image(
                content,
                pointSize: Self.decodePointSize,
                displayScale: displayScale
            )
        else { return nil }
        return cut(image)
    }

    /// The region of `image`, as its own image. The region names fractions
    /// of the image from its top-left corner, which is also where a bitmap's
    /// pixel space starts.
    private func cut(_ image: NSImage) -> NSImage {
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
