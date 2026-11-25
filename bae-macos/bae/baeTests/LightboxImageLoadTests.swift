import AppKit
import BaeKit
import Testing

@testable import bae

/// What a lightbox's load of its image ends in when the image will not load:
/// core's error, which says whether trying again can help, or no error when
/// nothing says it can.
@MainActor
struct LightboxImageLoadTests {
    private let image = ImageContent.remote(
        BridgeRemoteImageSet(
            url: "https://images.example/front.png",
            downscaled: []
        )
    )

    private func failure(of images: ImageStore) async -> LightboxFailure? {
        let load = LightboxImageLoad()
        await load.load(
            image,
            from: images,
            fitting: 400,
            displayScale: 2
        )
        guard case .failed(let failure) = load.phase else { return nil }
        return failure
    }

    @Test("A failure core reports keeps core's error and its say on a retry")
    func coreFailuresKeepTheirError() async throws {
        let images = ImageStore(fetchRemoteImage: { _, _ in
            throw BridgeError.Diagnostic(category: .internal, detail: "no")
        })

        let failure = try #require(await failure(of: images))

        #expect(failure.error?.detail != nil)
        #expect(!failure.offersRetry)
    }

    @Test("Bytes that do not decode fail with no error and no Retry")
    func undecodableBytesOfferNoRetry() async throws {
        let images = ImageStore(fetchRemoteImage: { _, _ in
            Data("not a picture".utf8)
        })

        #expect(await failure(of: images) == .undecodable)
    }

    @Test("An error that is not core's is a bug and offers no Retry")
    func otherErrorsAreBugs() async throws {
        let images = ImageStore(fetchRemoteImage: { _, _ in
            throw ImageStoreUnavailable()
        })

        #expect(await failure(of: images) == .bug)
    }
}
