package fm.bae.app.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType

/**
 * What the store promises its callers: a cached decode is pinned to the exact
 * bytes it came from, one kind's decodes can't evict another's, and a completed
 * load leaves something the next first frame can read synchronously.
 *
 * Native graphics so `BitmapFactory` really decodes and `allocationByteCount`
 * really reports what a bitmap costs — the cache budgets mean nothing against
 * shadow bitmaps.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ImageStoreTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val fitTo64 = DecodeSize.FitTo(64)

    @Test
    fun servesALoadedDecodeSynchronouslyOnTheNextLook() =
        runBlocking {
            val store = storeOf(library = { TestImages.png(128) })
            val content = ImageContent.LibraryImage(coverRef("rel-1"))

            assertNull("nothing is cached before the first load", store.cachedImage(content, fitTo64))
            val loaded = store.image(content, fitTo64).orFail("first load")
            assertSame(
                "the sync read serves the decode the load produced",
                loaded,
                store.cachedImage(content, fitTo64),
            )
        }

    @Test
    fun decodesEachSizeSeparately() =
        runBlocking {
            val store = storeOf(library = { TestImages.png(256) })
            val content = ImageContent.LibraryImage(coverRef("rel-1"))

            val small = store.image(content, DecodeSize.FitTo(32)).orFail("32px decode")
            val large = store.image(content, DecodeSize.FitTo(256)).orFail("256px decode")

            assertNotSame("one slot's decode never serves another's size", small, large)
            assertEquals(32, small.width)
            assertEquals(256, large.width)
            assertNull(
                "a size that was never decoded is still a miss",
                store.cachedImage(content, DecodeSize.FitTo(128)),
            )
        }

    @Test
    fun aBumpedVersionIsADifferentImage() =
        runBlocking {
            var served = TestImages.png(128, seed = 1)
            val store = storeOf(library = { served })

            val first = store.image(ImageContent.LibraryImage(coverRef("rel-1", "1")), fitTo64).orFail("v1")
            served = TestImages.png(128, seed = 2)
            val second = store.image(ImageContent.LibraryImage(coverRef("rel-1", "2")), fitTo64).orFail("v2")

            assertNotSame("the version moved, so the old decode can't be served", first, second)
        }

    @Test
    fun aBumpedModificationTimeIsADifferentFile() =
        runBlocking {
            val file = folder.newFile("candidate.png")
            file.writeBytes(TestImages.png(128, seed = 1))
            val store = storeOf()
            val content = ImageContent.LocalFile(file.path)

            val first = store.image(content, fitTo64).orFail("first read")
            assertSame(first, store.cachedImage(content, fitTo64))

            file.writeBytes(TestImages.png(128, seed = 2))
            file.setLastModified(file.lastModified() + 60_000)

            assertNull("the file changed, so its old decode is not served", store.cachedImage(content, fitTo64))
            assertNotSame(first, store.image(content, fitTo64).orFail("second read"))
        }

    @Test
    fun bytesInHandAreDecodedButNeverCached() =
        runBlocking {
            val store = storeOf()
            val content = ImageContent.Bytes(TestImages.png(128))

            val first = store.image(content, fitTo64).orFail("first decode")
            assertNull("bytes have no identity to pin a cache entry to", store.cachedImage(content, fitTo64))
            assertNotSame("every look decodes afresh", first, store.image(content, fitTo64).orFail("second decode"))
        }

    @Test
    fun evictionNeverCrossesBuckets() =
        runBlocking {
            // A 128px source at 64px costs 64 × 64 × 4 = 16 KB; a 256px source at
            // 128px costs 64 KB, which fills the release bucket on its own. So
            // filling that bucket over and over must leave the library bucket's one
            // entry where it is.
            val store =
                storeOf(
                    library = { TestImages.png(128) },
                    release = { TestImages.png(256) },
                    decodedBudgets =
                        DecodedImageBudgets(
                            libraryImage = 64 * 1024,
                            releaseImage = 64 * 1024,
                            remote = 64 * 1024,
                            localFile = 64 * 1024,
                        ),
                )
            val cover = ImageContent.LibraryImage(coverRef("rel-1"))
            val libraryDecode = store.image(cover, fitTo64).orFail("cover decode")

            repeat(8) { i ->
                store.image(
                    ImageContent.ReleaseImage("rel-$i", BridgeGallerySource.ReleaseFile("file-$i")),
                    DecodeSize.FitTo(128),
                )
            }

            assertSame(
                "release-image decodes evict only each other",
                libraryDecode,
                store.cachedImage(cover, fitTo64),
            )
        }

    @Test
    fun theByteCacheSpendsOneBridgeCrossingPerImage() =
        runBlocking {
            var crossings = 0
            val bytes = TestImages.png(256)
            val store =
                storeOf(library = {
                    crossings++
                    bytes
                })
            val content = ImageContent.LibraryImage(coverRef("rel-1"))

            store.image(content, DecodeSize.FitTo(32))
            store.image(content, DecodeSize.FitTo(64))
            store.image(content, DecodeSize.FitTo(128))

            assertEquals("three sizes, one read of the bytes they all decode from", 1, crossings)
        }

    @Test
    fun theByteCacheIsBoundedByItsBudget() =
        runBlocking {
            var crossings = 0
            // A 256px noise PNG is far larger than this budget, so no entry survives
            // the next put and every look reads again.
            val store =
                storeOf(
                    library = {
                        crossings++
                        TestImages.png(256)
                    },
                    byteBudgets = ImageByteBudgets(libraryImage = 4 * 1024),
                )

            store.imageBytes(ImageContent.LibraryImage(coverRef("rel-1")))
            store.imageBytes(ImageContent.LibraryImage(coverRef("rel-2")))
            store.imageBytes(ImageContent.LibraryImage(coverRef("rel-1")))

            assertEquals("an evicted entry is read again rather than served stale", 3, crossings)
        }

    @Test
    fun aReleaseFileIdNamesOneImmutableBlob() =
        runBlocking {
            var crossings = 0
            val store =
                storeOf(release = {
                    crossings++
                    TestImages.png(128)
                })
            val source = BridgeGallerySource.ReleaseFile("file-9")

            store.image(ImageContent.ReleaseImage("rel-1", source), fitTo64)
            store.image(ImageContent.ReleaseImage("rel-1", source), fitTo64)

            assertEquals(1, crossings)
        }

    @Test
    fun aCoverSlotTakesItsIdentityFromTheCover() =
        runBlocking {
            var crossings = 0
            val store =
                storeOf(release = {
                    crossings++
                    TestImages.png(128)
                })

            fun slot(version: String) = ImageContent.ReleaseImage("rel-1", BridgeGallerySource.Cover(coverRef("rel-1", version)))

            store.image(slot("1"), fitTo64)
            store.image(slot("1"), fitTo64)
            assertEquals("the cover slot's decode is held like any other", 1, crossings)

            // Replacing the release's cover moves that cover's version, and the
            // strip's cover slot is that same cover — so its entry goes too.
            assertNull(store.cachedImage(slot("2"), fitTo64))
            store.image(slot("2"), fitTo64)
            assertEquals(2, crossings)
        }

    @Test
    fun aNewValidatorReplacesEveryDecodeOfTheOldBytes() =
        runBlocking {
            var validator = "etag-1"
            val store =
                storeOf(remote = { RemoteImageBytes(bytes = TestImages.png(128), validator = validator) })
            val content = ImageContent.Remote("https://example.invalid/art.jpg")

            val small = store.image(content, DecodeSize.FitTo(32)).orFail("32px decode")
            val large = store.image(content, DecodeSize.FitTo(64)).orFail("64px decode")
            assertSame(small, store.cachedImage(content, DecodeSize.FitTo(32)))

            // Provider art is never held in the byte cache — only core's fetch
            // knows when the bytes behind a URL moved — so every read reaches the
            // fetch, and the fetch is what tells the store. It invalidates every
            // size, not just the one being loaded.
            validator = "etag-2"
            store.imageBytes(content)

            assertNull(store.cachedImage(content, DecodeSize.FitTo(32)))
            assertNull(store.cachedImage(content, DecodeSize.FitTo(64)))
            assertNotSame(large, store.image(content, DecodeSize.FitTo(64)).orFail("reloaded 64px decode"))
        }

    @Test
    fun aLibraryImageWithNoBytesResolvesToNothing() =
        runBlocking {
            val store = storeOf(library = { null })
            assertNull(store.image(ImageContent.LibraryImage(coverRef("rel-1")), fitTo64))
        }

    private fun coverRef(
        id: String,
        version: String = "1",
    ) = BridgeImageRef(id = id, version = version, imageType = BridgeLibraryImageType.COVER)

    /**
     * A store whose fetches answer from the given lambdas. A fetch the test didn't
     * supply throws, so a test that reaches one it didn't mean to fails loudly
     * instead of quietly resolving to nothing.
     */
    private fun storeOf(
        library: (() -> ByteArray?)? = null,
        release: (() -> ByteArray)? = null,
        remote: (() -> RemoteImageBytes)? = null,
        decodedBudgets: DecodedImageBudgets = DecodedImageBudgets(),
        byteBudgets: ImageByteBudgets = ImageByteBudgets(),
    ) = ImageStore(
        fetchLibraryImageBytes = { checkNotNull(library) { "no library-image fetch in this store" }() },
        fetchReleaseImageBytes = { _, _ -> checkNotNull(release) { "no release-image fetch in this store" }() },
        fetchRemoteImage = { checkNotNull(remote) { "no remote fetch in this store" }() },
        decodedBudgets = decodedBudgets,
        byteBudgets = byteBudgets,
        // Unconfined keeps every fetch and decode on the test's own thread, so
        // assertions run after the work rather than racing it.
        dispatcher = Dispatchers.Unconfined,
    )

    private fun <T : Any> T?.orFail(what: String): T = checkNotNull(this) { "expected $what to resolve" }
}
