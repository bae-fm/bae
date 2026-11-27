package fm.bae.app.data

import uniffi.bae_bridge.BridgePictureTurn

/**
 * How soon one request wants its image. Whoever asks for the image keeps this, and changes [asked] while the request
 * waits, as what the person looks at changes. A later [BridgePictureTurn] goes first.
 */
class ImageTurn(
    @Volatile var asked: BridgePictureTurn = BridgePictureTurn.VISIBLE,
)
