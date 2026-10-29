import Testing

@testable import bae

@Suite("AlbumDragPayload")
struct AlbumDragPayloadTests {
    @Test("an album drag's ids round-trip")
    func albumIdsRoundTrip() {
        let ids = [
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        ]
        #expect(
            AlbumDragPayload.albums(in: AlbumDragPayload.encode(ids)) == ids
        )
    }

    @Test("a dropped track id is no album drag")
    func aTrackIdIsNoAlbumDrag() {
        #expect(
            AlbumDragPayload.albums(
                in: "550e8400-e29b-41d4-a716-446655440000"
            ) == nil
        )
    }
}
