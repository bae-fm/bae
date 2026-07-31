package fm.bae.app

import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType

/** A cover reference for a test fixture: the release id plus a stand-in content
 *  version, which is what the payloads carrying cover art now hold. */
fun testCoverRef(id: String): BridgeImageRef =
    BridgeImageRef(id = id, version = "1", imageType = BridgeLibraryImageType.COVER)
