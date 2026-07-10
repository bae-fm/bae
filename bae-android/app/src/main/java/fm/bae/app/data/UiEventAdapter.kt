package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.ErrorLines
import fm.bae.app.playback.PlaybackEventSink
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInvalidation
import uniffi.bae_bridge.BridgeUiEvent

private const val TAG = "bae.UiEventAdapter"
private val logger = BaeLogger(TAG)

/**
 * Routes bridge UI events into Android's app state. Query-backed state is
 * refreshed from core on invalidation; playback transport events still carry the
 * live payload the Android player needs to drive Media3.
 */
object UiEventAdapter {
    suspend fun handle(
        event: BridgeUiEvent,
        appHandle: AppHandle,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        try {
            route(event, appHandle, stores, player, errors)
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException.Cancelled) {
            logger.debug("event handling cancelled", e)
        } catch (e: BridgeException) {
            logger.error("event handling failed", e)
            stores.config.showError(errors.line(e))
        } catch (e: Exception) {
            logger.error("event handling failed", e)
            stores.config.showError(e.message ?: e::class.java.simpleName)
        }
    }

    private suspend fun route(
        event: BridgeUiEvent,
        appHandle: AppHandle,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        when (event) {
            is BridgeUiEvent.Invalidated -> {
                handleInvalidation(event.invalidation, appHandle, stores, errors)
            }

            else -> {
                handleDirectEvent(event, stores, player, errors)
            }
        }
    }

    private fun handleDirectEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        if (handlePlaybackEvent(event, stores, player, errors)) {
            return
        }
        if (handleAppErrorEvent(event, stores, errors)) {
            return
        }
        ignoreObsoleteEvent(event)
    }

    private fun handlePlaybackEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ): Boolean {
        when (event) {
            is BridgeUiEvent.PlaybackLoading -> {
                player.onLoading(event.trackId, event.track)
            }

            is BridgeUiEvent.PlaybackPlaying -> {
                player.onPlaying(event)
            }

            is BridgeUiEvent.PlaybackPaused -> {
                player.onPaused(event)
            }

            BridgeUiEvent.PlaybackStopped -> {
                player.onStopped()
            }

            is BridgeUiEvent.PlaybackError -> {
                stores.config.showError(errors.line(event.reason))
            }

            is BridgeUiEvent.PlaybackProgress -> {
                player.onProgress(
                    trackId = event.trackId,
                    positionMs = event.positionMs.toLong(),
                    durationMs = event.durationMs.toLong(),
                    progress = event.progress,
                )
            }

            is BridgeUiEvent.PlaybackSeeked -> {
                player.onSeeked(
                    trackId = event.trackId,
                    positionMs = event.positionMs.toLong(),
                    durationMs = event.durationMs.toLong(),
                    progress = event.progress,
                )
            }

            is BridgeUiEvent.RepeatModeChanged -> {
                player.onRepeatModeChanged(event.mode)
            }

            is BridgeUiEvent.QueueUpdated -> {
                player.onQueueUpdated(
                    manual = event.snapshot.manual,
                    context = event.snapshot.context,
                    hasNext = event.snapshot.hasNext,
                    hasPrevious = event.snapshot.hasPrevious,
                    revision = event.snapshot.revision,
                )
            }

            is BridgeUiEvent.VolumeChanged -> {
                player.onVolumeChanged(event.volume)
            }

            is BridgeUiEvent.MuteChanged -> {
                player.onMuteChanged(event.isMuted)
            }

            is BridgeUiEvent.QueueItemsAdded -> {
                player.onQueueItemsAdded(event.count.toInt())
            }

            else -> {
                return false
            }
        }
        return true
    }

    private fun handleAppErrorEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        errors: ErrorLines,
    ): Boolean {
        when (event) {
            is BridgeUiEvent.Error -> {
                stores.config.showError(errors.line(event.error))
            }

            BridgeUiEvent.ErrorCleared -> {
                stores.config.clearError()
            }

            else -> {
                return false
            }
        }
        return true
    }

    private fun ignoreObsoleteEvent(event: BridgeUiEvent) {
        when (event) {
            BridgeUiEvent.PreviewIdle,
            is BridgeUiEvent.PreviewPlaying,
            is BridgeUiEvent.PreviewPaused,
            is BridgeUiEvent.PreviewProgress,
            is BridgeUiEvent.CandidateImportLoudnessProgress,
            is BridgeUiEvent.ReleaseTransferProgress,
            is BridgeUiEvent.ReleaseTransferEnded,
            -> {
                logger.debug("ignoring ${event::class.simpleName}")
            }

            is BridgeUiEvent.Invalidated,
            is BridgeUiEvent.PlaybackLoading,
            is BridgeUiEvent.PlaybackPlaying,
            is BridgeUiEvent.PlaybackPaused,
            BridgeUiEvent.PlaybackStopped,
            is BridgeUiEvent.PlaybackError,
            is BridgeUiEvent.PlaybackProgress,
            is BridgeUiEvent.PlaybackSeeked,
            is BridgeUiEvent.RepeatModeChanged,
            is BridgeUiEvent.QueueUpdated,
            is BridgeUiEvent.VolumeChanged,
            is BridgeUiEvent.MuteChanged,
            is BridgeUiEvent.QueueItemsAdded,
            is BridgeUiEvent.Error,
            BridgeUiEvent.ErrorCleared,
            -> {
                error("handled event reached obsolete-event path: ${event::class.simpleName}")
            }
        }
    }

    private suspend fun handleInvalidation(
        invalidation: BridgeInvalidation,
        appHandle: AppHandle,
        stores: OpenLibraryStores,
        errors: ErrorLines,
    ) {
        when (invalidation) {
            BridgeInvalidation.AlbumList -> {
                stores.library.invalidateAlbumList()
            }

            BridgeInvalidation.ComposerList -> {
                stores.library.invalidateComposerList()
            }

            BridgeInvalidation.ArtistList -> {
                stores.library.invalidateArtistList()
            }

            is BridgeInvalidation.Album -> {
                val detail = withContext(Dispatchers.IO) { appHandle.getAlbumDetail(invalidation.albumId) }
                stores.library.replaceAlbumDetail(detail)
            }

            is BridgeInvalidation.Release -> {
                val release =
                    withContext(Dispatchers.IO) {
                        appHandle.findReleaseDetail(invalidation.releaseId)
                    }
                if (release != null) {
                    val detail = withContext(Dispatchers.IO) { appHandle.getAlbumDetail(release.albumId) }
                    stores.library.replaceAlbumDetail(detail)
                }
            }

            BridgeInvalidation.Config -> {
                val config = withContext(Dispatchers.IO) { appHandle.getConfig() }
                stores.config.setConfig(config)
            }

            BridgeInvalidation.SyncStatus -> {
                val snapshot = withContext(Dispatchers.IO) { appHandle.getSyncStatus() }
                stores.config.setSyncStatus(snapshot, errors)
            }

            BridgeInvalidation.DownloadQueue -> {
                val snapshot = withContext(Dispatchers.IO) { appHandle.getDownloadSnapshot() }
                stores.downloads.setSnapshot(snapshot)
            }

            BridgeInvalidation.Outbox -> {
                val snapshot = withContext(Dispatchers.IO) { appHandle.getOutboxSnapshot() }
                stores.outbox.setSnapshot(snapshot)
            }

            BridgeInvalidation.Queue,
            BridgeInvalidation.ExportQueue,
            BridgeInvalidation.ImportCandidateList,
            is BridgeInvalidation.ImportCandidate,
            BridgeInvalidation.WatchedFolders,
            is BridgeInvalidation.Composer,
            -> {
                logger.debug("ignoring ${invalidation::class.simpleName}")
            }
        }
    }
}
