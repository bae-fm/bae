using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Routes transient BridgeUiEvent values: queue-add confirmations and error
// banners, plus import progress. Retained playback, preview, cast, and queue
// state arrive through typed value subscriptions.
internal sealed class UiEventRouter
{
    private readonly PlaybackStore _playback;
    private readonly Action<string, string> _showError;
    private readonly Action<BridgeUiEvent> _importEvents;

    public UiEventRouter(
        PlaybackStore playback,
        Action<string, string> showError,
        Action<BridgeUiEvent> importEvents)
    {
        _playback = playback;
        _showError = showError;
        _importEvents = importEvents;
    }

    public void Route(BridgeUiEvent evt)
    {
        switch (evt)
        {
            case BridgeUiEvent.QueueItemsAdded added:
                _playback.ApplyQueueItemsAdded(checked((int)added.Count));
                break;
            case BridgeUiEvent.PlaybackError playbackError:
                // The structured reason resolves its own localized line (the
                // actionable cloud-only cases, or a diagnostic's generic line), or
                // null when core says there is nothing to show.
                if (BridgeDisplay.LocalizedLine(playbackError.Reason) is { } playbackLine)
                {
                    _showError(Loc.Chrome("error.playback_title"), playbackLine);
                }
                break;
            case BridgeUiEvent.Error error:
                // Null means a cancellation — the user's own doing — so no banner.
                if (BridgeDisplay.LocalizedLine(error.ErrorValue) is { } errorLine)
                {
                    _showError(Loc.Chrome("error.title"), errorLine);
                }
                break;
            case BridgeUiEvent.CandidateImportLoudnessProgress:
            case BridgeUiEvent.CandidateSignalsUpdated:
            case BridgeUiEvent.ImportQueueIdentifyProgress:
                _importEvents(evt);
                break;
            default:
                // A BridgeUiEvent variant with no arm above: log the drift so a new
                // variant surfaces here instead of vanishing silently.
                BaeDiagnostics.Logger.Warning($"Unhandled BridgeUiEvent variant {evt.GetType().Name}.");
                break;
        }
    }
}
