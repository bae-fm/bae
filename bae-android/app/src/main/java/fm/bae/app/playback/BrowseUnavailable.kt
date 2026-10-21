package fm.bae.app.playback

/**
 * Why the browse tree can't answer a controller's request, when no read
 * failed: the controller is told the library isn't available, or that its
 * request was replaced, rather than that something went wrong reading it.
 */
internal sealed class BrowseUnavailable(
    message: String,
) : Exception(message) {
    /** The tree's library closed. */
    class LibraryClosed : BrowseUnavailable("library browse tree is closed")

    /** A newer request from the same controller replaced this one before it was answered. */
    class Superseded(
        what: String,
    ) : BrowseUnavailable(what)
}
