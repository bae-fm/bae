package fm.bae.app

/**
 * A failure ready to show a person: the localized line, and for a failure core
 * reported, the opaque diagnostic text behind it — the Rust error chain, never
 * translated — which [fm.bae.app.ui.components.ErrorDetailDisclosure] offers
 * beside the line with a copy action. A failure this app words itself carries
 * no detail. Mirrors the Apple apps' `DisplayError`.
 */
data class DisplayError(
    val line: String,
    val detail: String?,
) {
    /** An error this app words itself: prose with no diagnostic behind it. */
    constructor(line: String) : this(line, null)

    /**
     * The concrete fault as one compact line, for showing under [line], which
     * names only a category. The diagnostic's first line, bounded; null when
     * there is no diagnostic or it is blank.
     */
    val detailSummary: String?
        get() {
            val firstLine = detail?.lineSequence()?.first()?.trim() ?: return null
            if (firstLine.isEmpty()) return null
            return if (firstLine.length > DETAIL_SUMMARY_LENGTH) {
                firstLine.take(DETAIL_SUMMARY_LENGTH) + "…"
            } else {
                firstLine
            }
        }

    /** The opening of the diagnostic, for the expanded disclosure; the copy action takes all of [detail]. */
    val detailExcerpt: String?
        get() {
            val detail = detail ?: return null
            return if (detail.length > DETAIL_EXCERPT_LENGTH) detail.take(DETAIL_EXCERPT_LENGTH) + "…" else detail
        }

    private companion object {
        const val DETAIL_EXCERPT_LENGTH = 400
        const val DETAIL_SUMMARY_LENGTH = 180
    }
}
