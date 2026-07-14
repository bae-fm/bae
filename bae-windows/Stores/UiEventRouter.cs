using System;
using Microsoft.UI.Xaml.Controls;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Routes each BridgeUiEvent to the store that owns its field: playback events to
// the playback store, error banners to the shell store, invalidations to the
// projection registry. Playback events also push metadata, status, and timeline
// to the system media transport controls. Import-preview and candidate-loudness
// events are handed to the window, which owns the import picker's live labels
// (and drives the transport controls for the preview session).
internal sealed class UiEventRouter
{
    private readonly PlaybackStore _playback;
    private readonly ShellStore _shell;
    private readonly ProjectionRegistry _projections;
    private readonly MediaControlService _mediaControls;
    private readonly Action<BridgeUiEvent> _importEvents;
    private readonly TransferProgressStore _transfers;

    public UiEventRouter(
        PlaybackStore playback,
        ShellStore shell,
        ProjectionRegistry projections,
        MediaControlService mediaControls,
        Action<BridgeUiEvent> importEvents,
        TransferProgressStore transfers)
    {
        _playback = playback;
        _shell = shell;
        _projections = projections;
        _mediaControls = mediaControls;
        _importEvents = importEvents;
        _transfers = transfers;
    }

    public void Route(BridgeUiEvent evt)
    {
        switch (evt)
        {
            case BridgeUiEvent.PlaybackPlaying playing:
                _playback.ApplyPlaying(playing.AlbumId, playing.TrackId, playing.TrackTitle, playing.ArtistNames, playing.CoverImageId);
                _mediaControls.UpdateNowPlayingPlaying(
                    playing.TrackTitle, playing.ArtistNames, playing.AlbumTitle, playing.CoverImageId, playing.DurationMs);
                break;
            case BridgeUiEvent.PlaybackPaused paused:
                _playback.ApplyPaused(paused.AlbumId, paused.TrackId, paused.TrackTitle, paused.ArtistNames, paused.CoverImageId, paused.Reason);
                _mediaControls.UpdateNowPlayingPaused(
                    paused.TrackTitle, paused.ArtistNames, paused.AlbumTitle, paused.CoverImageId, paused.DurationMs);
                break;
            case BridgeUiEvent.PlaybackStopped:
                _playback.ApplyStopped();
                _mediaControls.UpdateNowPlayingStopped();
                break;
            case BridgeUiEvent.PlaybackProgress progress:
                _playback.ApplyProgress(progress.TrackId, progress.PositionMs, progress.DurationMs, progress.Progress);
                _mediaControls.UpdatePosition(progress.PositionMs, progress.DurationMs);
                break;
            case BridgeUiEvent.PlaybackSeeked seeked:
                _playback.ApplySeeked(seeked.TrackId, seeked.PositionMs, seeked.DurationMs, seeked.Progress);
                _mediaControls.UpdatePosition(seeked.PositionMs, seeked.DurationMs);
                break;
            case BridgeUiEvent.PlaybackLoading loading:
                _playback.ApplyLoading(loading.TrackId);
                // A bare loading event carries no metadata; leave the transport
                // controls showing the prior track until the target resolves.
                if (loading.Track is { } loadingTrack)
                {
                    _mediaControls.UpdateNowPlayingLoading(
                        loadingTrack.TrackTitle, loadingTrack.ArtistNames, loadingTrack.AlbumTitle,
                        loadingTrack.CoverImageId, loadingTrack.DurationMs);
                }
                break;
            case BridgeUiEvent.VolumeChanged volume:
                _playback.ApplyVolume(volume.Volume);
                break;
            case BridgeUiEvent.MuteChanged mute:
                _playback.ApplyMute(mute.IsMuted);
                break;
            case BridgeUiEvent.RepeatModeChanged repeat:
                _playback.ApplyRepeat(repeat.Mode);
                break;
            case BridgeUiEvent.QueueUpdated queue:
                _playback.ApplyQueueUpdated(queue.Snapshot);
                _mediaControls.UpdateCommandAvailability(queue.Snapshot.HasNext, queue.Snapshot.HasPrevious);
                break;
            case BridgeUiEvent.QueueItemsAdded added:
                _playback.ApplyQueueItemsAdded(checked((int)added.Count));
                break;
            case BridgeUiEvent.PlaybackError playbackError:
                // The structured reason resolves its own localized line (the
                // actionable cloud-only cases, or a diagnostic's generic line), or
                // null when core says there is nothing to show.
                if (BridgeDisplay.LocalizedLine(playbackError.Reason) is { } playbackLine)
                {
                    _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("error.playback_title"), playbackLine);
                }
                break;
            case BridgeUiEvent.Error error:
                // Null means a cancellation — the user's own doing — so no banner.
                if (BridgeDisplay.LocalizedLine(error.ErrorValue) is { } errorLine)
                {
                    _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("error.title"), errorLine);
                }
                break;
            case BridgeUiEvent.Invalidated invalidated:
                _projections.Invalidate(invalidated.Invalidation);
                break;
            case BridgeUiEvent.PreviewProgress:
            case BridgeUiEvent.PreviewPlaying:
            case BridgeUiEvent.PreviewPaused:
            case BridgeUiEvent.PreviewIdle:
            case BridgeUiEvent.CandidateImportLoudnessProgress:
                _importEvents(evt);
                break;
            case BridgeUiEvent.ReleaseTransferProgress transfer:
                // A pin/unpin/manage/unmanage transition started: light the
                // in-flight indicator on the storage row and album-detail band
                // until the matching ended event lands.
                _transfers.Apply(transfer.ReleaseId, NativeBae.TransferActionToken(transfer.Action));
                break;
            case BridgeUiEvent.ReleaseTransferEnded ended:
                // The transition finished (success or failure) — clear the
                // indicator. Core's drop guard guarantees this event even on abort,
                // so overlay entries cannot leak.
                _transfers.Clear(ended.ReleaseId);
                break;
            default:
                // A BridgeUiEvent variant with no arm above: log the drift so a new
                // variant surfaces here instead of vanishing silently.
                BaeDiagnostics.Logger.Warning($"Unhandled BridgeUiEvent variant {evt.GetType().Name}.");
                break;
        }
    }
}
