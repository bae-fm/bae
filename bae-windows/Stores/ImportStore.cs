using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using Microsoft.UI.Xaml.Controls;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Mirror of the import scan state: the candidate releases found under the watched
// folders, plus the picker's live preview-position label. Refreshed from core
// when import invalidations arrive (registered for the window's lifetime) and
// bound to the import dialog's list; the picker subscribes to the preview label.
// The store holds session-local view state and drives core through the session;
// nothing derived crosses into it.
internal sealed class ImportStore
{
    private readonly SessionStore _session;
    private readonly ShellStore _shell;
    private readonly MediaControlService _mediaControls;

    // The scan candidates, bound to the import dialog's list. Repopulated in place
    // on every refresh so the bound list re-renders.
    public ObservableCollection<ImportCandidate> Candidates { get; } = new();

    // The import dialog's status line after a candidate refresh: the no-releases
    // line when the scan turned up nothing, else blank. The dialog renders it on
    // CandidatesRefreshed.
    public string CandidatesStatusText { get; private set; } = string.Empty;
    public event Action? CandidatesRefreshed;

    // The import picker's live preview-position label ("0:23 / 3:45"), driven by
    // preview events while a candidate previews. The picker renders it on
    // PreviewElapsedChanged; ClearPreview resets it when the picker closes.
    public string PreviewElapsedText { get; private set; } = string.Empty;
    public event Action? PreviewElapsedChanged;

    // The previewing track's total-duration label, from PreviewPlaying/Paused.
    // Shown after the elapsed position; null when nothing is previewing.
    private string? _previewDurationLabel;

    public ImportStore(SessionStore session, ShellStore shell, MediaControlService mediaControls)
    {
        _session = session;
        _shell = shell;
        _mediaControls = mediaControls;
    }

    // Re-read the scan candidates from core into Candidates and update the status
    // line. A failed read surfaces the import banner; no handle is a no-op.
    public async void RefreshCandidates()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        var (current, candidates) = await _session.RunForCurrentHandle(NativeBae.ImportCandidates);
        if (!current)
        {
            return;
        }
        if (candidates is null)
        {
            _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("import.error_title"), Loc.Chrome("import.failed"));
            return;
        }

        Candidates.Clear();
        foreach (var candidate in candidates)
        {
            Candidates.Add(candidate);
        }
        CandidatesStatusText = Candidates.Count == 0 ? Loc.Chrome("import.no_releases") : string.Empty;
        CandidatesRefreshed?.Invoke();
    }

    // Replace a candidate row in place (ObservableCollection raises Replace so the
    // bound list re-renders), applying an optional mutation and a live status.
    public void UpdateCandidate(string? key, Action<ImportCandidate>? mutate, string status)
    {
        var index = IndexOfCandidate(key);
        if (index < 0)
        {
            return;
        }

        var existing = Candidates[index];
        var updated = new ImportCandidate
        {
            Key = existing.Key,
            Name = existing.Name,
            TrackCount = existing.TrackCount,
            Format = existing.Format,
            Matches = existing.Matches,
            Signals = existing.Signals,
            AudioPaths = existing.AudioPaths,
            FolderPath = existing.FolderPath,
            RowStatus = existing.RowStatus,
            StatusOverride = status,
        };
        mutate?.Invoke(updated);
        Candidates[index] = updated;
    }

    private int IndexOfCandidate(string? key)
    {
        for (var i = 0; i < Candidates.Count; i++)
        {
            if (Candidates[i].Key == key)
            {
                return i;
            }
        }

        return -1;
    }

    // Import-preview and candidate-loudness events, which drive the import picker's
    // live position label (and the system transport controls for the preview
    // session). Routed here by the event router.
    public void HandlePreviewEvent(BridgeUiEvent evt)
    {
        switch (evt)
        {
            case BridgeUiEvent.PreviewProgress previewProgress:
                var elapsed = PlaybackPositionModel.DurationLabel(previewProgress.PositionMs);
                PreviewElapsedText = _previewDurationLabel is null
                    ? elapsed
                    : $"{elapsed} / {_previewDurationLabel}";
                PreviewElapsedChanged?.Invoke();
                _mediaControls.UpdatePreviewPosition(previewProgress.PositionMs);
                break;
            case BridgeUiEvent.PreviewPlaying preview:
                // Total duration arrives once when preview starts; the next
                // PreviewProgress tick renders it alongside the elapsed position.
                _previewDurationLabel = PlaybackPositionModel.DurationLabel(preview.DurationMs);
                _mediaControls.UpdateNowPlayingForPreview(preview.Path, preview.DurationMs, isPlaying: true);
                break;
            case BridgeUiEvent.PreviewPaused preview:
                _previewDurationLabel = PlaybackPositionModel.DurationLabel(preview.DurationMs);
                _mediaControls.UpdateNowPlayingForPreview(preview.Path, preview.DurationMs, isPlaying: false);
                break;
            case BridgeUiEvent.PreviewIdle:
                _previewDurationLabel = null;
                PreviewElapsedText = string.Empty;
                PreviewElapsedChanged?.Invoke();
                _mediaControls.UpdatePreviewIdle();
                break;
            case BridgeUiEvent.CandidateImportLoudnessProgress loudness:
                // Replace the candidate status with a live per-track loudness line.
                UpdateCandidate(loudness.Key, null, Loc.Core(
                    "ui.import.loudness_progress",
                    new Dictionary<string, object?>
                    {
                        ["done"] = loudness.TracksDone,
                        ["total"] = loudness.TracksTotal,
                    }));
                break;
            default:
                // The router forwards only the preview and candidate-loudness
                // variants here; any other variant reaching this handler is a
                // routing drift, so log it rather than dropping it silently.
                BaeDiagnostics.Logger.Warning(
                    $"Unexpected BridgeUiEvent variant {evt.GetType().Name} reached the import preview handler.");
                break;
        }
    }

    // Reset the preview label when the picker closes so a reopened picker starts
    // blank rather than showing the last position.
    public void ClearPreview()
    {
        _previewDurationLabel = null;
        PreviewElapsedText = string.Empty;
        PreviewElapsedChanged?.Invoke();
    }

    // Scan a folder into the watched set (clearing the prior scan); candidates
    // stream in through invalidations. Returns the error line, or null on success.
    public System.Threading.Tasks.Task<(bool Current, string? Result)> ScanFolder(string path) =>
        _session.RunForCurrentHandle(handle => NativeBae.ScanFolder(handle, path, true));

    // Kick off auto-identification for an as-yet unidentified candidate.
    public System.Threading.Tasks.Task<bool> AutoIdentify(string candidateKey, string folderPath) =>
        _session.RunForCurrentHandle(handle => NativeBae.AutoIdentifyFolder(handle, candidateKey, folderPath));

    // Re-dispatch a candidate's lookups, keeping the user's signal exclusions.
    public System.Threading.Tasks.Task<bool> RerunIdentify(string candidateKey) =>
        _session.RunForCurrentHandle(handle => NativeBae.RerunIdentifyForCandidate(handle, candidateKey));

    // Toggle a signal in or out of the candidate's triangulation.
    public System.Threading.Tasks.Task<bool> ToggleSignal(string candidateKey, string kind, string value) =>
        _session.RunForCurrentHandle(handle => NativeBae.ToggleSignalForCandidate(handle, candidateKey, kind, value));

    // Scan candidates are per-library in-memory state; clear them on teardown so
    // the next library doesn't inherit the previous one's candidate list.
    public void Reset() => Candidates.Clear();
}
