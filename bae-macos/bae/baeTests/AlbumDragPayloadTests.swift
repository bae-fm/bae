import Testing

@testable import bae

@Suite("AlbumDragPayload")
struct AlbumDragPayloadTests {
    @Test("a single id round-trips unchanged")
    func singleIdRoundTrips() {
        let id = "550e8400-e29b-41d4-a716-446655440000"
        #expect(AlbumDragPayload.encode([id]) == id)
        #expect(AlbumDragPayload.decode(id) == [id])
    }

    @Test("several ids round-trip in order")
    func multipleIdsRoundTripInOrder() {
        let ids = [
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
        ]
        let payload = AlbumDragPayload.encode(ids)
        #expect(AlbumDragPayload.decode(payload) == ids)
    }
}
