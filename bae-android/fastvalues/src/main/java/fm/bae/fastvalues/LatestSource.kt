package fm.bae.fastvalues

/**
 * One key of a fast stream, which a composable hands to the View that draws
 * it. Only FrameFollowing reads its value.
 */
@ConsistentCopyVisibility
data class LatestSource<V : Any> internal constructor(
    internal val stream: LatestStream<V>,
    internal val key: String,
) {
    internal val latest: V? get() = stream.latest(key)
    internal val moving: Boolean get() = stream.moving(key)
}
