import Testing

@testable import bae

@Suite("MediaPaths.fileSystemPath")
struct MediaPathsFileSystemPathTests {

    @Test("strips the cache-busting version suffix")
    func stripsVersionSuffix() {
        #expect(
            MediaPaths.fileSystemPath(of: "/covers/ab/cd/release-1#v=1700000000")
                == "/covers/ab/cd/release-1")
    }

    @Test("returns a bare path unchanged when there's no version")
    func barePathUnchanged() {
        #expect(
            MediaPaths.fileSystemPath(of: "/covers/ab/cd/release-1")
                == "/covers/ab/cd/release-1")
    }
}
