import Testing

@testable import bae

@Suite("CloudKitService")
struct CloudKitServiceTests {
    @Test("constructing the driver does not access CloudKit")
    func constructionDoesNotAccessCloudKit() {
        let service = CloudKitService.bae()

        withExtendedLifetime(service) {}
    }
}
