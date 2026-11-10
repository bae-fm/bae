package fm.bae.app

/**
 * A failure ready to show a person: the localized line, and for a failure core
 * reported, the opaque diagnostic text behind it — the Rust error chain, never
 * translated — which the notice's Details button opens in a dialog, with a
 * copy action ([fm.bae.app.ui.components.ErrorNotice]). A failure this app
 * words itself carries no detail. Mirrors the Apple apps' `DisplayError`.
 */
data class DisplayError(
    val line: String,
    val detail: String?,
    /**
     * Whether a surface offers Retry on this failure: always, unless core says
     * making the same call again can't get past it — a damaged database, a bug,
     * an edit to fix. Prose this app wrote, and a failure core keys without
     * saying, have nothing ruling a retry out.
     */
    val offersRetry: Boolean,
) {
    /** An error this app words itself: prose with no diagnostic behind it. */
    constructor(line: String) : this(line, null, true)

    /** A headline this app words over a diagnostic core wrote, with nothing ruling a retry out. */
    constructor(line: String, detail: String?) : this(line, detail, true)

    /** The opening of the diagnostic, for the dialog's causes panel; the copy action takes all of [detail]. */
    val detailExcerpt: String?
        get() {
            val detail = detail ?: return null
            return if (detail.length > DETAIL_EXCERPT_LENGTH) detail.take(DETAIL_EXCERPT_LENGTH) + "…" else detail
        }

    private companion object {
        const val DETAIL_EXCERPT_LENGTH = 400
    }
}
