package fm.bae.app.data

import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeListRows

/** The album grid's rows, as its view or a page of it delivers them. */
internal fun albumRows(rows: BridgeListRows): List<BridgeAlbumAppearance> =
    (rows as? BridgeListRows.Albums)?.rows ?: error("the album grid delivered rows of another list")

/** The composer list's rows, as its view or a page of it delivers them. */
internal fun composerRows(rows: BridgeListRows): List<BridgeComposerSummary> =
    (rows as? BridgeListRows.Composers)?.rows ?: error("the composer list delivered rows of another list")

/** A composer's page rows, as its view or a page of it delivers them. */
internal fun composerPageRows(rows: BridgeListRows): List<BridgeComposerDetailRow> =
    (rows as? BridgeListRows.ComposerPage)?.rows ?: error("a composer's page delivered rows of another list")
