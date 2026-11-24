package fm.bae.app.data

import android.graphics.Bitmap
import fm.bae.app.BaeRobolectricTestRunner
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/**
 * What the store promises its callers: a cached decode is pinned to the exact
 * bytes it came from, one budget bounds every kind of decode, requests for one
 * image share one load, and a completed load leaves something the next first
 * frame can read synchronously.
 *
 * Native graphics so `BitmapFactory` really decodes and `allocationByteCount`
 * really reports what a bitmap costs — the cache budgets mean nothing against
 * shadow bitmaps.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ImageStoreTest {
    private val fitTo64 = DecodeSize.FitTo(64)
    private val decodePool =
        Executors
            .newFixedThreadPool(2) { Thread(it, "$DECODE_THREAD_PREFIX-test") }
            .asCoroutineDispatcher()

    @After
    fun closeDecodePool() = decodePool.close()

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
    fun oneBudgetBoundsEveryKindOfContent() =
        runBlocking {
            // A 128px source at 64px costs 64 × 64 × 4 = 16 KB, so a 32 KB budget holds
            // two decodes. A third, of another kind, pushes out the least recently used.
            val store =
                storeOf(
                    library = { TestImages.png(128) },
                    release = { TestImages.png(128) },
                    decodedBudget = 32 * 1024,
                )
            val cover = ImageContent.LibraryImage(coverRef("rel-1"))
            store.image(cover, fitTo64).orFail("cover decode")

            store.image(ImageContent.ReleaseImage("rel-1", BridgeGallerySource.ReleaseFile("file-1")), fitTo64)
            store.image(ImageContent.ReleaseImage("rel-1", BridgeGallerySource.ReleaseFile("file-2")), fitTo64)

            assertNull("the cover is the oldest decode, so a release image evicts it", store.cachedImage(cover, fitTo64))
        }

    @Test
    fun concurrentRequestsShareOneLoad() =
        runBlocking {
            var crossings = 0
            val decodes = AtomicInteger()
            val release = CompletableDeferred<Unit>()
            val store =
                storeOf(
                    library = {
                        crossings++
                        release.await()
                        TestImages.png(128)
                    },
                    decode = { bytes, size ->
                        decodes.incrementAndGet()
                        decodeBytes(bytes, size)
                    },
                )
            val content = ImageContent.LibraryImage(coverRef("rel-1"))

            // Each request runs until it waits on the load, so both are in flight
            // before the fetch is let go.
            val first = async(start = CoroutineStart.UNDISPATCHED) { store.image(content, fitTo64) }
            val second = async(start = CoroutineStart.UNDISPATCHED) { store.image(content, fitTo64) }
            release.complete(Unit)

            assertSame("both requests get the one decode", first.await(), second.await())
            assertEquals(1, crossings)
            assertEquals(1, decodes.get())
        }

    @Test
    fun cancellingTheLastRequestBeforeTheDecodeStartsCancelsTheFetch() =
        runBlocking {
            val fetching = CompletableDeferred<Unit>()
            val cancelled = CompletableDeferred<Unit>()
            val store =
                ImageStore(
                    fetchLibraryImageBytes = {
                        fetching.complete(Unit)
                        try {
                            awaitCancellation()
                        } finally {
                            cancelled.complete(Unit)
                        }
                    },
                    dispatcher = Dispatchers.Unconfined,
                )
            val request = launch { store.image(ImageContent.LibraryImage(coverRef("rel-1")), fitTo64) }
            fetching.await()

            request.cancel()

            withTimeout(5_000) { cancelled.await() }
        }

    @Test
    fun aRequestAfterTheLastWasCancelledJoinsTheRunningDecode() =
        runBlocking {
            val decodeStarted = CompletableDeferred<Unit>()
            val letDecodeFinish = CountDownLatch(1)
            val decodes = AtomicInteger()
            var crossings = 0
            val store =
                storeOf(
                    library = {
                        crossings++
                        TestImages.png(128)
                    },
                    dispatcher = decodePool,
                    decode = { bytes, size ->
                        decodes.incrementAndGet()
                        decodeStarted.complete(Unit)
                        letDecodeFinish.await()
                        decodeBytes(bytes, size)
                    },
                )
            val content = ImageContent.LibraryImage(coverRef("rel-1"))

            val first = launch { store.image(content, fitTo64) }
            decodeStarted.await()
            first.cancel()
            val second = async { store.image(content, fitTo64) }
            letDecodeFinish.countDown()

            assertNotNull("the second request is answered by the running decode", second.await())
            first.join()
            assertEquals("one decode for both requests", 1, decodes.get())
            assertEquals("one read of the bytes", 1, crossings)
        }

    @Test
    fun decodesRunOnTheDecodeThreadsNotTheCallers() =
        runBlocking {
            val decodeThreads = mutableListOf<String>()
            val store =
                storeOf(
                    library = { TestImages.png(128) },
                    dispatcher = decodePool,
                    decode = { bytes, size ->
                        synchronized(decodeThreads) { decodeThreads += Thread.currentThread().name }
                        decodeBytes(bytes, size)
                    },
                )

            store.image(ImageContent.LibraryImage(coverRef("rel-1")), fitTo64).orFail("decode")

            assertEquals(1, decodeThreads.size)
            assertTrue(decodeThreads.single(), decodeThreads.single().startsWith(DECODE_THREAD_PREFIX))
            assertNotEquals(Thread.currentThread().name, decodeThreads.single())
        }

    @Test
    fun aReleaseFileIdNamesOneImmutableFile() =
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
    fun aLibraryImageWithNoBytesResolvesToNothing() =
        runBlocking {
            val store = storeOf(library = { null })
            assertNull(store.image(ImageContent.LibraryImage(coverRef("rel-1")), fitTo64))
        }

    @Test
    fun bytesThatArentAnImageAreAFailureNotAMissingImage() {
        val store = storeOf(library = { "not an image".toByteArray() })
        assertThrows(UnreadableImage::class.java) {
            runBlocking { store.image(ImageContent.LibraryImage(coverRef("rel-1")), fitTo64) }
        }
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
        library: (suspend () -> ByteArray?)? = null,
        release: (suspend () -> ByteArray)? = null,
        decodedBudget: Int = 48 * 1024 * 1024,
        // Unconfined keeps every fetch and decode on the test's own thread, so
        // assertions run after the work rather than racing it.
        dispatcher: CoroutineDispatcher = Dispatchers.Unconfined,
        decode: (ByteArray, DecodeSize) -> Bitmap? = ::decodeBytes,
    ) = ImageStore(
        fetchLibraryImageBytes = { checkNotNull(library) { "no library-image fetch in this store" }() },
        fetchReleaseImageBytes = { _, _ -> checkNotNull(release) { "no release-image fetch in this store" }() },
        decodedBudget = decodedBudget,
        dispatcher = dispatcher,
        decode = decode,
    )

    private fun <T : Any> T?.orFail(what: String): T = checkNotNull(this) { "expected $what to resolve" }

    private companion object {
        const val DECODE_THREAD_PREFIX = "image-decode"
    }
}
