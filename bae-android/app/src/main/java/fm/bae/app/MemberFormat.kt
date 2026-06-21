package fm.bae.app

/** How many leading hex characters of a public key form its fingerprint. */
private const val FINGERPRINT_LENGTH = 8

/**
 * The short, human-comparable form of a device's public key: the first eight hex
 * characters. Both sides of an approval read the same eight characters off their
 * respective screens to confirm they're talking about the same device, so this
 * is the one definition of "fingerprint" the membership UI uses — the joiner
 * showing its own request code, and the owner confirming the code it scanned.
 */
fun pubkeyFingerprint(pubkey: String): String = pubkey.take(FINGERPRINT_LENGTH)
