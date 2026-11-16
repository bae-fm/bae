package fm.bae.app

import uniffi.bae_bridge.BridgeStars

/** How many stars this is, one to five: bae-bridge declares BridgeStars in order from one star to five, so the
 *  ordinal plus one is the count. */
val BridgeStars.number: Int
    get() = ordinal + 1

/** The stars of [number], `null` outside one to five. */
fun starsOfNumber(number: Int): BridgeStars? = BridgeStars.entries.getOrNull(number - 1)
