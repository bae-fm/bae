using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Mirror of the import scan state: the candidate releases found under the watched
// folders, plus the picker's live preview-position label. Refreshed from core
// when import invalidations arrive (registered for the window's lifetime) and
// bound to the import dialog's list; the picker subscribes to the preview label.
// The store holds session-local view state and drives core through the session;
// nothing derived crosses into it.
internal sealed class ImportStore
{
    private readonly ImportService _import;
    private readonly Action<string, string> _showError;
    private readonly IMediaControl _mediaControls;

    // The active tab's candidates in sort order, bound to the import dialog's list.
    // Repopulated in place on every projection so the bound list re-renders; it is
    // the displayed tab, not the whole scan.
    public ObservableCollection<ImportCandidate> Candidates { get; } = new();

    // Every scanned candidate in core snapshot order (watched-folder add order,
    // then path) — the "date added" baseline the projection tabs and sorts.
    private readonly List<ImportCandidate> _all = new();

    // The watched folders behind the current scan, bound to the dialog's folders
    // section. Repopulated in place on each refresh.
    public ObservableCollection<BridgeWatchedFolder> WatchedFolders { get; } = new();

    // The tab the dialog currently shows; the projection filters Candidates to it.
    // Resets to New on each dialog open and on teardown.
    public CandidateTab ActiveTab { get; private set; } = CandidateTab.New;

    // The persisted candidate-list sort order, loaded once at construction and
    // saved whenever the dialog changes it.
    public CandidateSortOrder SortOrder { get; private set; }

    // The per-tab candidate counts from the last projection, for the tab labels.
    public (int New, int Added, int Skipped) TabCounts { get; private set; }

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

    public ImportStore(ImportService import, Action<string, string> showError, IMediaControl mediaControls)
    {
        _import = import;
        _showError = showError;
        _mediaControls = mediaControls;
        SortOrder = ImportSortStore.Load();
    }

    // Re-read the scan from core into _all and the watched folders, then project
    // onto the active tab. A failed read surfaces the import banner; no handle is a
    // no-op.
    public async void RefreshCandidates()
    {
        var (current, result) = await _import.ImportCandidates();
        if (!current)
        {
            return;
        }
        var (rows, folders) = result;
        if (rows is null || folders is null)
        {
            _showError(Loc.Chrome("import.error_title"), Loc.Chrome("import.failed"));
            return;
        }

        _all.Clear();
        _all.AddRange(rows);
        WatchedFolders.Clear();
        foreach (var folder in folders)
        {
            WatchedFolders.Add(folder);
        }
        // The no-releases line reflects the whole scan, not the active tab: an empty
        // tab under a non-empty scan shows a blank status and lets the tab counts
        // explain it.
        CandidatesStatusText = _all.Count == 0 ? Loc.Chrome("import.no_releases") : string.Empty;
        Reproject();
    }

    // Show a different tab (absolute set — the caller passes the tab its button
    // represents). Re-projects Candidates onto the new tab.
    public void SetActiveTab(CandidateTab tab)
    {
        ActiveTab = tab;
        Reproject();
    }

    // Change and persist the candidate-list sort order (absolute set — the caller
    // passes the order its control represents). Re-projects Candidates.
    public void SetSortOrder(CandidateSortOrder order)
    {
        SortOrder = order;
        ImportSortStore.Save(order);
        Reproject();
    }

    // Rebuild the tab counts and the displayed Candidates from _all for the active
    // tab and sort order, then announce the refresh. Candidates becomes exactly the
    // active tab's rows in sort order, so the bound list is the tab.
    private void Reproject()
    {
        var listRows = _all.Select(ToListRow).ToList();
        TabCounts = ImportListModel.Counts(listRows);
        var displayed = ImportListModel.DisplayedKeys(listRows, ActiveTab, SortOrder);

        var byKey = new Dictionary<string, ImportCandidate>(_all.Count);
        foreach (var candidate in _all)
        {
            byKey[candidate.Key] = candidate;
        }

        Candidates.Clear();
        foreach (var key in displayed)
        {
            Candidates.Add(byKey[key]);
        }
        CandidatesRefreshed?.Invoke();
    }

    private static ImportListRow ToListRow(ImportCandidate candidate) =>
        new(
            candidate.Key,
            candidate.Name,
            candidate.Skipped,
            candidate.IsAdded,
            candidate.RowStatus.Kind == "complete",
            candidate.Invalid);

    // Replace a candidate row in place, applying an optional mutation and a live
    // status. Updates the snapshot baseline and, when the row is on the active tab,
    // mirrors the replacement into Candidates (ObservableCollection raises Replace
    // so the bound list re-renders). Tab membership is unchanged here — import-state
    // changes that would re-tab a row arrive through an invalidation-driven refresh.
    public void UpdateCandidate(string? key, Action<ImportCandidate>? mutate, string status)
    {
        var index = IndexInAll(key);
        if (index < 0)
        {
            return;
        }

        var existing = _all[index];
        var updated = new ImportCandidate
        {
            Key = existing.Key,
            Name = existing.Name,
            TrackCount = existing.TrackCount,
            Format = existing.Format,
            Matches = existing.Matches,
            Signals = existing.Signals,
            AudioPaths = existing.AudioPaths,
            Documents = existing.Documents,
            FolderPath = existing.FolderPath,
            RowStatus = existing.RowStatus,
            StatusOverride = status,
            Skipped = existing.Skipped,
            IsAdded = existing.IsAdded,
            Invalid = existing.Invalid,
        };
        mutate?.Invoke(updated);
        _all[index] = updated;

        var displayedIndex = IndexOfCandidate(key);
        if (displayedIndex >= 0)
        {
            Candidates[displayedIndex] = updated;
        }
    }

    // Un-watch a folder: core drops it and its candidates, and the WatchedFolders /
    // candidate invalidations re-render the dialog. A failed call surfaces the
    // import banner; success is silent (the invalidation drives the refresh).
    public async void RemoveWatchedFolder(string path)
    {
        var (current, error) = await _import.RemoveWatchedFolder(path);
        if (current && error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
        }
    }

    // Skip or un-skip a candidate: core persists the change and the candidate
    // invalidation re-tabs the row. A failed call surfaces the import banner;
    // success is silent (the invalidation drives the refresh).
    public async void SetCandidateSkipped(string key, bool skipped)
    {
        var (current, error) = await _import.SetCandidateSkipped(key, skipped);
        if (current && error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
        }
    }

    private int IndexInAll(string? key)
    {
        for (var i = 0; i < _all.Count; i++)
        {
            if (_all[i].Key == key)
            {
                return i;
            }
        }

        return -1;
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
                var elapsed = BridgeDisplay.Clock(previewProgress.PositionMs);
                PreviewElapsedText = _previewDurationLabel is null
                    ? elapsed
                    : $"{elapsed} / {_previewDurationLabel}";
                PreviewElapsedChanged?.Invoke();
                _mediaControls.UpdatePreviewPosition(previewProgress.PositionMs);
                break;
            case BridgeUiEvent.PreviewPlaying preview:
                // Total duration arrives once when preview starts; the next
                // PreviewProgress tick renders it alongside the elapsed position.
                _previewDurationLabel = BridgeDisplay.Clock(preview.DurationMs);
                _mediaControls.UpdateNowPlayingForPreview(preview.Path, preview.DurationMs, isPlaying: true);
                break;
            case BridgeUiEvent.PreviewPaused preview:
                _previewDurationLabel = BridgeDisplay.Clock(preview.DurationMs);
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
        _import.ScanFolder(path);

    // Kick off auto-identification for an as-yet unidentified candidate.
    public System.Threading.Tasks.Task<bool> AutoIdentify(string candidateKey, string folderPath) =>
        _import.AutoIdentifyFolder(candidateKey, folderPath);

    // Re-dispatch a candidate's lookups, keeping the user's signal exclusions.
    public System.Threading.Tasks.Task<bool> RerunIdentify(string candidateKey) =>
        _import.RerunIdentifyForCandidate(candidateKey);

    // Toggle a signal in or out of the candidate's triangulation.
    public System.Threading.Tasks.Task<bool> ToggleSignal(string candidateKey, string kind, string value) =>
        _import.ToggleSignalForCandidate(candidateKey, kind, value);

    // Scan candidates and watched folders are per-library in-memory state; clear
    // them on teardown so the next library doesn't inherit the previous one's list,
    // and reset the tab to New. The sort order persists — it's a preference, not
    // library state.
    public void Reset()
    {
        Candidates.Clear();
        _all.Clear();
        WatchedFolders.Clear();
        TabCounts = default;
        ActiveTab = CandidateTab.New;
    }
}
