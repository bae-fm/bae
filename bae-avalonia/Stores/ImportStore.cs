using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Session state for the import flow: the sidebar's paged list of core-placed
// items, the chrome around it, the sweep's identify progress, and the
// preview-position label. The list and its summary are core-driven and arrive
// one window at a time; QueueIdentifyProgress is driven by progress events;
// ActiveTab/FilterText/SortOrder/SelectedReady are view state the sidebar sets,
// and each of the first three travels to core in the list's view request
// because it changes which item sits at which offset. Unlike macOS's per-field
// bindings, this store fires one coarse Changed event and the sidebar rebuilds
// its chrome wholesale — the established pattern for this app's imperative
// views (see QueuePane).
internal sealed class ImportStore : IDisposable
{
    private readonly ImportService _import;
    private readonly Action<string, string> _showError;
    private readonly IMediaControl _mediaControls;
    private readonly Action<Action> _dispatch;
    private IDisposable? _releaseLibraryStatusSubscription;
    private long _releaseLibraryStatusGeneration;

    // Everything the chrome around the list shows — the tab counts, the Ready
    // set, the group keys, the row the identify count is waiting on — computed
    // by core in the same pass as the items, so none of it can disagree with
    // them. Defaults to an empty summary rather than null: "not loaded yet" and
    // "the queue is genuinely empty" render identically (the tab's empty
    // state), so no surface needs to tell them apart.
    public BridgeImportQueueSummary Summary { get; private set; } = EmptySummary;

    // The folders being watched for imports, in add order — the sidebar's "+"
    // menu.
    public List<BridgeWatchedFolder> WatchedFolders { get; private set; } = new();

    // The items of every window the list has loaded, by their stable key: what
    // a realized row resolves its position through.
    private readonly Dictionary<string, BridgeImportListItem> _items = new();

    // Every key's runtime as the runtime stream last reported it, the candidate
    // the pane is holding as its own query last read it, and the pipelines of
    // the re-identify keys, which name releases rather than scanned folders.
    private readonly Dictionary<string, BridgeCandidateRuntimeSnapshot> _runtime = new();
    private readonly Dictionary<string, BridgeImportCandidateDetail> _details = new();
    private readonly Dictionary<string, ImportCandidate> _candidates = new();
    private readonly Dictionary<string, (
        ImportCandidateRowStatus RowStatus,
        List<ReleaseCandidateChoice> Matches,
        List<SignalBadge> Signals)> _runtimeCandidates = new();

    private ImportListPageSource _source;
    private IDisposable? _observedCandidate;
    private string? _observedKey;

    // The queue sweep's identified-count over total, for the header's progress
    // line and bar. Null before the first tick of a session — the header hides
    // rather than opening on a bar frozen at zero.
    public (uint Identified, uint Total)? QueueIdentifyProgress { get; private set; }

    // The active tab; resets to Pending on each section entry and on teardown.
    public BridgeTriageTab ActiveTab { get; private set; } = BridgeTriageTab.Pending;

    // The live filter query over the candidate list.
    public string FilterText { get; private set; } = string.Empty;

    // The persisted candidate-list sort order, loaded once at construction and
    // saved whenever the sidebar changes it.
    public CandidateSortOrder SortOrder { get; private set; }

    // Bulk-select state for Pending's importable rows. Selection is view state —
    // it is not persisted and does not cross the bridge.
    public HashSet<string> SelectedReady { get; } = new();
    public ReleaseQueueInteractionModel Interaction { get; } = new();

    // The paged list itself: ordered stable keys, with the items interned in
    // `_items` by the ingest below. Swapped for a fresh instance only when the
    // library is reset — a tab, filter, order or disclosure change travels in
    // the view request instead, and the same list re-ingests at the same
    // offsets.
    public PaginatedList<BridgeImportListItem, string> List { get; private set; }

    // What the list is currently showing. Every field of it changes which item
    // sits at which offset, so it crosses as one value.
    public BridgeImportListView View { get; private set; }

    /// <summary>Raised when the list is swapped for a fresh instance, so the
    /// sidebar rebinds to the new one.</summary>
    public event Action? ListSwapped;

    /// <summary>Raised when the candidate the pane is holding stops being a
    /// scanned folder, so the pane lets go of it.</summary>
    public event Action? ObservedCandidateGone;

    // Fired whenever anything the sidebar renders changes: a list value, the
    // watched folders, progress, the active tab, the filter text, the sort
    // order, or the selection. The sidebar rebuilds its chrome on every tick
    // and tells the realized rows to render again.
    public event Action? Changed;

    // The live preview-position label ("0:23 / 3:45"), driven by preview events
    // while a slot row auditions. The mapping pane renders it on
    // PreviewElapsedChanged; ClearPreview resets it when the pane moves to
    // another folder.
    public string PreviewElapsedText { get; private set; } = string.Empty;
    public event Action? PreviewElapsedChanged;

    public BridgeLibraryStatus? ReleaseLibraryStatus { get; private set; }
    public event Action? ReleaseLibraryStatusChanged;

    // The file the preview transport is playing, by its absolute path — the
    // only identity the preview events carry. The mapping pane accents the slot
    // row whose audio this is; null when nothing is previewing.
    public string? PreviewingPath { get; private set; }

    // The previewing track's total-duration label, from PreviewPlaying/Paused.
    // Shown after the elapsed position; null when nothing is previewing.
    private string? _previewDurationLabel;

    public ImportStore(
        ImportService import,
        Action<string, string> showError,
        IMediaControl mediaControls,
        Action<Action> dispatch)
    {
        _import = import;
        _showError = showError;
        _mediaControls = mediaControls;
        _dispatch = dispatch;
        SortOrder = ImportSortStore.Load();
        View = BuildView();
        _source = ImportListPageSource.Closed();
        List = BuildList(_source);
    }

    private static BridgeImportQueueSummary EmptySummary => new(
        Counts: new BridgeTriageTabCounts(Pending: 0, Done: 0, Skipped: 0),
        WatchedFolders: Array.Empty<BridgeWatchedFolder>(),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>(),
        GroupKeys: Array.Empty<BridgeFolderReleaseDecisionKey>(),
        Ready: Array.Empty<BridgeReadyRowRef>(),
        FirstUnidentifiedKey: null);

    private BridgeImportListView BuildView() => new(
        Tab: ActiveTab,
        FilterText: FilterText,
        CollapsedGroups: Interaction.CollapsedKeys()
            .Select(key => new BridgeFolderReleaseDecisionKey(key.WatchedRoot, key.RelativePath))
            .ToArray(),
        Order: TriageListModel.ListOrder(SortOrder));

    private ImportListPageSource BuildSource() => new(
        View,
        _import.SubscribeImportList,
        action => _dispatch(action),
        ApplyListSnapshot);

    private PaginatedList<BridgeImportListItem, string> BuildList(ImportListPageSource source) =>
        new(source, StableKey, Ingest, error => Show(error));

    private void Show(Exception error)
    {
        if (error is OperationCanceledException)
        {
            return;
        }
        _showError(
            Loc.Chrome("import.error_title"),
            error is PageLoadException { Line: { } line } ? line : Loc.Chrome("import.failed"));
    }

    internal static string StableKey(BridgeImportListItem item) => item switch
    {
        BridgeImportListItem.GroupHeader header => header.StableKey,
        BridgeImportListItem.Candidate candidate => candidate.StableKey,
        BridgeImportListItem.Boundary boundary => boundary.StableKey,
        BridgeImportListItem.Invalid invalid => invalid.StableKey,
        _ => throw new ArgumentOutOfRangeException(nameof(item), item, "Unknown list item"),
    };

    private void Ingest(IReadOnlyList<BridgeImportListItem> items)
    {
        foreach (var item in items)
        {
            _items[StableKey(item)] = item;
        }
    }

    /// <summary>The item at a stable key, from whichever window loaded it.</summary>
    public BridgeImportListItem? Item(string stableKey) => _items.GetValueOrDefault(stableKey);

    /// <summary>Open the list over the library that is now current, and start
    /// its first read. Called once the handle exists, which is what the cold
    /// read runs against.</summary>
    public void StartList()
    {
        List.Cancel();
        _source.Dispose();
        _items.Clear();
        View = BuildView();
        _source = BuildSource();
        List = BuildList(_source);
        ListSwapped?.Invoke();
        _ = List.LoadInitialAsync();
    }

    private void ApplyView()
    {
        View = BuildView();
        _source.SetView(View);
        Changed?.Invoke();
    }

    public void ObserveReleaseLibraryStatus(
        BridgeMetadataSource source,
        string releaseId,
        string? sourceGroupId)
    {
        _releaseLibraryStatusSubscription?.Dispose();
        var generation = ++_releaseLibraryStatusGeneration;
        ReleaseLibraryStatus = null;
        ReleaseLibraryStatusChanged?.Invoke();
        _releaseLibraryStatusSubscription = _import.SubscribeReleaseLibraryStatus(
            source,
            releaseId,
            sourceGroupId,
            status => _dispatch(() =>
            {
                if (generation != _releaseLibraryStatusGeneration)
                {
                    return;
                }
                ReleaseLibraryStatus = status.ReleaseInLibrary || status.AlbumInLibrary
                    ? status
                    : null;
                ReleaseLibraryStatusChanged?.Invoke();
            }),
            error => _dispatch(() =>
            {
                if (generation == _releaseLibraryStatusGeneration)
                {
                    _showError(Loc.Chrome("error.title"), error.Message);
                }
            }));
    }

    public void ClearReleaseLibraryStatus()
    {
        _releaseLibraryStatusSubscription?.Dispose();
        _releaseLibraryStatusSubscription = null;
        _releaseLibraryStatusGeneration += 1;
        ReleaseLibraryStatus = null;
        ReleaseLibraryStatusChanged?.Invoke();
    }

#if DEBUG
    /// <summary>Seed one window of items and their summary without a bridge,
    /// for the shot-capture scenes and the view tests.</summary>
    internal void SeedPreview(
        IReadOnlyList<BridgeImportListItem> items,
        BridgeImportQueueSummary summary,
        BridgeTriageTab activeTab,
        IEnumerable<ImportCandidate>? candidates = null)
    {
        ActiveTab = activeTab;
        View = BuildView();
        Summary = summary;
        WatchedFolders = summary.WatchedFolders.ToList();
        Ingest(items);
        List.PreloadForPreview(items.Select(StableKey).ToList());
        if (candidates is not null)
        {
            foreach (var candidate in candidates)
            {
                _candidates[candidate.Key] = candidate;
            }
        }
        ListSwapped?.Invoke();
        Changed?.Invoke();
    }
#endif

    /// <summary>One read of the list: its chrome, and the watched folders the
    /// list is built from. The items themselves reach the store through the
    /// page source's ingest.</summary>
    private void ApplyListSnapshot(BridgeImportListSnapshot snapshot)
    {
        Summary = snapshot.Summary;
        WatchedFolders = snapshot.Summary.WatchedFolders.ToList();
        Interaction.RetainGroupDisclosureKeys(
            Summary.GroupKeys.Select(GroupDisclosureKey));

        // Selection can outlive the rows that earned it (a row imported by a
        // faster sibling call, or reclassified out of Ready) — drop keys no
        // longer in Ready rather than let a bulk import act on a stale one.
        var currentReady = Summary.Ready
            .Select(row => row.CandidateKey)
            .ToHashSet();
        SelectedReady.RemoveWhere(key => !currentReady.Contains(key));

        Changed?.Invoke();
    }

    /// <summary>One key's runtime changed. A folder key re-projects its row
    /// (or waits for the row, when the run started before the list re-read);
    /// a re-identify key re-projects its pipeline.</summary>
    public void ApplyCandidateRuntime(BridgeCandidateRuntimeChange change)
    {
        switch (change)
        {
            case BridgeCandidateRuntimeChange.Updated updated:
                _runtime[updated.Key] = updated.Runtime;
                if (_details.TryGetValue(updated.Key, out var detail))
                {
                    _candidates[updated.Key] = _import.ProjectFolderCandidate(detail, updated.Runtime);
                }
                else if (updated.Key.StartsWith("reidentify:", StringComparison.Ordinal))
                {
                    _runtimeCandidates[updated.Key] = _import.ProjectRuntimeCandidate(updated.Runtime);
                }
                break;
            case BridgeCandidateRuntimeChange.Removed removed:
                _runtime.Remove(removed.Key);
                _runtimeCandidates.Remove(removed.Key);
                if (_details.TryGetValue(removed.Key, out var kept))
                {
                    _candidates[removed.Key] = _import.ProjectFolderCandidate(kept, null);
                }
                break;
            default:
                throw new ArgumentOutOfRangeException(nameof(change), change, "Unknown runtime change");
        }
        Changed?.Invoke();
    }

    /// <summary>Put one candidate under its own query: the pane reads its
    /// folder, its files and its resumed identify state from there, and a value
    /// of null says the key names no scanned folder any more.</summary>
    public void ObserveCandidate(string key)
    {
        if (_observedKey == key)
        {
            return;
        }
        ClearObservedCandidate();
        _observedKey = key;
        _observedCandidate = _import.SubscribeImportCandidate(
            key,
            detail => _dispatch(() => ApplyCandidateDetail(key, detail)),
            error => _dispatch(() => Show(error)));
    }

    public void ClearObservedCandidate()
    {
        _observedCandidate?.Dispose();
        _observedCandidate = null;
        if (_observedKey is { } key)
        {
            _details.Remove(key);
            _candidates.Remove(key);
        }
        _observedKey = null;
    }

    private void ApplyCandidateDetail(string key, BridgeImportCandidateDetail? detail)
    {
        if (_observedKey != key)
        {
            return;
        }
        if (detail is null)
        {
            _details.Remove(key);
            _candidates.Remove(key);
            ObservedCandidateGone?.Invoke();
            Changed?.Invoke();
            return;
        }
        _details[key] = detail;
        _candidates[key] = _import.ProjectFolderCandidate(detail, _runtime.GetValueOrDefault(key));
        Changed?.Invoke();
    }

    /// <summary>The candidate at a key, when its own query is open for it.</summary>
    public ImportCandidate? Candidate(string key) =>
        _candidates.GetValueOrDefault(key);

    /// <summary>The row at a candidate key: the pane's own read where one is
    /// open, and the loaded window's item otherwise.</summary>
    public BridgeTriageRow? Row(string key)
    {
        if (_details.TryGetValue(key, out var detail))
        {
            return detail.Row;
        }
        return _items.Values
            .OfType<BridgeImportListItem.Candidate>()
            .FirstOrDefault(item => item.Row.CandidateKey == key)?.Row;
    }

    /// <summary>What a row's runtime is doing, projected: the row says that an
    /// import is running, this says how far. Null for a key nothing has run
    /// on.</summary>
    public ImportCandidateRowStatus? RowStatus(string key) =>
        _runtime.GetValueOrDefault(key) is { } runtime
            ? _import.ProjectRuntimeCandidate(runtime).RowStatus
            : null;

    /// <summary>One candidate read once from its own query, for a bulk import
    /// acting on rows nobody has opened. The query is closed as soon as it has
    /// answered.</summary>
    public Task<ImportCandidate?> ReadCandidate(string key)
    {
        var completion = new TaskCompletionSource<ImportCandidate?>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        IDisposable? subscription = null;
        subscription = _import.SubscribeImportCandidate(
            key,
            detail => _dispatch(() =>
            {
                completion.TrySetResult(
                    detail is null
                        ? null
                        : _import.ProjectFolderCandidate(detail, _runtime.GetValueOrDefault(key)));
                subscription?.Dispose();
            }),
            _ => _dispatch(() =>
            {
                completion.TrySetResult(null);
                subscription?.Dispose();
            }));
        if (subscription is null)
        {
            completion.TrySetResult(null);
        }
        return completion.Task;
    }

    public (
        ImportCandidateRowStatus RowStatus,
        List<ReleaseCandidateChoice> Matches,
        List<SignalBadge> Signals)? ReidentifyPipeline(string key) =>
        _runtimeCandidates.GetValueOrDefault(key);

    // Show a different tab (absolute set — the caller passes the tab its button
    // represents). Which rows the tab holds is core's answer, so the tab
    // travels in the list's view request.
    public void SetActiveTab(BridgeTriageTab tab)
    {
        ActiveTab = tab;
        ApplyView();
    }

    public void SetFilterText(string text)
    {
        FilterText = text;
        ApplyView();
    }

    // Change and persist the candidate-list sort order (absolute set — the
    // caller passes the order its control represents).
    public void SetSortOrder(CandidateSortOrder order)
    {
        SortOrder = order;
        ImportSortStore.Save(order);
        ApplyView();
    }

    // Fold a group open or shut. A folded group's rows are not in the list at
    // all, so this travels in the view request too.
    public void SetGroupExpanded(BridgeFolderReleaseDecisionKey key, bool expanded)
    {
        Interaction.SetGroupExpanded(GroupDisclosureKey(key), expanded);
        ApplyView();
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

    // The identify-progress event updates only the queue-wide header; candidate
    // rows arrive through the triage subscription.
    public void ApplyQueueIdentifyProgress(uint identified, uint total)
    {
        QueueIdentifyProgress = (identified, total);
        Changed?.Invoke();
    }

    // Un-watch a folder: core drops it and its candidates. The candidate and
    // triage subscriptions deliver the resulting state.
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

    // Skip or un-skip a candidate: core persists the change and the triage
    // subscription delivers the candidate in its new tab.
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
        return true;
    }

    // Record the cover this candidate commits with. Returns whether it landed.
    public async Task<bool> SetCandidateCover(string key, BridgeCoverSelection cover) =>
        await Write(() => _import.SetCandidateCover(key, cover));

    // Record one album-level metadata field as the user left it.
    public async Task<bool> SetCandidateEditField(
        string key, BridgeCandidateEditField field, string value) =>
        await Write(() => _import.SetCandidateEditField(key, field, value));

    // Record one mapping-table row as the user left it.
    public async Task<bool> SetCandidateTrackEdit(string key, BridgeRawTrackEdit track) =>
        await Write(() => _import.SetCandidateTrackEdit(key, track));

    // Take one mapping-table row out of the import.
    public async Task<bool> DropCandidateTrack(string key, string trackId) =>
        await Write(() => _import.DropCandidateTrack(key, trackId));

    // Run one write and put its failure on the import banner. Returns whether
    // it landed; a session that moved on lands nothing and says nothing.
    private async Task<bool> Write(Func<Task<(bool Current, string? Error)>> operation)
    {
        var (current, error) = await operation();
        if (!current)
        {
            return false;
        }
        if (error is not null)
        {
            _showError(Loc.Chrome("import.error_title"), error);
            return false;
        }
        return true;
    }

    // Put one of a candidate's files in a role, or put it back. Core persists
    // the decision and clears the stored identify verdict; the folder is a
    // different set of rows afterwards, and the per-candidate read draws them.
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
        return true;
    }

    // Import progress is transient. Preview state and position arrive through
    // the retained playback-values subscription.
    public void HandlePreviewEvent(BridgeUiEvent evt)
    {
        switch (evt)
        {
            case BridgeUiEvent.CandidateImportLoudnessProgress:
                // The sidebar shows import progress as a percent + step off the
                // row's own BridgeCandidateImportStatus (delivered by the
                // candidate subscription); a per-track loudness fraction has no
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

    public void ApplyPreviewValues(BridgePreviewValues values)
    {
        switch (values.State)
        {
            case BridgePreviewState.Playing playing:
                _previewDurationLabel = BridgeDisplay.Clock(playing.DurationMs);
                PreviewingPath = playing.Path;
                _mediaControls.UpdateNowPlayingForPreview(
                    playing.Path, playing.DurationMs, isPlaying: true);
                break;
            case BridgePreviewState.Paused paused:
                _previewDurationLabel = BridgeDisplay.Clock(paused.DurationMs);
                PreviewingPath = paused.Path;
                _mediaControls.UpdateNowPlayingForPreview(
                    paused.Path, paused.DurationMs, isPlaying: false);
                break;
            case BridgePreviewState.Idle:
                _previewDurationLabel = null;
                PreviewingPath = null;
                _mediaControls.UpdatePreviewIdle();
                break;
        }
        PreviewElapsedText = values.State is BridgePreviewState.Idle
            ? string.Empty
            : $"{BridgeDisplay.Clock(values.PositionMs)} / {_previewDurationLabel}";
        PreviewElapsedChanged?.Invoke();
        if (values.State is not BridgePreviewState.Idle)
        {
            _mediaControls.UpdatePreviewPosition(values.PositionMs);
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

    // Scan a folder into the watched set; candidates stream through the value
    // subscription. Returns the error line, or null on success.
    public System.Threading.Tasks.Task<(bool Current, string? Result)> ScanFolder(string path) =>
        _import.ScanFolder(path);

    // Kick off auto-identification for an as-yet unidentified candidate — the
    // click gate for a row whose phase is still Queued.
    public System.Threading.Tasks.Task<bool> AutoIdentify(string candidateKey) =>
        _import.AutoIdentifyFolder(candidateKey);

    // Scan candidates, watched folders, and selection are per-library in-memory
    // state; clear them on teardown so the next library doesn't inherit the
    // previous one's list, and reset the tab to Pending. The sort order persists
    // — it's a preference, not library state.
    public void Reset()
    {
        ClearReleaseLibraryStatus();
        ClearObservedCandidate();
        Summary = EmptySummary;
        WatchedFolders = new List<BridgeWatchedFolder>();
        QueueIdentifyProgress = null;
        ActiveTab = BridgeTriageTab.Pending;
        FilterText = string.Empty;
        SelectedReady.Clear();
        _runtime.Clear();
        _runtimeCandidates.Clear();
        Interaction.RetainGroupDisclosureKeys(
            Array.Empty<ReleaseGroupDisclosureKey>());
        List.Cancel();
        _source.Dispose();
        _items.Clear();
        View = BuildView();
        _source = BuildSource();
        List = BuildList(_source);
        ListSwapped?.Invoke();
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

    public void Dispose()
    {
        ClearReleaseLibraryStatus();
        ClearObservedCandidate();
        List.Cancel();
        _source.Dispose();
    }
}
