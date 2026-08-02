using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Session state for the import flow: the sidebar's core-projected triage queue,
// its watched folders, the sweep's identify progress, and the preview-position
// label. TriageQueue/WatchedFolders/QueueIdentifyProgress are core-driven
// (refreshed from an invalidation or a progress event); ActiveTab/FilterText/
// SortOrder/SelectedReady are view state the sidebar itself sets. Unlike
// macOS's per-field bindings, this store fires one coarse Changed event and the
// sidebar rebuilds its content wholesale — the established pattern for this
// app's imperative views (see QueuePane).
internal sealed class ImportStore
{
    private readonly ImportService _import;
    private readonly Action<string, string> _showError;
    private readonly IMediaControl _mediaControls;
    private readonly CoalescedReadModel _triageReads = new();

    // The sidebar's pre-shaped sections and tab counts — core's
    // triage projection, read whole and re-read on the import-domain
    // invalidations RefreshTriageQueue registers for. Defaults to an empty
    // queue rather than null: "not loaded yet" and "the queue is genuinely
    // empty" render identically (the tab's empty state), so no surface needs to
    // tell them apart.
    public BridgeTriageQueue TriageQueue { get; private set; } = new(
        Sections: Array.Empty<BridgeTriageSection>(),
        Counts: new BridgeTriageTabCounts(Ready: 0, NeedsYou: 0, Done: 0, Skipped: 0),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>());

    // The folders being watched for imports, in add order — the sidebar's "+"
    // menu.
    public List<BridgeWatchedFolder> WatchedFolders { get; private set; } = new();

    // The queue sweep's identified-count over total, for the header's progress
    // line and bar. Null before the first tick of a session — the header hides
    // rather than opening on a bar frozen at zero.
    public (uint Identified, uint Total)? QueueIdentifyProgress { get; private set; }

    // The active tab; resets to Ready on each dialog open and on teardown.
    public BridgeTriageTab ActiveTab { get; private set; } = BridgeTriageTab.Ready;

    // The live filter query over the candidate list.
    public string FilterText { get; private set; } = string.Empty;

    // The persisted candidate-list sort order, loaded once at construction and
    // saved whenever the sidebar changes it.
    public CandidateSortOrder SortOrder { get; private set; }

    // Bulk-select state for the Ready tab's foot bar. Selection is view state —
    // it is not persisted and does not cross the bridge.
    public HashSet<string> SelectedReady { get; } = new();
    public ReleaseQueueInteractionModel Interaction { get; } = new();

    // Fired whenever anything the sidebar renders changes: the triage queue,
    // watched folders, progress, active tab, filter text, sort order, or
    // selection. The sidebar rebuilds its content on every tick.
    public event Action? Changed;

    // The live preview-position label ("0:23 / 3:45"), driven by preview events
    // while a slot row auditions. The mapping pane renders it on
    // PreviewElapsedChanged; ClearPreview resets it when the pane moves to
    // another folder.
    public string PreviewElapsedText { get; private set; } = string.Empty;
    public event Action? PreviewElapsedChanged;

    // The file the preview transport is playing, by its absolute path — the
    // only identity the preview events carry. The mapping pane accents the slot
    // row whose audio this is; null when nothing is previewing.
    public string? PreviewingPath { get; private set; }

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

#if DEBUG
    internal void SeedPreview(
        BridgeTriageQueue queue,
        List<BridgeWatchedFolder> watchedFolders,
        BridgeTriageTab activeTab)
    {
        TriageQueue = queue;
        WatchedFolders = watchedFolders;
        ActiveTab = activeTab;
        Changed?.Invoke();
    }
#endif

    // Re-read the triage queue and the watched folders from core. A failed read
    // surfaces the import banner; no handle is a no-op. Called on the
    // import-domain invalidations (ImportCandidateList, ImportCandidate,
    // WatchedFolders, Release) and on the queue sweep's progress ticks, whose
    // comment on why explains the coalescing: a burst of ticks collapses into
    // the one query that finishes after the burst settles.
    public async void RefreshTriageQueue() => await RequestCandidateRead();

    private async Task RequestCandidateRead()
    {
        if (!_triageReads.Request())
        {
            return;
        }
        try
        {
            do
            {
                await ReadCandidates();
            } while (_triageReads.Complete());
        }
        catch
        {
            _triageReads.Fail();
            throw;
        }
    }

    // The read itself, awaitable — a caller that must see the new rows before
    // it reads them (the sheet-binding editor) waits on this rather than on an
    // `async void` it cannot observe.
    private async Task ReadCandidates()
    {
        var (queueCurrent, queueResult) = await _import.TriageQueue();
        if (!queueCurrent)
        {
            return;
        }
        if (queueResult.Queue is not { } queue)
        {
            _showError(Loc.Chrome("import.error_title"), queueResult.Error ?? Loc.Chrome("import.failed"));
            return;
        }
        TriageQueue = queue;
        Interaction.RetainGroupDisclosureKeys(
            queue.Sections
                .Select(section => section.Group)
                .OfType<BridgeTriageGroup>()
                .Select(group => GroupDisclosureKey(group.Key)));

        var (foldersCurrent, folders) = _import.WatchedFolders();
        if (foldersCurrent)
        {
            WatchedFolders = folders;
        }

        // Selection can outlive the rows that earned it (a row imported by a
        // faster sibling call, or reclassified out of Ready) — drop keys no
        // longer in Ready rather than let a bulk import act on a stale one.
        var currentReady = TriageListModel
            .SelectableReadyRows(TriageQueue, string.Empty, CandidateSortOrder.NameAZ)
            .Select(row => row.CandidateKey)
            .ToHashSet();
        SelectedReady.RemoveWhere(key => !currentReady.Contains(key));

        Changed?.Invoke();
    }

    // Show a different tab (absolute set — the caller passes the tab its button
    // represents).
    /// <summary>The candidate under `key`, read from core rather than from a
    /// cached list — the triage store holds rows, not whole candidates, and a
    /// caller wanting the files needs what the folder is now.</summary>
    public ImportCandidate? Candidate(string key)
    {
        var (current, candidate) = _import.CandidateForKey(key);
        return current ? candidate : null;
    }

    public void SetActiveTab(BridgeTriageTab tab)
    {
        ActiveTab = tab;
        Changed?.Invoke();
    }

    public void SetFilterText(string text)
    {
        FilterText = text;
        Changed?.Invoke();
    }

    // Change and persist the candidate-list sort order (absolute set — the
    // caller passes the order its control represents).
    public void SetSortOrder(CandidateSortOrder order)
    {
        SortOrder = order;
        ImportSortStore.Save(order);
        Changed?.Invoke();
    }

    public void ToggleReadySelection(string key)
    {
        if (!SelectedReady.Remove(key))
        {
            SelectedReady.Add(key);
        }
        Changed?.Invoke();
    }

    public void SelectAllReady(IEnumerable<string> keys)
    {
        ReadySelectionModel.Replace(SelectedReady, keys);
        Changed?.Invoke();
    }

    public void ClearReadySelection()
    {
        SelectedReady.Clear();
        Changed?.Invoke();
    }

    // CandidateVerdictStored invalidates each row list after its guarded DB
    // write. This event updates only the queue-wide header.
    public void ApplyQueueIdentifyProgress(uint identified, uint total)
    {
        QueueIdentifyProgress = (identified, total);
        Changed?.Invoke();
    }

    // Un-watch a folder: core drops it and its candidates, and the
    // ImportCandidateList/WatchedFolders invalidations re-render the sidebar. A
    // failed call surfaces the import banner; success is silent (the
    // invalidation drives the refresh).
    public async void RemoveWatchedFolder(string path)
    {
        var (current, error) = await _import.RemoveWatchedFolder(path);
        if (current && error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
        }
    }

    public async void RefreshWatchedFolder(string path)
    {
        Interaction.SetRefreshing(path, true);
        Changed?.Invoke();
        try
        {
            var (current, error) = await _import.RefreshWatchedFolder(path);
            if (current && error is not null)
            {
                _showError(
                    Loc.Chrome("import.error_title"),
                    FolderError(path, error));
            }
        }
        finally
        {
            Interaction.SetRefreshing(path, false);
            Changed?.Invoke();
        }
    }

    public async void SetFolderReleaseDecision(
        BridgeFolderReleaseDecisionKey key,
        BridgeFolderReleaseDecision decision)
    {
        var (current, error) = await _import.SetFolderReleaseDecision(key, decision);
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

    // What a track sheet may be bound to. Empty when core has nothing to offer —
    // a sheet naming one file per track, or a folder with no audio — and empty
    // on failure, with the reason on the import banner.
    public async Task<List<ImportSheetBindingOption>> SheetBindingOptions(string key, string sheetFileId)
    {
        var (current, result) = await _import.SheetBindingOptions(key, sheetFileId);
        if (!current)
        {
            return new List<ImportSheetBindingOption>();
        }
        var (options, error) = result;
        if (error is not null || options is null)
        {
            _showError(Loc.Chrome("import.error_title"), error ?? Loc.Chrome("import.failed"));
            return new List<ImportSheetBindingOption>();
        }
        return options;
    }

    // Name the audio a track sheet describes, or clear it with null. Core
    // persists the decision, clears the candidate's stored identify verdict, and
    // the list is re-read so the row's track count and format follow the shape
    // the folder now has. Returns whether the change landed.
    public async Task<bool> SetSheetBinding(string key, string sheetFileId, string? audioFileId)
    {
        var (current, error) = await _import.SetSheetBinding(key, sheetFileId, audioFileId);
        if (!current)
        {
            return false;
        }
        if (error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
            return false;
        }
        await RequestCandidateRead();
        return true;
    }

    // Say which disc of the release a track sheet's entries are, or take them
    // out of the tracklist. Re-shapes the tracklist exactly as a binding does,
    // so the list is re-read the same way. Returns whether the change landed.
    public async Task<bool> SetSheetDisc(string key, string sheetFileId, BridgeSheetDisc disc)
    {
        var (current, error) = await _import.SetSheetDisc(key, sheetFileId, disc);
        if (!current)
        {
            return false;
        }
        if (error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
            return false;
        }
        await RequestCandidateRead();
        return true;
    }

    // The mapping table for a folder nobody has picked a release for. Null when
    // the session moved on, and null with the reason on the import banner when
    // the read failed.
    public BridgeMappingTable? CandidateMapping(string key)
    {
        var (current, result) = _import.CandidateMapping(key);
        if (!current)
        {
            return null;
        }
        var (mapping, error) = result;
        if (error is not null || mapping is null)
        {
            _showError(Loc.Chrome("import.error_title"), error ?? Loc.Chrome("import.failed"));
            return null;
        }
        return mapping;
    }

    // Put one of a candidate's files in a role, or put it back. Core persists
    // the decision and clears the stored identify verdict; the triage queue is
    // re-read so the row's counts follow the shape the folder now has. Returns
    // whether the change landed — the mapping pane drops the excluded file's
    // slot rows itself rather than re-prefetching over the user's edits, so it
    // has to know the write succeeded before it does.
    public async Task<bool> SetFileRole(string key, string fileId, BridgeFileRoleChoice choice)
    {
        var (current, error) = await _import.SetFileRole(key, fileId, choice);
        if (!current)
        {
            return false;
        }
        if (error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
            return false;
        }
        await ReadCandidates();
        return true;
    }

    // Import-preview and candidate-loudness events, which drive the import
    // picker's live position label (and the system transport controls for the
    // preview session), plus the queue sweep's progress. Routed here by the
    // event router.
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
                PreviewingPath = preview.Path;
                PreviewElapsedChanged?.Invoke();
                _mediaControls.UpdateNowPlayingForPreview(preview.Path, preview.DurationMs, isPlaying: true);
                break;
            case BridgeUiEvent.PreviewPaused preview:
                _previewDurationLabel = BridgeDisplay.Clock(preview.DurationMs);
                PreviewingPath = preview.Path;
                PreviewElapsedChanged?.Invoke();
                _mediaControls.UpdateNowPlayingForPreview(preview.Path, preview.DurationMs, isPlaying: false);
                break;
            case BridgeUiEvent.PreviewIdle:
                _previewDurationLabel = null;
                PreviewingPath = null;
                PreviewElapsedText = string.Empty;
                PreviewElapsedChanged?.Invoke();
                _mediaControls.UpdatePreviewIdle();
                break;
            case BridgeUiEvent.CandidateImportLoudnessProgress:
                // The sidebar shows import progress as a percent + step off the
                // row's own BridgeCandidateImportStatus (re-read on the next
                // candidate invalidation); a per-track loudness fraction has no
                // leaf in this UI to drive.
                break;
            case BridgeUiEvent.ImportQueueIdentifyProgress progress:
                ApplyQueueIdentifyProgress(progress.Identified, progress.Total);
                break;
            default:
                // The router forwards only these variants here; any other one
                // reaching this handler is a routing drift, so log it rather
                // than dropping it silently.
                BaeDiagnostics.Logger.Warning(
                    $"Unexpected BridgeUiEvent variant {evt.GetType().Name} reached the import preview handler.");
                break;
        }
    }

    // Reset the preview label when the pane leaves the folder whose audio was
    // playing, so the next folder starts blank rather than showing the last
    // position.
    public void ClearPreview()
    {
        _previewDurationLabel = null;
        PreviewingPath = null;
        PreviewElapsedText = string.Empty;
        PreviewElapsedChanged?.Invoke();
    }

    // Scan a folder into the watched set; candidates stream in through
    // invalidations. Returns the error line, or null on success.
    public System.Threading.Tasks.Task<(bool Current, string? Result)> ScanFolder(string path) =>
        _import.ScanFolder(path);

    // Kick off auto-identification for an as-yet unidentified candidate — the
    // click gate for a row whose phase is still Queued.
    public System.Threading.Tasks.Task<bool> AutoIdentify(string candidateKey) =>
        _import.AutoIdentifyFolder(candidateKey);

    // Scan candidates, watched folders, and selection are per-library in-memory
    // state; clear them on teardown so the next library doesn't inherit the
    // previous one's list, and reset the tab to Ready. The sort order persists
    // — it's a preference, not library state.
    public void Reset()
    {
        TriageQueue = new BridgeTriageQueue(
            Sections: Array.Empty<BridgeTriageSection>(),
            Counts: new BridgeTriageTabCounts(Ready: 0, NeedsYou: 0, Done: 0, Skipped: 0),
            FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>());
        WatchedFolders = new List<BridgeWatchedFolder>();
        QueueIdentifyProgress = null;
        ActiveTab = BridgeTriageTab.Ready;
        FilterText = string.Empty;
        SelectedReady.Clear();
        Interaction.RetainGroupDisclosureKeys(
            Array.Empty<ReleaseGroupDisclosureKey>());
        Changed?.Invoke();
    }

    internal static ReleaseGroupDisclosureKey GroupDisclosureKey(
        BridgeFolderReleaseDecisionKey key) =>
        new(key.WatchedFolderPath, key.RelativeFolderPath);

    private string FolderError(string path, string detail)
    {
        var folder = WatchedFolders.FirstOrDefault(folder => folder.Path == path);
        return $"{folder?.Name ?? path} ({path}): {detail}";
    }
}
