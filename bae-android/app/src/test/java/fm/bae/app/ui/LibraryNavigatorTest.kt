package fm.bae.app.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The navigator is an ordered in-memory back stack: pushing shows the new top,
 * popping returns to the previous level, an empty stack pops to null, and every
 * push gets a fresh key so revisiting the same destination never shares saved
 * UI state with a prior visit.
 */
class LibraryNavigatorTest {
    @Test
    fun pushShowsNewTopInOrder() {
        val navigator = LibraryNavigator()
        navigator.push(LibraryDestination.Composer("artist-1"))
        navigator.push(LibraryDestination.Work("work-1"))
        navigator.push(LibraryDestination.Album(AlbumTarget("album-1")))

        assertEquals(LibraryDestination.Album(AlbumTarget("album-1")), navigator.top?.destination)
        assertEquals(
            listOf(
                LibraryDestination.Composer("artist-1"),
                LibraryDestination.Work("work-1"),
                LibraryDestination.Album(AlbumTarget("album-1")),
            ),
            navigator.entries.map { it.destination },
        )
    }

    @Test
    fun popReturnsToPreviousLevel() {
        val navigator = LibraryNavigator()
        navigator.push(LibraryDestination.Composer("artist-1"))
        navigator.push(LibraryDestination.Work("work-1"))
        navigator.push(LibraryDestination.Album(AlbumTarget("album-1")))

        assertEquals(LibraryDestination.Album(AlbumTarget("album-1")), navigator.pop()?.destination)
        assertEquals(LibraryDestination.Work("work-1"), navigator.top?.destination)
        assertEquals(LibraryDestination.Work("work-1"), navigator.pop()?.destination)
        assertEquals(LibraryDestination.Composer("artist-1"), navigator.top?.destination)
    }

    @Test
    fun popOnEmptyReturnsNull() {
        val navigator = LibraryNavigator()
        assertNull(navigator.pop())
        assertTrue(navigator.entries.isEmpty())
    }

    @Test
    fun keysAreUniqueAcrossRepushOfSameDestination() {
        val navigator = LibraryNavigator()
        navigator.push(LibraryDestination.Work("work-1"))
        val firstKey = navigator.pop()?.key
        navigator.push(LibraryDestination.Work("work-1"))
        val secondKey = navigator.top?.key

        assertTrue(firstKey != secondKey)
    }

    @Test
    fun settingsThenMembersStacksBothLevels() {
        val navigator = LibraryNavigator()
        navigator.push(LibraryDestination.Settings)
        navigator.push(LibraryDestination.Members)

        assertEquals(LibraryDestination.Members, navigator.pop()?.destination)
        assertEquals(LibraryDestination.Settings, navigator.top?.destination)
    }
}
