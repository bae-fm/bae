package fm.bae.app

import uniffi.bae_bridge.BridgeStars

/** How many stars this is, one to five: the stars are declared from one up. */
val BridgeStars.number: Int
    get() = ordinal + 1

/** The stars of [number], `null` outside one to five. */
fun starsOfNumber(number: Int): BridgeStars? = BridgeStars.entries.getOrNull(number - 1)
