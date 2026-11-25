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

/// The images before and after the one a lightbox shows, wrapping around the
/// ends.
struct LightboxNeighbors<ID: Hashable>: Equatable {
    let previous: ID
    let next: ID
}

/// Where a lightbox stands among its images: the images its strip shows, the
/// one it shows, and the ones before and after it, which a lone image lacks.
/// Core works out the place, and whoever shows the lightbox passes it in.
struct LightboxPlace<Item: LightboxImage>: Equatable {
    let items: [Item]
    let current: Item
    let neighbors: LightboxNeighbors<Item.ID>?

    /// Whether the lightbox moves between images.
    var canCycle: Bool { neighbors != nil }
}

/// The close button's circle, which the top row of the stage is as tall as,
/// with the space round it.
private let lightboxCloseButtonDiameter: CGFloat = 36
private let lightboxStageTopInset =
    lightboxCloseButtonDiameter + 2 * ThemeSpace.group

struct MacLightbox<Item: LightboxImage>: View {
    let place: LightboxPlace<Item>
    /// The current item's image as it loads, owned by whoever shows the
    /// lightbox.
    let load: LightboxImageLoad
    /// Show the image `id` names.
    let onSelect: (Item.ID) -> Void
    let onDismiss: () -> Void
    var onBrowseAll: (() -> Void)?

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
    /// The picture and time of asking whose tiles core could not give, which
    /// the stage shows as it shows a picture that did not load, until the
    /// person retries or moves to another picture.
    @State
    private var failedZoom: FailedZoom?
    /// How many times the person retried a failed load or zoom; part of the
    /// load's identity, so a retry restarts it.
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
                        // The stage under the row that holds the close
                        // button: the image, its messages and the arrows
                        // share its middle.
                        imageContent
                            .padding(.top, lightboxStageTopInset)
                        if place.canCycle {
                            cycleNavButtons
                        }
                        closeButtonOverlay
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .task(id: currentRequest) {
                        await load.load(
                            place.current.image,
                            from: imageStore,
                            fitting: max(geo.size.width, geo.size.height),
                            displayScale: displayScale
                        )
                    }
                }

                labelView

                if place.canCycle {
                    MacThumbnailStrip(
                        items: place.items,
                        current: place.current.id,
                        centered: true,
                        onSelect: onSelect,
                        name: \.label,
                        stroke: { _, isActive in
                            (
                                isActive
                                    ? Theme.onFill : Theme.onFillSecondary,
                                isActive ? 2 : 1
                            )
                        }
                    ) { item in
                        Artwork(
                            content: item.image,
                            pointSize: MacThumbnailStripLayout.thumbnailSize,
                            cornerRadius: nil
                        )
                    }
                    .padding(.bottom, ThemeSpace.group)
                }
            }
            .allowsHitTesting(true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        // The keyboard stays on the lightbox: Tab would move to the gallery
        // under its backdrop.
        .onKeyPress(.tab) { .handled }
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
    }

    /// The load of the current image, at this time of asking.
    private var currentRequest: LightboxLoadRequest {
        LightboxLoadRequest(image: place.current.image, attempt: attempt)
    }

    @ViewBuilder
    private var imageContent: some View {
        // The load holds the previous image's phase until the load of the
        // current one starts.
        let phase = load.phase(for: place.current.image)
        if let failed = failedZoom, failed.request == currentRequest {
            loadFailedView(failed.failure)
        }
        else if let nsImage = phase.image {
            loadedImageView(nsImage, analysis: phase.analysis)
        }
        else if case .failed(let failure) = phase {
            loadFailedView(failure)
        }
        else {
            MacSpinner(
                Text(
                    "Loading the image…",
                    comment:
                        "Lightbox, while bae reads the image shown large, which can come from the library's cloud."
                )
            )
        }
    }

    private func loadedImageView(
        _ nsImage: DecodedImage,
        analysis: ImageAnalysis?
    ) -> some View {
        MacZoomablePicture(
            picture: place.current.image.tiledSource,
            fit: nsImage,
            fitAnalysis: analysis,
            images: imageStore,
            textReader: load.textReader,
            onFailure: { [request = currentRequest] failure in
                failedZoom = FailedZoom(request: request, failure: failure)
            },
            onClickBeside: onDismiss
        )
        .background {
            // The picture's shadow, which the picture over it hides.
            Image(nsImage: nsImage.image)
                .resizable()
                .scaledToFit()
                .shadow(color: Theme.shadow, radius: 20)
        }
        .padding(.horizontal, ThemeSpace.page)
        .padding(.vertical, ThemeSpace.edge)
    }
}

// MARK: - Overlay chrome

extension MacLightbox {
    fileprivate func goPrevious() {
        if let previous = place.neighbors?.previous { onSelect(previous) }
    }

    fileprivate func goNext() {
        if let next = place.neighbors?.next { onSelect(next) }
    }

    fileprivate var labelView: some View {
        VStack(spacing: 0) {
            Text(verbatim: place.current.label)
                .themeText(.body)
                .foregroundStyle(Theme.onFill)
                .lineLimit(2)
            Text(verbatim: place.current.sourceLabel)
                .themeText(.detail)
                .foregroundStyle(Theme.onFillSecondary)
        }
        .multilineTextAlignment(.center)
        .padding(.horizontal, ThemeSpace.section)
        .padding(.vertical, ThemeSpace.related)
    }

    fileprivate var cycleNavButtons: some View {
        HStack {
            circleIconButton(
                systemName: "chevron.backward",
                diameter: navButtonDiameter,
                icon: .stageArrow,
                action: goPrevious
            )
            Spacer()
            circleIconButton(
                systemName: "chevron.forward",
                diameter: navButtonDiameter,
                icon: .stageArrow,
                action: goNext
            )
        }
        .padding(.horizontal, ThemeSpace.edge)
        // The arrows sit level with the middle of the stage under the row
        // that holds the close button.
        .padding(.top, lightboxStageTopInset)
    }

    fileprivate var closeButtonOverlay: some View {
        VStack {
            HStack {
                if let onBrowseAll {
                    Button(action: onBrowseAll) {
                        Text("Browse all images")
                            .padding(.horizontal, ThemeSpace.group)
                            // As tall as the close button beside it.
                            .frame(height: lightboxCloseButtonDiameter)
                            .background(Theme.scrim, in: Capsule())
                    }
                    .buttonStyle(.accentPlain)
                    .foregroundStyle(Theme.onFill)
                    .padding(ThemeSpace.group)
                }
                Spacer()
                circleIconButton(
                    systemName: "xmark",
                    diameter: lightboxCloseButtonDiameter,
                    icon: .stageClose
                ) {
                    onDismiss()
                }
                .padding(ThemeSpace.group)
            }
            Spacer()
        }
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
        .buttonStyle(.accentPlain)
    }
}

extension MacLightbox {
    /// The failed-load and failed-zoom state: core's error, when core said
    /// anything to show, behind Details, and a Retry when trying again can
    /// get past the failure.
    fileprivate func loadFailedView(_ failure: LightboxFailure) -> some View {
        MacNotice(
            title: String(localized: "Could not load the image"),
            advice: nil,
            error: failure.error,
            look: .centeredOnStage,
            action: failure.offersRetry ? ErrorAction { attempt += 1 } : nil
        )
    }
}

/// A zoom whose tiles core could not give, and why.
private struct FailedZoom {
    let request: LightboxLoadRequest
    let failure: LightboxFailure
}

// periphery:ignore - an identity: compared, never read.
/// One load of the lightbox's image: which image, and which time of asking.
private struct LightboxLoadRequest: Equatable {
    let image: ImageContent
    let attempt: Int
}
