import BaeKit
import SwiftUI

/// The lightbox over a candidate's folder images. The lightbox keeps only the
/// file id of the image the person moved to; core's live query says which
/// image the lightbox shows and which images come before and after it.
@MainActor
@Observable
final class FolderImagesLightbox {
    /// The images and the shown image, as core's read last returned them.
    private(set) var images: BridgeFolderImages?
    private let candidateKey: Data
    /// The file id of the image the person moved to last.
    private var focusedImage: String
    private let query: FolderImagesQuery
    /// Where a failed read or request lands.
    private let onError: (DisplayError) -> Void
    private var values:
        ScreenSubscription<PulledValues<BridgeFolderImagesSnapshot>>?

    /// Follow the images of the candidate at `candidateKey`, showing the
    /// image whose file id is `focusedImage`.
    init(
        candidateKey: Data,
        focusedImage: String,
        query: FolderImagesQuery,
        onError: @escaping (DisplayError) -> Void
    ) {
        self.candidateKey = candidateKey
        self.focusedImage = focusedImage
        self.query = query
        self.onError = onError
        values = ScreenSubscription(
            query.values,
            onValue: { [weak self] in self?.receive($0) },
            onError: { [weak self] error in
                if let failure = DisplayError(error) { self?.onError(failure) }
            }
        )
        show()
    }

    /// Where the lightbox stands, once core's read has answered.
    var place: LightboxPlace<LightboxItem>? {
        guard let images, let focus = images.focus else { return nil }
        return LightboxPlace(
            items: images.images.map(LightboxItem.init(image:)),
            current: LightboxItem(image: focus.image),
            neighbors: focus.neighbors.map {
                LightboxNeighbors(previous: $0.previous, next: $0.next)
            }
        )
    }

    /// Show the image whose file id is `fileId`.
    func focus(_ fileId: String) {
        guard fileId != focusedImage else { return }
        focusedImage = fileId
        show()
    }

    func close() {
        values?.stop()
        values = nil
    }

    private var request: BridgeFolderImagesRequest {
        BridgeFolderImagesRequest(
            candidateKey: candidateKey,
            focus: focusedImage
        )
    }

    private func show() {
        do {
            try query.setRequest(request)
        }
        catch {
            if let failure = DisplayError(error) { onError(failure) }
        }
    }

    /// Take a value only while it answers the request shown now.
    private func receive(_ snapshot: BridgeFolderImagesSnapshot) {
        guard snapshot.request == request, let images = snapshot.images else {
            return
        }
        self.images = images
    }
}

extension LightboxItem {
    /// The lightbox's item for one of the folder's images.
    init(image: BridgeMappingImage) {
        self.init(
            id: image.fileId,
            label: image.name,
            image: .localFile(image.file)
        )
    }
}
