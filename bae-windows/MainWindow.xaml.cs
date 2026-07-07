using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

/// <summary>
/// The library grid. On launch it discovers the libraries on disk, opens the
/// active one (or the first), and loads the first page of albums (newest first)
/// into the grid. With no library present it offers to create one; with none
/// discoverable (as in CI) the discovery list is empty and the same path runs.
///
/// The handle is held for the window's lifetime (playback, events, and later
/// screens reuse it) and released when the window closes.
/// </summary>
public sealed partial class MainWindow : Window
{
    private const uint PositionUpdateIntervalMs = 250;
    private const ulong FirstPageSize = 500;

    // LabelKey is a chrome key resolved to the localized menu label at display
    // time; Field is the locale-free sort identifier the generated bridge expects (never
    // localized).
    private sealed record SortOption(string LabelKey, string Field, bool Ascending);
    private sealed record QueueEntryRow(BridgeQueueEntry Entry)
    {
        internal string EntryId => Entry.EntryId;
        public override string ToString() =>
            $"{Entry.Title} — {Entry.ArtistNames} · {Loc.Duration(Entry.DurationMs)}".Trim();
    }

    private enum BrowserMode
    {
        Albums,
        Composers,
    }

    private static readonly SortOption[] AlbumSortOptions =
    {
        new("sort.newest", "date_added", false),
        new("sort.title", "title", true),
        new("sort.artist", "artist", true),
        new("sort.year", "year", true),
    };
    private static readonly SortOption[] ComposerSortOptions =
    {
        new("sort.name", "name", true),
        new("search.section.works", "work_count", false),
        new("search.section.releases", "linked_release_count", false),
    };

    private SortOption _albumSort = AlbumSortOptions[0];
    private SortOption _composerSort = ComposerSortOptions[0];
    private BrowserMode _browserMode = BrowserMode.Albums;
    private bool _populatingSortBox;

    private sealed record SeekProjection(
        string? TrackId,
        ulong TargetPositionMs,
        ulong DurationMs,
        double Progress);
    private sealed record PlaybackPositionSnapshot(
        ulong DurationMs,
        ulong PositionMs,
        double Progress);
    private sealed record PlaybackPositionState(
        PlaybackPositionSnapshot Snapshot,
        SeekProjection? Projection);
    private sealed record NowPlayingState(
        string? AlbumId,
        string? TrackId,
        PlaybackPositionState? Position);

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };

    private readonly object _handleGate = new();
    private AppHandle? _handle;
    private int _sessionGeneration;

    // Releases whose unmanage is running right now. Unmanage is a blocking
    // foreground transfer (unlike pin, which enqueues, or upload, which lives in
    // the outbox), so it has no queue snapshot to read — we track it here while
    // RunStorageActionForReleases awaits, letting the storage row offer to cancel
    // it. UI-thread only (added/removed around the await, read when building the
    // menu).
    private readonly HashSet<string> _unmanagingReleases = new();

    // Held for the subscription's lifetime; the generated callback map keeps the
    // managed object rooted while Rust holds the callback handle.
    private NativeBae.EventCallback? _eventCallback;

    // Latest queue snapshot from QueueUpdated; the queue dialog reads it on open.
    // The two lanes are kept separate so the dialog renders them as distinct
    // sections: the manual lane ("Up Next") and the context (the release being
    // played from), or null when nothing plays from a release.
    private List<BridgeQueueEntry> _queueManual = new();
    private BridgePlaybackContext? _queueContext;

    // Holds the +N queue badge visible for ~1.4s after the last add; a fresh add
    // restarts it, replacing the count and resetting the timer.
    private DispatcherTimer? _queueBadgeTimer;

    // Scan candidates, refreshed from core when import invalidations arrive and
    // bound to the import dialog list.
    private readonly ObservableCollection<ImportCandidate> _candidates = new();

    // The import dialog's status line while it is open.
    private TextBlock? _scanStatus;

    // Reloads the storage dialog's outbox panel and storage rows; set while that
    // dialog is open so outbox invalidations refresh them live, null when closed.
    private Action? _refreshOutbox;
    private Action? _refreshDownloads;

    // Re-reads generated bridge settings into the open settings dialog's labels; set while that
    // dialog is open so config invalidations refresh it live, null when closed.
    private Action? _refreshSettings;

    // Reloads the storage dialog's release rows; set while that dialog is open so
    // album/release invalidations refresh each row's state badge and actions, not
    // just the album grid.
    private Action? _refreshStorageRows;

    // Toolbar sync indicator state, refreshed from the sync-status snapshot.
    private bool _syncing;
    private string? _lastSyncTime;
    private string? _syncErrorText;

    // The import-picker's preview position label, set while that picker is open so
    // PreviewProgress updates it; null when closed.
    private TextBlock? _previewElapsed;

    // The previewing track's total-duration label, from PreviewPlaying. Shown after
    // the elapsed position ("0:23 / 3:45"); null when nothing is previewing.
    private string? _previewDurationLabel;

    // The welcome chooser controls (the on-disk library list plus create /
    // restore), shown when no library is open; removed once one is opened.
    private StackPanel? _welcome;

    // True while the user is dragging the seek slider, so progress events don't
    // fight the drag; set the seek on release.
    private bool _userSeeking;

    // Whatever is currently playing, tracked from playback events so "go to now
    // playing" (Ctrl+L) can open that album and reveal the track. Null when
    // nothing is playing.
    private NowPlayingState? _nowPlaying;

    // Suppresses the volume slider's ValueChanged while we set it programmatically
    // (seeding + VolumeChanged events), so it doesn't echo back as a SetVolume.
    private bool _suppressVolume;

    public ObservableCollection<Album> Albums { get; } = new();
    public ObservableCollection<ComposerSummary> Composers { get; } = new();

    public MainWindow()
    {
        InitializeComponent();
        Closed += OnClosed;

        // Bind layout direction to the UI locale: ar/he (and any other RTL
        // culture) lay out right-to-left. The whole tree inherits from the root
        // grid, so this is the single place the app decides direction. macOS gets
        // this from the system; on Windows the app sets it from the culture.
        RootGrid.FlowDirection = CultureInfo.CurrentUICulture.TextInfo.IsRightToLeft
            ? FlowDirection.RightToLeft
            : FlowDirection.LeftToRight;

        BrowserModeBox.Items.Add(Loc.Chrome("library.mode.albums"));
        BrowserModeBox.Items.Add(Loc.Chrome("library.mode.composers"));
        PopulateSortBox();
        BrowserModeBox.SelectedIndex = 0;

        // Seek on release, not on every drag tick. The Slider handles pointer
        // events internally, so register with handledEventsToo.
        NpProgress.AddHandler(UIElement.PointerPressedEvent,
            new PointerEventHandler((_, _) =>
            {
                _userSeeking = true;
                ClearSeekProjection();
            }), true);
        NpProgress.AddHandler(UIElement.PointerReleasedEvent,
            new PointerEventHandler((_, _) =>
            {
                _userSeeking = false;
                var handle = CurrentHandleOrNull();
                if (handle != null)
                {
                    ProjectSeekDrop(NpProgress.Value);
                    NativeBae.SeekByRatio(handle, NpProgress.Value);
                }
            }), true);

        LoadLibrary();
    }

    private void OnSortChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_populatingSortBox)
        {
            return;
        }
        if (SortBox.SelectedIndex < 0)
        {
            return;
        }

        SetActiveSort(ActiveSortOptions[SortBox.SelectedIndex]);

        // Sort drives the full-library view; search results keep their relevance
        // order. Reload only when no search is active and the library is open.
        if (CurrentHandleOrNull() != null && string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private void OnBrowserModeChanged(object sender, SelectionChangedEventArgs e)
    {
        _browserMode = BrowserModeBox.SelectedIndex == 1 ? BrowserMode.Composers : BrowserMode.Albums;
        PopulateSortBox();
        if (CurrentHandleOrNull() != null && string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private SortOption[] ActiveSortOptions =>
        _browserMode == BrowserMode.Composers ? ComposerSortOptions : AlbumSortOptions;

    private SortOption ActiveSort =>
        _browserMode == BrowserMode.Composers ? _composerSort : _albumSort;

    private void SetActiveSort(SortOption sort)
    {
        if (_browserMode == BrowserMode.Composers)
        {
            _composerSort = sort;
        }
        else
        {
            _albumSort = sort;
        }
    }

    private void PopulateSortBox()
    {
        _populatingSortBox = true;
        SortBox.Items.Clear();
        foreach (var option in ActiveSortOptions)
        {
            SortBox.Items.Add(Loc.Chrome(option.LabelKey));
        }
        SortBox.SelectedIndex = Math.Max(0, Array.IndexOf(ActiveSortOptions, ActiveSort));
        _populatingSortBox = false;
    }

    private void ShowAlbumBrowser()
    {
        AlbumGrid.Visibility = Visibility.Visible;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        SearchResultsScroll.Visibility = Visibility.Collapsed;
    }

    private void ShowComposerBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Visible;
        SearchResultsScroll.Visibility = Visibility.Collapsed;
    }

    private void ShowSearchBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        SearchResultsScroll.Visibility = Visibility.Visible;
    }

    private void LoadCurrentBrowserMode()
    {
        switch (_browserMode)
        {
            case BrowserMode.Albums:
                var (current, json) = WithCurrentHandle(
                    handle => NativeBae.AlbumPageJson(
                        handle,
                        0,
                        FirstPageSize,
                        _albumSort.Field,
                        _albumSort.Ascending));
                if (!current)
                {
                    return;
                }
                SetAlbums(json, Loc.Chrome("library.empty"));
                break;
            case BrowserMode.Composers:
                LoadComposers();
                break;
        }
    }

    // Create a new library, reporting failure through the caller's surface (the
    // welcome status line, or the library manager's). Returns the new id, or null
    // on failure. Callers diverge only in how they open it.
    private string? CreateLibraryOrReport(Action<string> reportError)
    {
        try
        {
            return NativeBae.CreateLibrary();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to create library.", exception);
            reportError(exception.Message);
            return null;
        }
    }

    // The libraries discovered on this device. Empty when discovery fails or none
    // exist; callers pick the active one, or list them.
    private List<BridgeLibrary> LoadLibraries()
    {
        try
        {
            return NativeBae.Libraries();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to discover libraries.", exception);
            StatusText.Text = exception.Message;
            return new List<BridgeLibrary>();
        }
    }

    private void LoadLibrary()
    {
        var libraries = LoadLibraries();
        var library = libraries.FirstOrDefault(candidate => candidate.IsActive)
            ?? libraries.FirstOrDefault();
        if (library is null)
        {
            ShowWelcome();
            return;
        }

        OpenLibrary(library.Id);
    }

    private void OpenLibrary(string libraryId)
    {
        var handle = NativeBae.Init(libraryId, PositionUpdateIntervalMs);
        if (handle == null)
        {
            StatusText.Text = Loc.Chrome("library.open_failed");
            return;
        }

        if (!NativeBae.HasEncryptionKey(handle))
        {
            // Encrypted library whose key isn't on this device: the handle works
            // locally but sync is deferred. Free it and prompt for the key rather
            // than show a half-open library; unlocking re-opens with sync online.
            NativeBae.HandleFree(handle);
            _ = ShowUnlock(libraryId);
            return;
        }

        lock (_handleGate)
        {
            _handle = handle;
            _sessionGeneration++;
        }

        // Committed to showing this library: drop the welcome chooser if it's up.
        // Done here (not at the call sites) so a failed open or an unlock detour
        // above leaves the welcome in place rather than stranding the user.
        DismissWelcome();

        LoadCurrentBrowserMode();

        _suppressVolume = true;
        NpVolume.Value = NativeBae.GetVolume(handle);
        _suppressVolume = false;

        RefreshSyncStatus();

        var generation = _sessionGeneration;
        _eventCallback = new NativeBae.EventCallback(evt => OnNativeEvent(generation, evt));
        NativeBae.Subscribe(handle, _eventCallback);
    }

    // Clear the toolbar sync indicator back to blank — no current status to
    // report.
    private void ResetSyncIndicator()
    {
        _syncing = false;
        _lastSyncTime = null;
        _syncErrorText = null;
        UpdateSyncIndicator();
    }

    private void RefreshSyncStatus()
    {
        var (current, result) = WithCurrentHandle(NativeBae.SyncStatus);
        if (!current)
        {
            return;
        }
        if (result.Error is not null)
        {
            BaeDiagnostics.Logger.Error($"Failed to read sync status snapshot: {result.Error}");
            ResetSyncIndicator();
            return;
        }

        var status = result.Status;
        if (status is null)
        {
            BaeDiagnostics.Logger.Error("Failed to read sync status snapshot.");
            ResetSyncIndicator();
            return;
        }

        RenderSyncStatus(status);
    }

    private void RenderSyncStatus(BridgeSyncStatusSnapshot status)
    {
        var syncLine = status.Error is null ? null : NativeBae.ToDiagnosticError(status.Error).LocalizedLine;
        if (syncLine is null)
        {
            SyncBanner.IsOpen = false;
        }
        else
        {
            var reconnect = new Button { Content = Loc.Chrome("sync.reconnect") };
            reconnect.Click += (_, _) => WithCurrentHandle(NativeBae.TriggerSync);
            SyncBanner.Severity = InfoBarSeverity.Error;
            SyncBanner.Title = Loc.Chrome("sync.error_title");
            SyncBanner.Message = syncLine;
            SyncBanner.ActionButton = reconnect;
            SyncBanner.IsOpen = true;
        }

        _syncErrorText = syncLine;
        _syncing = status.Syncing;
        _lastSyncTime = FormatSyncTime(status.LastSyncTime);
        UpdateSyncIndicator();
    }

    // A locked library (encrypted, key absent on this device): prompt for the
    // 64-character hex key. unlock_library stores it in the credential store; a
    // successful unlock re-opens the library with sync online. The dialog stays
    // open on a bad key; cancelling leaves the library locked.
    private async System.Threading.Tasks.Task ShowUnlock(string libraryId)
    {
        var keyBox = new TextBox { PlaceholderText = Loc.Chrome("library.unlock.key_placeholder"), Width = 360 };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("library.unlock.body"),
            TextWrapping = TextWrapping.Wrap,
        });
        content.Children.Add(keyBox);
        content.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("library.unlock.title"),
            Content = content,
            PrimaryButtonText = Loc.Chrome("library.unlock.confirm"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var key = keyBox.Text?.Trim() ?? string.Empty;
            var error = await System.Threading.Tasks.Task.Run(() => NativeBae.UnlockLibrary(libraryId, key));
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
        {
            OpenLibrary(libraryId);
        }
        else
        {
            StatusText.Text = Loc.Chrome("library.locked");
        }
    }

    // Switch the active library: persist the current one's playback state, tear
    // down its handle and view state, then open the target. generated bridge init writes the
    // target as the new active library; a locked target lands on the unlock
    // prompt. Used for switching to an existing library and for a freshly
    // created one.
    private async System.Threading.Tasks.Task SwitchLibrary(string libraryId)
    {
        await TearDownLibrary();
        OpenLibrary(libraryId);
    }

    // Tear down the open library: shut down and free its handle, and reset every
    // piece of per-library view state so nothing from it bleeds into the next
    // library or the welcome chooser. Leaves the window with no library open.
    private async System.Threading.Tasks.Task TearDownLibrary()
    {
        await ShutdownAndFreeCurrentHandle();

        _eventCallback = null;
        _queueManual = new List<BridgeQueueEntry>();
        _queueContext = null;
        _userSeeking = false;
        _nowPlaying = null;
        // Scan candidates are per-library in-memory state; clear them on teardown
        // so the next library doesn't inherit the previous one's candidate list.
        _candidates.Clear();
        Albums.Clear();
        SearchBox.Text = string.Empty;
        StatusText.Text = string.Empty;
        NowPlayingBar.Visibility = Visibility.Collapsed;
        // The banners report the old library's sync / playback errors; clear them
        // so they don't describe state the next library (or none) doesn't have.
        Banner.IsOpen = false;
        Banner.ActionButton = null;
        SyncBanner.IsOpen = false;
        ResetSyncIndicator();
    }

    // Close the open library and return to the welcome chooser, which now lists
    // the libraries on disk so the user can reopen one or create another.
    private async System.Threading.Tasks.Task CloseLibrary()
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        await TearDownLibrary();
        ShowWelcome();
    }

    private async System.Threading.Tasks.Task ShutdownAndFreeCurrentHandle()
    {
        AppHandle handle;
        lock (_handleGate)
        {
            if (_handle == null)
            {
                return;
            }

            handle = _handle;
            _handle = null;
            _sessionGeneration++;
        }

        await System.Threading.Tasks.Task.Run(() =>
        {
            lock (_handleGate)
            {
                NativeBae.Shutdown(handle);
                NativeBae.HandleFree(handle);
            }
        });
    }

    private async System.Threading.Tasks.Task<(bool Current, T Result)>
        RunForCurrentHandle<T>(Func<AppHandle, T> action)
    {
        var response = await System.Threading.Tasks.Task.Run(() =>
        {
            lock (_handleGate)
            {
                if (_handle == null)
                {
                    return (Ran: false, Generation: _sessionGeneration, Result: default!);
                }

                var generation = _sessionGeneration;
                var result = action(_handle);
                return (Ran: true, Generation: generation, Result: result);
            }
        });
        return (
            response.Ran && response.Generation == _sessionGeneration,
            response.Result);
    }

    private async System.Threading.Tasks.Task<bool> RunForCurrentHandle(Action<AppHandle> action)
    {
        var (current, _) = await RunForCurrentHandle(handle =>
        {
            action(handle);
            return true;
        });
        return current;
    }

    private bool WithCurrentHandle(Action<AppHandle> action)
    {
        lock (_handleGate)
        {
            if (_handle == null)
            {
                return false;
            }

            action(_handle);
            return true;
        }
    }

    private (bool Current, T Result) WithCurrentHandle<T>(Func<AppHandle, T> action)
    {
        lock (_handleGate)
        {
            if (_handle == null)
            {
                return (false, default!);
            }

            return (true, action(_handle));
        }
    }

    private AppHandle? CurrentHandleOrNull()
    {
        lock (_handleGate)
        {
            return _handle;
        }
    }

    private async void OnCloseLibraryClick(object sender, RoutedEventArgs e)
    {
        await CloseLibrary();
    }

    // The library manager: switch between the libraries on this device, or add a
    // new one. Restore-from-code lives only in the first-run flow. Once a library
    // is open the first-run flow never shows, so this is the only way to reach
    // other libraries or create another.
    private void OnShuffleLibraryClick(object sender, RoutedEventArgs e)
    {
        WithCurrentHandle(NativeBae.PlayLibraryShuffled);
    }

    private async void OnLibrariesClick(object sender, RoutedEventArgs e)
    {
        var libraries = LoadLibraries();

        var list = new StackPanel { Spacing = 4, MinWidth = 360 };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        list.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("libraries.title"),
            Content = new ScrollViewer { Content = list, MaxHeight = 420 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };

        foreach (var library in libraries)
        {
            var id = library.Id;
            var isActive = library.IsActive;

            var row = new Grid { ColumnSpacing = 4 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            // Click the name to switch to that library; the active one can't switch
            // to itself but can still be renamed.
            var switchButton = new Button
            {
                Content = isActive
                    ? Loc.Chrome("libraries.active", "name", library.Name)
                    : library.Name,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                IsEnabled = !isActive,
            };
            switchButton.Click += async (_, _) =>
            {
                dialog.Hide();
                await SwitchLibrary(id);
            };
            Grid.SetColumn(switchButton, 0);
            row.Children.Add(switchButton);

            // Rename via a flyout editor (a nested ContentDialog can't open over this
            // one). Saving updates the row label in place; no list rebuild needed.
            var nameBox = new TextBox { Text = library.Name, MinWidth = 220 };
            var renameStatus = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            var saveName = new Button { Content = Loc.Chrome("action.save") };
            var renameContent = new StackPanel { Spacing = 6 };
            renameContent.Children.Add(nameBox);
            renameContent.Children.Add(saveName);
            renameContent.Children.Add(renameStatus);
            var renameFlyout = new Flyout { Content = renameContent };
            saveName.Click += (_, _) =>
            {
                var newName = nameBox.Text?.Trim() ?? string.Empty;
                if (string.IsNullOrEmpty(newName))
                {
                    return;
                }

                var (current, error) = WithCurrentHandle(
                    handle => NativeBae.RenameLibrary(handle, id, newName));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    renameStatus.Text = error;
                    renameStatus.Visibility = Visibility.Visible;
                    return;
                }

                switchButton.Content = isActive
                    ? Loc.Chrome("libraries.active", "name", newName)
                    : newName;
                // Keep the editor's value in sync so reopening it shows the saved
                // name, not the stale snapshot it was seeded with.
                nameBox.Text = newName;
                renameFlyout.Hide();
            };
            var renameButton = new Button { Content = Loc.Chrome("libraries.rename"), Flyout = renameFlyout };
            Grid.SetColumn(renameButton, 1);
            row.Children.Add(renameButton);

            list.Children.Add(row);
        }

        var newButton = new Button
        {
            Content = Loc.Chrome("libraries.new"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        newButton.Click += async (_, _) =>
        {
            var newId = CreateLibraryOrReport(message =>
            {
                status.Text = message;
                status.Visibility = Visibility.Visible;
            });
            if (newId is null)
            {
                return;
            }

            dialog.Hide();
            await SwitchLibrary(newId);
        };
        list.Children.Add(newButton);

        await dialog.ShowAsync();
    }

    // The welcome chooser, shown before any library is open: on first run (no
    // library on disk) and after closing one. Lists the libraries already on
    // disk to reopen, and offers to create a new one or restore from a code or
    // the cloud directly. Creating writes the new library's keys (Windows
    // Credential Manager) and on-disk layout; restoring pulls an existing library
    // from the cloud onto this device.
    private void ShowWelcome()
    {
        // Re-entrant safety: drop any welcome panel from a previous showing so we
        // don't stack two.
        DismissWelcome();

        var libraries = LoadLibraries();
        StatusText.Text = libraries.Count > 0
            ? Loc.Chrome("welcome.choose_library")
            : Loc.Chrome("welcome.no_library");

        _welcome = new StackPanel { Spacing = 8, HorizontalAlignment = HorizontalAlignment.Center };

        if (libraries.Count > 0)
        {
            _welcome.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("welcome.your_libraries"),
                HorizontalAlignment = HorizontalAlignment.Center,
            });
            foreach (var library in libraries)
            {
                var id = library.Id;
                var openButton = new Button
                {
                    Content = library.Name,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                };
                openButton.Click += (_, _) => OpenLibrary(id);
                _welcome.Children.Add(openButton);
            }
        }

        var createButton = new Button
        {
            Content = Loc.Chrome("welcome.create_library"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        createButton.Click += (_, _) =>
        {
            var libraryId = CreateLibraryOrReport(message => StatusText.Text = message);
            if (libraryId is null)
            {
                return;
            }

            OpenLibrary(libraryId);
        };

        var codeBox = new TextBox { PlaceholderText = Loc.Chrome("restore.code_placeholder"), Width = 320 };
        var restoreButton = new Button
        {
            Content = Loc.Chrome("restore.from_code"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        restoreButton.Click += async (_, _) => await RestoreFromCode(codeBox.Text ?? string.Empty);

        // Join a library that already exists on another device: this device shows
        // its join-request code, an existing owner approves it, and the invite
        // code it returns brings the library down here.
        var joinButton = new Button
        {
            Content = Loc.Chrome("welcome.join_library"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        joinButton.Click += async (_, _) => await ShowJoinLibrary();

        // Restore by entering the cloud location and credentials directly, when
        // there's no restore code (the code can't carry secrets like S3 keys).
        var restoreCloudButton = new Button
        {
            Content = Loc.Chrome("restore.from_cloud"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        restoreCloudButton.Click += async (_, _) => await ShowRestoreFromCloud();

        _welcome.Children.Add(createButton);
        _welcome.Children.Add(codeBox);
        _welcome.Children.Add(restoreButton);
        _welcome.Children.Add(joinButton);
        _welcome.Children.Add(restoreCloudButton);
        EmptyState.Children.Add(_welcome);
    }

    private void DismissWelcome()
    {
        if (_welcome is not null)
        {
            EmptyState.Children.Remove(_welcome);
            _welcome = null;
        }
    }

    private async System.Threading.Tasks.Task RestoreFromCode(string code)
    {
        if (string.IsNullOrWhiteSpace(code))
        {
            return;
        }

        BridgeRestoreCodeInfo info;
        try
        {
            info = NativeBae.DecodeRestoreCode(code);
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to decode restore code.", exception);
            StatusText.Text = Loc.Chrome("restore.invalid_code");
            return;
        }

        // OAuth providers (Google Drive, Dropbox, OneDrive) need a sign-in first: the
        // core opens the browser and captures the 127.0.0.1 redirect, returning a
        // token JSON that the restore pull authenticates with. Credential providers
        // pass no token.
        string? oauthTokenJson = null;
        if (info.NeedsOauth)
        {
            // The provider list is the build's support boundary: an S3-only
            // build cannot restore an OAuth-provider code here.
            if (!NativeBae.IsCloudProviderAvailable(info.CloudProvider))
            {
                StatusText.Text = Loc.Chrome("cloud.unsupported_provider", "provider", ProviderDisplayName(info.CloudProvider));
                return;
            }
            if (!OAuthCreds.Available)
            {
                StatusText.Text = OAuthCreds.RegistrationError
                    ?? Loc.Chrome("cloud.signin.not_configured");
                return;
            }
            StatusText.Text = Loc.Chrome("cloud.signin.in_progress", "provider", ProviderDisplayName(info.CloudProvider));
            try
            {
                oauthTokenJson = await System.Threading.Tasks.Task.Run(() => NativeBae.OAuthAuthorize(info.CloudProvider));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to authorize cloud provider for restore.", exception);
                StatusText.Text = Loc.Chrome("cloud.signin.failed");
                return;
            }
        }

        StatusText.Text = Loc.Chrome("restore.in_progress_named", "name", info.LibraryName);
        string libraryId;
        try
        {
            libraryId = await System.Threading.Tasks.Task.Run(() => NativeBae.RestoreFromCode(code, oauthTokenJson));
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to restore library from code.", exception);
            StatusText.Text = Loc.Chrome("restore.failed");
            return;
        }

        DismissWelcome();
        OpenLibrary(libraryId);
    }

    // Copy text to the system clipboard (a shared code the user hands to another
    // device).
    private static void CopyToClipboard(string text)
    {
        var package = new DataPackage();
        package.SetText(text);
        Clipboard.SetContent(package);
    }

    // A code-display block: the QR image (when it renders), the code as selectable
    // monospaced text, and a Copy button. Shared by the join screen (this device's
    // code), the approve flow (the invite code), and the recovery reveal.
    private static StackPanel BuildCodeDisplay(string code)
    {
        var panel = new StackPanel { Spacing = 8, HorizontalAlignment = HorizontalAlignment.Center };

        var qr = QrCode.Image(code);
        if (qr is not null)
        {
            panel.Children.Add(new Image
            {
                Source = qr,
                Width = 180,
                Height = 180,
                Stretch = Stretch.Uniform,
            });
        }

        panel.Children.Add(new TextBox
        {
            Text = code,
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        });

        var copy = new Button
        {
            Content = Loc.Chrome("action.copy"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        copy.Click += (_, _) => CopyToClipboard(code);
        panel.Children.Add(copy);

        return panel;
    }

    // Join a library that already lives on another device. This device generates
    // its join-request code (its public key) and shows it as a QR + text + short
    // fingerprint; an existing owner approves it (in their Settings → Devices) and
    // reads back an invite code, which the user pastes or scans here. Decoding the
    // invite runs the OAuth sign-in when the provider needs it, then JoinFromCode
    // pulls the library down. Mirrors RestoreFromCode's non-cancellable flow.
    private async System.Threading.Tasks.Task ShowJoinLibrary()
    {
        var content = new StackPanel { Spacing = 12, MinWidth = 360 };
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.intro"),
            TextWrapping = TextWrapping.Wrap,
        });

        // This device's code section, filled once it's generated below.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.this_device_code"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var deviceCodeHost = new StackPanel { Spacing = 8 };
        deviceCodeHost.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        content.Children.Add(deviceCodeHost);
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.show_to_member"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });

        // Invite-code section.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.invite_label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.invite_hint"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        var inviteBox = new TextBox
        {
            PlaceholderText = Loc.Chrome("join.invite_placeholder"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var scanButton = new Button { Content = Loc.Chrome("action.scan") };
        var inviteRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        inviteRow.Children.Add(inviteBox);
        inviteRow.Children.Add(scanButton);
        content.Children.Add(inviteRow);

        var invitePreview = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(invitePreview);

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(status);

        var joinButton = new Button
        {
            Content = Loc.Chrome("join.confirm"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            IsEnabled = false,
        };
        content.Children.Add(joinButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("join.title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 560 },
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };

        // The decoded invite and the OAuth token (when the provider needed one):
        // both feed the Join click and gate the button.
        BridgeInviteCodeInfo? decoded = null;
        string? oauthTokenJson = null;

        void ShowStatus(string message)
        {
            status.Text = message;
            status.Visibility = Visibility.Visible;
        }

        void ClearStatus()
        {
            status.Text = string.Empty;
            status.Visibility = Visibility.Collapsed;
        }

        // The button is ready once a valid invite is decoded and — for an OAuth
        // provider — the sign-in has produced a token.
        void Revalidate()
        {
            joinButton.IsEnabled = decoded is not null && (!decoded.NeedsOauth || oauthTokenJson is not null);
        }

        // Decode the typed/scanned invite and preview the library it joins. The
        // decode is a fast in-memory parse, so it runs on the UI thread; OAuth
        // sign-in is deferred to the Join click so the user isn't sent to the
        // browser just for typing.
        void DecodeInvite(string code)
        {
            decoded = null;
            oauthTokenJson = null;
            invitePreview.Visibility = Visibility.Collapsed;
            ClearStatus();
            Revalidate();
            if (string.IsNullOrWhiteSpace(code))
            {
                return;
            }

            BridgeInviteCodeInfo info;
            try
            {
                info = NativeBae.DecodeInviteCode(code);
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to decode invite code.", exception);
                ShowStatus(Loc.Chrome("join.invalid_invite"));
                return;
            }

            decoded = info;
            invitePreview.Text = Loc.Chrome("join.invite_for", new Dictionary<string, object?>
            {
                ["name"] = info.LibraryName,
                ["provider"] = ProviderDisplayName(info.CloudProvider),
                ["fingerprint"] = info.OwnerFingerprint,
            });
            invitePreview.Visibility = Visibility.Visible;
            Revalidate();
        }

        inviteBox.TextChanged += (_, _) => DecodeInvite(inviteBox.Text?.Trim() ?? string.Empty);
        scanButton.Click += async (_, _) =>
        {
            var scanned = await QrScanner.ScanFromFileAsync(WinRT.Interop.WindowNative.GetWindowHandle(this));
            if (scanned is not null)
            {
                inviteBox.Text = scanned.Trim();
            }
        };

        joinButton.Click += async (_, _) =>
        {
            if (decoded is null)
            {
                return;
            }

            var info = decoded;
            ClearStatus();

            // OAuth providers (Google Drive, Dropbox, OneDrive) need a sign-in
            // first: the joining device authorizes its own cloud account, exactly
            // as RestoreFromCode does.
            if (info.NeedsOauth)
            {
                if (!NativeBae.IsCloudProviderAvailable(info.CloudProvider))
                {
                    ShowStatus(Loc.Chrome("cloud.unsupported_provider", "provider", ProviderDisplayName(info.CloudProvider)));
                    return;
                }
                if (!OAuthCreds.Available)
                {
                    ShowStatus(OAuthCreds.RegistrationError ?? Loc.Chrome("cloud.signin.not_configured"));
                    return;
                }

                joinButton.IsEnabled = false;
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                ShowStatus(Loc.Chrome("cloud.signin.in_progress", "provider", ProviderDisplayName(info.CloudProvider)));
                try
                {
                    oauthTokenJson = await System.Threading.Tasks.Task.Run(() => NativeBae.OAuthAuthorize(info.CloudProvider));
                }
                catch (BridgeException exception)
                {
                    BaeDiagnostics.Logger.Error("Failed to authorize cloud provider for join.", exception);
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    ShowStatus(Loc.Chrome("cloud.signin.failed"));
                    Revalidate();
                    return;
                }
            }

            joinButton.IsEnabled = false;
            status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            ShowStatus(Loc.Chrome("join.in_progress", "name", info.LibraryName));
            var code = inviteBox.Text?.Trim() ?? string.Empty;
            var token = oauthTokenJson;
            string libraryId;
            try
            {
                libraryId = await System.Threading.Tasks.Task.Run(() => NativeBae.JoinFromCode(code, token));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to join library from invite.", exception);
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                ShowStatus(Loc.Chrome("join.failed"));
                joinButton.IsEnabled = true;
                return;
            }

            dialog.Hide();
            DismissWelcome();
            OpenLibrary(libraryId);
        };

        // Generate this device's join-request code off the UI thread, then render
        // it (QR + text + Copy) and its fingerprint. A failure leaves the device
        // section showing only an error — the invite half is unaffected.
        _ = GenerateJoinCode(deviceCodeHost);

        await dialog.ShowAsync();
    }

    // Fill the join screen's device-code section: generate the join-request code
    // and its fingerprint, then render the code display. Runs the blocking generated bridge off
    // the UI thread.
    private async System.Threading.Tasks.Task GenerateJoinCode(StackPanel host)
    {
        BridgeJoinRequest request;
        try
        {
            request = await System.Threading.Tasks.Task.Run(() => NativeBae.GenerateJoinRequest());
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to generate join request.", exception);
            host.Children.Clear();
            host.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("join.generate_failed"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        host.Children.Clear();

        host.Children.Add(BuildCodeDisplay(request.Code));

        // The same short form the approving device sees, so the user can confirm
        // they're pairing the right device.
        host.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("join.fingerprint", "fingerprint", request.Fingerprint),
            FontFamily = new FontFamily("Consolas"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
    }

    // Owner-side approve flow: a single dialog whose body swaps between steps —
    // capture (scan or paste the new device's join-request code) → confirm (its
    // fingerprint) → invited (the invite code to enter on the new device). Approve
    // wraps the library key to the device and signs a membership entry; the invite
    // code it returns is the new device's way in. Mirrors macOS's ApproveDeviceSheet.
    private async System.Threading.Tasks.Task ShowApproveDevice()
    {
        var body = new StackPanel { Spacing = 12, MinWidth = 360 };
        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("members.approve.title"),
            Content = new ScrollViewer { Content = body, MaxHeight = 560 },
            CloseButtonText = Loc.Chrome("action.done"),
            XamlRoot = Content.XamlRoot,
        };

        void ShowCapture()
        {
            body.Children.Clear();
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.capture_hint"),
                TextWrapping = TextWrapping.Wrap,
            });

            var pasteBox = new TextBox
            {
                PlaceholderText = Loc.Chrome("members.approve.paste_placeholder"),
                HorizontalAlignment = HorizontalAlignment.Stretch,
            };
            var decode = new Button { Content = Loc.Chrome("members.approve.decode") };
            var scan = new Button { Content = Loc.Chrome("action.scan") };
            var captureRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            captureRow.Children.Add(pasteBox);
            captureRow.Children.Add(decode);
            captureRow.Children.Add(scan);
            body.Children.Add(captureRow);

            var error = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            body.Children.Add(error);

            void TryDecode(string code)
            {
                if (string.IsNullOrWhiteSpace(code))
                {
                    return;
                }

                BridgeJoinRequestInfo info;
                try
                {
                    info = NativeBae.DecodeJoinRequest(code);
                }
                catch (BridgeException exception)
                {
                    BaeDiagnostics.Logger.Error("Failed to decode join request.", exception);
                    error.Text = Loc.Chrome("members.approve.invalid_request");
                    error.Visibility = Visibility.Visible;
                    return;
                }

                ShowConfirm(info);
            }

            decode.Click += (_, _) => TryDecode(pasteBox.Text?.Trim() ?? string.Empty);
            scan.Click += async (_, _) =>
            {
                var scanned = await QrScanner.ScanFromFileAsync(WinRT.Interop.WindowNative.GetWindowHandle(this));
                if (scanned is not null)
                {
                    TryDecode(scanned.Trim());
                }
            };
        }

        void ShowConfirm(BridgeJoinRequestInfo info)
        {
            body.Children.Clear();
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.confirm_title"),
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            });
            body.Children.Add(new TextBlock
            {
                Text = info.Fingerprint,
                FontFamily = new FontFamily("Consolas"),
            });
            if (!string.IsNullOrEmpty(info.Email))
            {
                body.Children.Add(new TextBlock
                {
                    Text = info.Email,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                });
            }
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.confirm_hint"),
                TextWrapping = TextWrapping.Wrap,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });

            var error = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };

            var back = new Button { Content = Loc.Chrome("action.back") };
            back.Click += (_, _) => ShowCapture();
            var approve = new Button
            {
                Content = Loc.Chrome("members.approve.confirm"),
                Style = Application.Current.Resources["AccentButtonStyle"] as Style,
            };
            approve.Click += async (_, _) =>
            {
                approve.IsEnabled = false;
                back.IsEnabled = false;
                error.Visibility = Visibility.Collapsed;
                error.Text = Loc.Chrome("members.approve.in_progress");
                error.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                error.Visibility = Visibility.Visible;

                var pubkey = info.Pubkey;
                var (current, code) = await RunForCurrentHandle(
                    handle => NativeBae.InviteMember(handle, pubkey));
                if (!current)
                {
                    return;
                }
                if (code is null)
                {
                    error.Text = Loc.Chrome("members.approve.failed");
                    error.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    error.Visibility = Visibility.Visible;
                    approve.IsEnabled = true;
                    back.IsEnabled = true;
                    return;
                }

                ShowInvited(code);
            };

            var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            buttons.Children.Add(back);
            buttons.Children.Add(approve);
            body.Children.Add(buttons);
            body.Children.Add(error);
        }

        void ShowInvited(string code)
        {
            body.Children.Clear();
            body.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.approve.invited_hint"),
                TextWrapping = TextWrapping.Wrap,
            });
            body.Children.Add(BuildCodeDisplay(code));
        }

        ShowCapture();
        await dialog.ShowAsync();
    }

    // Restore a library by entering its cloud location and credentials directly,
    // for the S3 credential provider whose secrets a restore code can't carry.
    // OAuth-backed libraries restore from a code instead, where the browser
    // sign-in supplies the tokens.
    private async System.Threading.Tasks.Task ShowRestoreFromCloud()
    {
        var content = new StackPanel { Spacing = 8, MinWidth = 360 };

        var libraryIdBox = new TextBox { Header = Loc.Chrome("restore.field.library_id") };
        // The encryption key unlocks the whole library — mask it, as macOS does.
        var keyBox = new PasswordBox { Header = Loc.Chrome("restore.field.encryption_key") };
        var nameBox = new TextBox { Header = Loc.Chrome("restore.field.library_name") };
        content.Children.Add(libraryIdBox);
        content.Children.Add(keyBox);
        content.Children.Add(nameBox);

        var providerPicker = new ComboBox
        {
            Header = Loc.Chrome("restore.field.cloud_storage"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        foreach (var wire in new[] { "s3" })
        {
            providerPicker.Items.Add(new ComboBoxItem { Content = ProviderDisplayName(wire), Tag = wire });
        }
        content.Children.Add(providerPicker);

        // S3 fields, shown when S3 is selected.
        var s3Bucket = new TextBox { Header = Loc.Chrome("s3.field.bucket"), Visibility = Visibility.Collapsed };
        var s3Region = new TextBox { Header = Loc.Chrome("s3.field.region"), Visibility = Visibility.Collapsed };
        var s3Endpoint = new TextBox { Header = Loc.Chrome("s3.field.endpoint"), Visibility = Visibility.Collapsed };
        var s3AccessKey = new PasswordBox { Header = Loc.Chrome("s3.field.access_key"), Visibility = Visibility.Collapsed };
        var s3SecretKey = new PasswordBox { Header = Loc.Chrome("s3.field.secret_key"), Visibility = Visibility.Collapsed };
        content.Children.Add(s3Bucket);
        content.Children.Add(s3Region);
        content.Children.Add(s3Endpoint);
        content.Children.Add(s3AccessKey);
        content.Children.Add(s3SecretKey);

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(status);

        var restoreButton = new Button
        {
            Content = Loc.Chrome("restore.confirm"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            IsEnabled = false,
        };
        content.Children.Add(restoreButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("restore.from_cloud_title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 520 },
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };

        string SelectedWire() =>
            (providerPicker.SelectedItem as ComboBoxItem)?.Tag as string ?? string.Empty;

        // Enable restore only when the common fields and the selected provider's
        // required fields are all filled.
        void Revalidate()
        {
            var wire = SelectedWire();
            var common = !string.IsNullOrWhiteSpace(libraryIdBox.Text)
                && !string.IsNullOrWhiteSpace(keyBox.Password);
            var providerReady = wire switch
            {
                "s3" => !string.IsNullOrWhiteSpace(s3Bucket.Text)
                    && !string.IsNullOrWhiteSpace(s3Region.Text)
                    && !string.IsNullOrWhiteSpace(s3AccessKey.Password)
                    && !string.IsNullOrWhiteSpace(s3SecretKey.Password),
                _ => false,
            };
            restoreButton.IsEnabled = common && providerReady;
        }

        providerPicker.SelectionChanged += (_, _) =>
        {
            var s3 = SelectedWire() == "s3";
            s3Bucket.Visibility = s3Region.Visibility = s3Endpoint.Visibility =
                s3AccessKey.Visibility = s3SecretKey.Visibility =
                    s3 ? Visibility.Visible : Visibility.Collapsed;
            Revalidate();
        };
        foreach (var box in new[] { libraryIdBox, s3Bucket, s3Region })
        {
            box.TextChanged += (_, _) => Revalidate();
        }
        foreach (var secret in new[] { keyBox, s3AccessKey, s3SecretKey })
        {
            secret.PasswordChanged += (_, _) => Revalidate();
        }
        // Land on S3 so its fields show immediately; fires the handler above.
        providerPicker.SelectedIndex = 0;

        restoreButton.Click += async (_, _) =>
        {
            restoreButton.IsEnabled = false;
            status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            status.Text = Loc.Chrome("restore.in_progress");
            status.Visibility = Visibility.Visible;

            var libraryId = libraryIdBox.Text?.Trim() ?? string.Empty;
            var key = keyBox.Password?.Trim() ?? string.Empty;
            var name = nameBox.Text?.Trim() ?? string.Empty;
            string restoredLibraryId;
            try
            {
                restoredLibraryId = await System.Threading.Tasks.Task.Run(
                    () => NativeBae.RestoreFromS3(
                        libraryId,
                        key,
                        name,
                        s3Bucket.Text?.Trim() ?? string.Empty,
                        s3Region.Text?.Trim() ?? string.Empty,
                        s3Endpoint.Text?.Trim(),
                        s3AccessKey.Password?.Trim() ?? string.Empty,
                        s3SecretKey.Password ?? string.Empty));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to restore S3 library.", exception);
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                status.Text = Loc.Chrome("restore.failed");
                restoreButton.IsEnabled = true;
                return;
            }

            dialog.Hide();
            DismissWelcome();
            OpenLibrary(restoredLibraryId);
        };

        await dialog.ShowAsync();
    }

    // The wire provider tag (google_drive / dropbox / onedrive / s3) as a name to
    // show the user. Unknown tags pass through unchanged.
    private static string ProviderDisplayName(string provider) => provider switch
    {
        "google_drive" => Loc.Chrome("cloud.provider.google_drive"),
        "dropbox" => Loc.Chrome("cloud.provider.dropbox"),
        "onedrive" => Loc.Chrome("cloud.provider.onedrive"),
        "s3" => Loc.Chrome("cloud.provider.s3"),
        _ => provider,
    };

    private static string ProviderDisplayName(BridgeCloudProvider provider)
    {
        var key = NativeBae.CloudProviderLabelKey(provider);
        if (key is not null)
        {
            return Loc.Core(key);
        }

        return provider switch
        {
            BridgeCloudProvider.GoogleDrive => Loc.Chrome("cloud.provider.google_drive"),
            BridgeCloudProvider.Dropbox => Loc.Chrome("cloud.provider.dropbox"),
            BridgeCloudProvider.OneDrive => Loc.Chrome("cloud.provider.onedrive"),
            BridgeCloudProvider.CloudKit => Loc.Chrome("cloud.provider.icloud"),
            _ => provider.ToString(),
        };
    }

    // Fires on a background thread; hop to the UI thread before touching WinUI.
    private void OnNativeEvent(int generation, BridgeUiEvent evt) =>
        DispatcherQueue.TryEnqueue(() => HandleEvent(generation, NativeBae.ToBaeEvent(evt)));

    // Render the toolbar sync indicator from the accumulated state: an active error
    // wins, then an in-progress pass, then the last successful sync time; blank when
    // there's nothing to report (no sync, or none yet this session).
    private void UpdateSyncIndicator()
    {
        if (_syncErrorText is not null)
        {
            SyncIndicator.Text = Loc.Chrome("sync.error_title");
            SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
        }
        else if (_syncing)
        {
            SyncIndicator.Text = Loc.Chrome("sync.syncing");
            SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
        }
        else if (_lastSyncTime is not null)
        {
            SyncIndicator.Text = Loc.Chrome("sync.synced", "time", _lastSyncTime);
            SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
        }
        else
        {
            SyncIndicator.Text = string.Empty;
        }
    }

    // Format a Unix epoch-millis sync timestamp as a local short time ("2:32 PM"),
    // or null when there's been no sync.
    private static string? FormatSyncTime(long? epochMillis)
    {
        if (epochMillis is not long ms)
        {
            return null;
        }

        return DateTimeOffset.FromUnixTimeMilliseconds(ms).ToLocalTime().ToString("t");
    }

    private void ProjectSeekDrop(double requestedProgress)
    {
        var nowPlaying = _nowPlaying;
        var state = nowPlaying?.Position;
        if (nowPlaying is null || state is null || state.Snapshot.DurationMs == 0)
        {
            BaeDiagnostics.Logger.Warning(
                $"Skipping seek projection for progress {requestedProgress} because playback position is unavailable for track {_nowPlaying?.TrackId ?? "<none>"}.");
            return;
        }

        var position = state.Snapshot;
        var progress = Math.Clamp(requestedProgress, NpProgress.Minimum, NpProgress.Maximum);
        var progressRange = NpProgress.Maximum - NpProgress.Minimum;
        if (progressRange <= 0)
        {
            throw new InvalidOperationException("Seek slider range must be positive.");
        }

        var ratio = (progress - NpProgress.Minimum) / progressRange;
        ratio = Math.Clamp(ratio, 0, 1);
        var targetPositionMs = (ulong)Math.Round(position.DurationMs * ratio);
        if (targetPositionMs > position.DurationMs)
        {
            targetPositionMs = position.DurationMs;
        }

        var projection = new SeekProjection(
            nowPlaying.TrackId,
            targetPositionMs,
            position.DurationMs,
            progress);
        _nowPlaying = nowPlaying with { Position = state with { Projection = projection } };
        RenderSeekPosition(progress, targetPositionMs, position.DurationMs);
    }

    private void RenderSeekPosition(double progress, ulong positionMs, ulong durationMs)
    {
        NpProgress.Value = progress;
        NpElapsed.Text = DurationLabel(positionMs);
        NpRemaining.Text = DurationLabel(RemainingDurationMs(positionMs, durationMs));
    }

    private static ulong RemainingDurationMs(ulong positionMs, ulong durationMs)
    {
        if (positionMs > durationMs)
        {
            throw new InvalidOperationException(
                $"Playback position {positionMs} exceeds duration {durationMs}.");
        }

        return durationMs - positionMs;
    }

    private static string DurationLabel(ulong milliseconds)
    {
        if (milliseconds > long.MaxValue)
        {
            throw new InvalidOperationException($"Duration {milliseconds}ms exceeds supported label range.");
        }

        return Loc.Duration((long)milliseconds);
    }

    private void ClearSeekProjection()
    {
        if (_nowPlaying is { Position: { } state } nowPlaying)
        {
            _nowPlaying = nowPlaying with { Position = state with { Projection = null } };
        }
    }

    private bool PlaybackPositionTargetsCurrentTrack(BaeEvent evt)
    {
        if (evt.TrackId is null)
        {
            BaeDiagnostics.Logger.Warning("Ignoring playback position with no track id.");
            return false;
        }
        if (_nowPlaying?.TrackId is { } currentTrackId && currentTrackId != evt.TrackId)
        {
            BaeDiagnostics.Logger.Warning(
                $"Ignoring playback position for stale track {evt.TrackId}; current track is {currentTrackId}.");
            return false;
        }
        return true;
    }

    private void RenderPlaybackProgress(BaeEvent evt)
    {
        if (!PlaybackPositionTargetsCurrentTrack(evt))
        {
            return;
        }
        var snapshot = new PlaybackPositionSnapshot(
            evt.DurationMs,
            evt.PositionMs,
            evt.Progress);
        var projection = _nowPlaying?.Position?.Projection;

        if (projection is not null)
        {
            if (projection.TrackId == evt.TrackId)
            {
                RenderSeekPosition(
                    projection.Progress,
                    projection.TargetPositionMs,
                    projection.DurationMs);
                return;
            }
        }

        ApplyPlaybackPositionSnapshot(evt, null);
    }

    private void RenderPlaybackSeeked(BaeEvent evt)
    {
        if (!PlaybackPositionTargetsCurrentTrack(evt))
        {
            return;
        }
        ApplyPlaybackPositionSnapshot(evt, null);
    }

    private void ApplyPlaybackPositionSnapshot(BaeEvent evt, SeekProjection? projection)
    {
        var snapshot = new PlaybackPositionSnapshot(
            evt.DurationMs,
            evt.PositionMs,
            evt.Progress);
        _nowPlaying = _nowPlaying is { } nowPlaying
            ? nowPlaying with { Position = new PlaybackPositionState(snapshot, projection) }
            : new NowPlayingState(null, evt.TrackId, new PlaybackPositionState(snapshot, projection));

        if (!_userSeeking)
        {
            NpProgress.Value = evt.Progress;
        }
        NpElapsed.Text = DurationLabel(evt.PositionMs);
        NpRemaining.Text = DurationLabel(RemainingDurationMs(evt.PositionMs, evt.DurationMs));
    }

    private void HandleEvent(int generation, BaeEvent evt)
    {
        if (generation != _sessionGeneration || CurrentHandleOrNull() == null)
        {
            return;
        }

        switch (evt.Type)
        {
            case "PlaybackPlaying":
            case "PlaybackPaused":
                NowPlayingBar.Visibility = Visibility.Visible;
                var positionState = _nowPlaying is { } nowPlaying && nowPlaying.TrackId == evt.TrackId
                    ? nowPlaying.Position
                    : null;
                _nowPlaying = new NowPlayingState(evt.AlbumId, evt.TrackId, positionState);
                NpTitle.Text = evt.TrackTitle ?? string.Empty;
                NpArtist.Text = evt.Artist ?? string.Empty;
                NpPlayPause.Content = evt.Type == "PlaybackPlaying" ? "⏸" : "▶";
                NpCover.Source = CoverImage.LoadImage(CurrentHandleOrNull(), evt.CoverImageId);
                // Audio is flowing: drop the buffering spinner, restore the
                // play/pause control.
                NpLoading.IsActive = false;
                NpLoading.Visibility = Visibility.Collapsed;
                NpPlayPause.Visibility = Visibility.Visible;
                if (evt.Type == "PlaybackPaused")
                {
                    _ = ShowSidePauseDialog(evt.PauseReason);
                }
                break;
            case "PlaybackStopped":
                NowPlayingBar.Visibility = Visibility.Collapsed;
                _userSeeking = false;
                _nowPlaying = null;
                NpLoading.IsActive = false;
                NpLoading.Visibility = Visibility.Collapsed;
                NpPlayPause.Visibility = Visibility.Visible;
                break;
            case "PlaybackProgress":
                RenderPlaybackProgress(evt);
                break;
            case "PlaybackSeeked":
                RenderPlaybackSeeked(evt);
                break;
            case "VolumeChanged":
                _suppressVolume = true;
                NpVolume.Value = evt.Volume;
                _suppressVolume = false;
                break;
            case "MuteChanged":
                NpMute.Content = evt.IsMuted ? "🔇" : "🔊";
                break;
            case "RepeatModeChanged":
                NpRepeat.Content = evt.Mode switch
                {
                    "track" => "🔂",
                    "context" => "🔁",
                    _ => "↻",
                };
                break;
            case "PreviewProgress":
                if (_previewElapsed is not null)
                {
                    var elapsed = evt.ElapsedLabel;
                    _previewElapsed.Text = _previewDurationLabel is null
                        ? elapsed
                        : $"{elapsed} / {_previewDurationLabel}";
                }
                break;
            case "PreviewPlaying":
                // Total duration arrives once when preview starts; the next
                // PreviewProgress tick renders it alongside the elapsed position.
                _previewDurationLabel = evt.DurationLabel;
                break;
            case "PreviewIdle":
                _previewDurationLabel = null;
                if (_previewElapsed is not null)
                {
                    _previewElapsed.Text = string.Empty;
                }
                break;
            case "Invalidated":
                HandleInvalidation(evt.Invalidation);
                break;
            case "PlaybackError":
                Banner.Severity = InfoBarSeverity.Error;
                Banner.Title = Loc.Chrome("error.playback_title");
                // The structured reason resolves its own localized line (the
                // actionable cloud-only cases, or a diagnostic's generic line).
                Banner.Message = evt.Reason?.LocalizedLine ?? Loc.Core("core.error.category.internal");
                Banner.ActionButton = null;
                Banner.IsOpen = true;
                break;
            case "Error":
                Banner.Severity = InfoBarSeverity.Error;
                Banner.Title = Loc.Chrome("error.title");
                Banner.Message = evt.Error?.LocalizedLine ?? Loc.Core("core.error.category.internal");
                Banner.ActionButton = null;
                Banner.IsOpen = true;
                break;
            case "ErrorCleared":
                Banner.IsOpen = false;
                break;
            case "PlaybackLoading":
                // Core is preparing or buffering the track (initial load, or a
                // seek to a position not yet downloaded). Show the bar with a
                // spinner over the transport; the prior track's title/cover stay
                // until PlaybackPlaying lands (on a fresh play they fill then).
                if (evt.TrackId is not null
                    && _nowPlaying is { } loadingState
                    && evt.TrackId != loadingState.TrackId)
                {
                    _nowPlaying = loadingState with { Position = null };
                }
                NowPlayingBar.Visibility = Visibility.Visible;
                NpPlayPause.Visibility = Visibility.Collapsed;
                NpLoading.IsActive = true;
                NpLoading.Visibility = Visibility.Visible;
                break;
            case "QueueUpdated":
                _queueManual = evt.Manual ?? new List<BridgeQueueEntry>();
                _queueContext = evt.Context;
                NpPrev.IsEnabled = evt.HasPrevious;
                NpNext.IsEnabled = evt.HasNext;
                break;
            case "QueueItemsAdded":
                if (evt.Count is int added && added > 0)
                {
                    FlashQueueAddBadge(added);
                }
                break;
            case "CandidateImportLoudnessProgress":
                // The long-pole loudness pass: replace the candidate's status
                // with a live "Measuring loudness — N/M" line per track. The
                // row is replaced in place (UpdateCandidate raises Replace), so
                // the list isn't re-rendered.
                UpdateCandidate(evt.Key, null, Loc.Core(
                    "ui.import.loudness_progress",
                    new Dictionary<string, object?>
                    {
                        ["done"] = evt.TracksDone,
                        ["total"] = evt.TracksTotal,
                    }));
                break;
        }
    }

    private void HandleInvalidation(BaeInvalidation? invalidation)
    {
        if (invalidation is null)
        {
            BaeDiagnostics.Logger.Warning("Ignoring invalidation event with no invalidation payload.");
            return;
        }

        switch (invalidation.Kind)
        {
            case "album_list":
            case "composer_list":
                if (string.IsNullOrEmpty(SearchBox.Text))
                {
                    LoadCurrentBrowserMode();
                }
                break;
            case "album":
            case "release":
                _refreshStorageRows?.Invoke();
                break;
            case "config":
                _refreshSettings?.Invoke();
                break;
            case "sync_status":
                RefreshSyncStatus();
                break;
            case "outbox":
                _refreshOutbox?.Invoke();
                break;
            case "download_queue":
                _refreshDownloads?.Invoke();
                break;
            case "import_candidate_list":
            case "import_candidate":
            case "watched_folders":
                RefreshImportCandidates();
                break;
        }
    }

    private async void RefreshImportCandidates()
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var (current, json) = await RunForCurrentHandle(NativeBae.ImportCandidatesJson);
        if (!current)
        {
            return;
        }
        var candidates = json is null
            ? null
            : JsonSerializer.Deserialize<List<ImportCandidate>>(json, JsonOptions);
        if (candidates is null)
        {
            ShowImportBanner(Loc.Chrome("import.failed"));
            return;
        }

        _candidates.Clear();
        foreach (var candidate in candidates)
        {
            _candidates.Add(candidate);
        }
        if (_scanStatus is not null)
        {
            _scanStatus.Text = _candidates.Count == 0 ? Loc.Chrome("import.no_releases") : string.Empty;
        }
    }

    // Replace a candidate row in place (ObservableCollection raises Replace so the
    // bound list re-renders), applying an optional mutation and a live status.
    private void UpdateCandidate(string? key, Action<ImportCandidate>? mutate, string status)
    {
        var index = IndexOfCandidate(key);
        if (index < 0)
        {
            return;
        }

        var existing = _candidates[index];
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
        _candidates[index] = updated;
    }


    private int IndexOfCandidate(string? key)
    {
        for (var i = 0; i < _candidates.Count; i++)
        {
            if (_candidates[i].Key == key)
            {
                return i;
            }
        }

        return -1;
    }

    private async void OnImportClick(object sender, RoutedEventArgs e)
    {
        await ShowImportDialog();
    }

    // Build and show the import dialog: the folder scan source plus the live
    // candidate list bound to _candidates.
    // Shared by the toolbar import button and the folder-drop handler.
    private async System.Threading.Tasks.Task ShowImportDialog()
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }
        RefreshImportCandidates();

        var scanButton = new Button { Content = Loc.Chrome("import.choose_folder") };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var list = new ListView
        {
            ItemsSource = _candidates,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            MaxHeight = 320,
        };

        // Each row builds imperatively: the candidate's one-line summary plus a
        // signals badge row. ContainerContentChanging fires per recycled container
        // (re-firing when UpdateCandidate swaps in a fresh instance), so the badges
        // refresh as identify events land.
        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue || args.Item is not ImportCandidate candidate)
            {
                return;
            }

            args.ItemContainer.Content = BuildCandidateRow(candidate);
            args.Handled = true;
        };

        // First click on an unidentified candidate kicks off auto-identification;
        // once it has a result, clicking opens the import dialog (auto matches
        // plus a manual-search fallback). The row reflects status from the
        // candidate snapshot.
        list.ItemClick += async (_, args) =>
        {
            if (args.ClickedItem is not ImportCandidate candidate)
            {
                return;
            }

            if (string.IsNullOrEmpty(candidate.Status))
            {
                _ = RunForCurrentHandle(
                    handle => NativeBae.AutoIdentifyFolder(handle, candidate.Key, candidate.FolderPath));
            }
            else
            {
                await ShowImportPicker(candidate);
            }
        };

        // The picker needs the app window's handle in an unpackaged app. Adding
        // a watched folder starts scanning; invalidations refresh _candidates.
        scanButton.Click += async (_, _) =>
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                return;
            }

            status.Text = Loc.Chrome("import.scanning");
            status.Visibility = Visibility.Visible;
            var path = folder.Path;
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.ScanFolder(handle, path, true));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                status.Text = error;
            }
            else
            {
                RefreshImportCandidates();
            }
        };

        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(scanButton);
        content.Children.Add(status);
        content.Children.Add(list);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };

        _scanStatus = status;
        await dialog.ShowAsync();
        _scanStatus = null;
    }

    // Accept a dragged folder anywhere over the window (matching macOS, which
    // imports a folder dropped on its window). DragOver fires continuously, so
    // keep it to the cheap format check; the real work happens in OnWindowDrop.
    private void OnWindowDragOver(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() != null && e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            e.AcceptedOperation = DataPackageOperation.Copy;
            // Null for some shell drags; the caption is just a cursor hint.
            if (e.DragUIOverride is not null)
            {
                e.DragUIOverride.Caption = Loc.Chrome("import.drop_caption");
            }
        }
        else
        {
            e.AcceptedOperation = DataPackageOperation.None;
        }
    }

    // Scan a dropped folder and open the import dialog on its candidates. Mirrors
    // the macOS window drop: the first dropped folder is scanned with clearFirst,
    // candidates stream into _candidates, and the dialog (bound to that list) shows
    // them. Scanning runs off the UI thread; errors surface in the banner.
    private async void OnWindowDrop(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            return;
        }

        string? folderPath = null;
        string? readError = null;
        var deferral = e.GetDeferral();
        try
        {
            var items = await e.DataView.GetStorageItemsAsync();
            // Match macOS: the first dropped item must be a folder.
            if (items.FirstOrDefault() is StorageFolder folder && !string.IsNullOrEmpty(folder.Path))
            {
                folderPath = folder.Path;
            }
        }
        catch (Exception)
        {
            readError = Loc.Chrome("import.drop_read_failed");
        }
        finally
        {
            // Release the drop as soon as its data is read — before scanning or
            // showing the dialog — so the drag source isn't left hanging.
            deferral.Complete();
        }

        if (readError is not null)
        {
            ShowImportBanner(readError);
            return;
        }

        if (folderPath is null)
        {
            ShowImportBanner(Loc.Chrome("import.drop_folder_only"));
            return;
        }

        var (current, error) = await RunForCurrentHandle(
            handle => NativeBae.ScanFolder(handle, folderPath, true));
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            ShowImportBanner(error);
        }

        // Open the import dialog on the streamed candidates — on scan error too,
        // matching macOS, which navigates to import regardless of the scan result.
        // Skip if one is already open (only one ContentDialog can open at a time).
        if (_scanStatus is null)
        {
            await ShowImportDialog();
        }
    }

    private void ShowImportBanner(string message)
    {
        Banner.Severity = InfoBarSeverity.Error;
        Banner.Title = Loc.Chrome("import.error_title");
        Banner.Message = message;
        Banner.ActionButton = null;
        Banner.IsOpen = true;
    }

    // One candidate row: the summary line, then a signals badge row underneath
    // (omitted until the first toolbar event lands). Core pre-shapes every badge;
    // this iterates and renders.
    private StackPanel BuildCandidateRow(ImportCandidate candidate)
    {
        var row = new StackPanel { Spacing = 4 };
        row.Children.Add(new TextBlock
        {
            Text = candidate.ToString(),
            TextWrapping = TextWrapping.Wrap,
        });

        if (candidate.Signals.Count > 0)
        {
            var badges = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                VerticalAlignment = VerticalAlignment.Center,
            };
            foreach (var signal in candidate.Signals)
            {
                badges.Children.Add(BuildSignalBadge(candidate.Key, signal));
            }

            badges.Children.Add(BuildRerunButton(candidate.Key));
            row.Children.Add(badges);
        }

        return row;
    }

    // The trailing re-run control on the signals row: re-dispatches the
    // candidate's lookups (keeping the user's exclusions). The re-derived state
    // arrives through candidate invalidation.
    private Button BuildRerunButton(string candidateKey)
    {
        var button = new Button
        {
            Content = "↻",
            Padding = new Thickness(8, 3, 8, 3),
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(button, Loc.Chrome("import.rerun_identify"));
        button.Click += (_, _) =>
        {
            if (CurrentHandleOrNull() != null)
            {
                _ = RunForCurrentHandle(
                    handle => NativeBae.RerunIdentifyForCandidate(handle, candidateKey));
            }
        };
        return button;
    }

    // Middle-truncate a signal value so both ends stay visible: catalog numbers and
    // barcodes differ at the end, which an end-ellipsis would hide. WinUI has no
    // middle TextTrimming, and the badge value is monospace (Consolas), so a
    // character budget tracks the pixel width closely. Mirrors macOS's
    // .truncationMode(.middle) on the signal value.
    private static string MiddleTruncate(string value, int maxChars)
    {
        if (value.Length <= maxChars)
        {
            return value;
        }

        var keep = maxChars - 1; // one character for the ellipsis
        var head = keep / 2;
        var tail = keep - head;
        return value[..head] + "…" + value[^tail..];
    }

    // One signals badge: a kind label, the value (truncated), and a trailing
    // state visual (spinner / count / dash / warning). Excluded badges dim and
    // strike through but stay in place so the row's layout is stable. Clicking a
    // badge toggles its signal in/out of triangulation (excluded badges re-include
    // on click); the re-derived toolbar arrives through candidate invalidation.
    // Mirrors the macOS SignalBadge anatomy in plain WinUI primitives.
    private Button BuildSignalBadge(string candidateKey, SignalBadge signal)
    {
        var inner = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var label = new TextBlock
        {
            Text = SignalKindLabel(signal.Kind),
            FontSize = 12,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (signal.Excluded)
        {
            label.TextDecorations = global::Windows.UI.Text.TextDecorations.Strikethrough;
        }

        inner.Children.Add(label);

        if (!string.IsNullOrEmpty(signal.Value))
        {
            var value = new TextBlock
            {
                Text = MiddleTruncate(signal.Value, 20),
                FontSize = 11,
                FontFamily = new FontFamily("Consolas"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                MaxWidth = 140,
                // The character budget keeps the value under MaxWidth; CharacterEllipsis
                // is only a backstop if a font measures wider than monospace estimates.
                TextTrimming = TextTrimming.CharacterEllipsis,
                VerticalAlignment = VerticalAlignment.Center,
            };
            if (signal.Excluded)
            {
                value.TextDecorations = global::Windows.UI.Text.TextDecorations.Strikethrough;
            }

            inner.Children.Add(value);
        }

        inner.Children.Add(BuildSignalState(signal));

        // A Button, not a Border+Tapped: the ListView raises ItemClick from its own
        // gesture handling and ignores a child that merely marks Tapped handled, but
        // it honors a pointer-capturing control. So the badge press toggles the
        // signal without also re-triggering auto-identify / opening the import
        // dialog. MinWidth/Height 0 keeps it badge-sized, not the default button box.
        var badge = new Button
        {
            Content = inner,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(8, 3, 8, 3),
            MinWidth = 0,
            MinHeight = 0,
            CornerRadius = new CornerRadius(8),
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.DimGray),
            Margin = new Thickness(0, 0, 6, 0),
            Opacity = signal.Excluded ? 0.45 : 1.0,
        };

        // Clicking toggles this signal's exclusion. Excluded badges stay clickable
        // (to re-include). The catalog kind names a specific candidate by its value;
        // disc_id / barcode are singletons (value ignored core-side).
        ToolTipService.SetToolTip(
            badge,
            signal.Excluded ? Loc.Chrome("signal.include") : Loc.Chrome("signal.exclude"));
        badge.Click += (_, _) =>
        {
            if (CurrentHandleOrNull() != null)
            {
                var kind = signal.Kind;
                var value = signal.Value ?? string.Empty;
                _ = RunForCurrentHandle(
                    handle => NativeBae.ToggleSignalForCandidate(handle, candidateKey, kind, value));
            }
        };
        return badge;
    }

    // The badge's trailing state visual, chosen by the pre-shaped SignalState the
    // generated bridge carried over. An excluded badge shows the exclusion mark regardless.
    private static FrameworkElement BuildSignalState(SignalBadge signal)
    {
        if (signal.Excluded)
        {
            return new TextBlock
            {
                Text = "✕",
                FontSize = 11,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                VerticalAlignment = VerticalAlignment.Center,
            };
        }

        switch (signal.State.Kind)
        {
            case "looking_up":
                return new ProgressRing { IsActive = true, Width = 14, Height = 14 };
            case "found":
                return CountPill((signal.State.Count ?? 0).ToString(), Microsoft.UI.Colors.LightGreen);
            case "confirms":
                return signal.State.Count is > 0
                    ? new TextBlock
                    {
                        Text = "✓",
                        FontSize = 12,
                        FontWeight = Microsoft.UI.Text.FontWeights.Bold,
                        Foreground = new SolidColorBrush(Microsoft.UI.Colors.DeepSkyBlue),
                        VerticalAlignment = VerticalAlignment.Center,
                    }
                    : CountPill("0", Microsoft.UI.Colors.Gray);
            case "no_match":
                return CountPill("0", Microsoft.UI.Colors.Gray);
            case "skipped":
                return new TextBlock
                {
                    Text = "–",
                    FontSize = 12,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    VerticalAlignment = VerticalAlignment.Center,
                };
            case "failed":
                var warning = new TextBlock
                {
                    Text = "⚠",
                    FontSize = 12,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Orange),
                    VerticalAlignment = VerticalAlignment.Center,
                };
                // The structured lookup failure resolves its localized line for
                // the hover tooltip; no prose crosses the bridge.
                if (signal.State.Failure is { } failure)
                {
                    ToolTipService.SetToolTip(warning, failure.LocalizedLine);
                }
                return warning;
            default:
                return new TextBlock { Text = string.Empty };
        }
    }

    // The badge's kind label. Mirrors the macOS SignalBadgeStyle.label(for:);
    // the wire kind names come from the generated bridge's snake_case mapping, resolved to a
    // localized chrome label.
    private static string SignalKindLabel(string kind) => kind switch
    {
        "disc_id" => Loc.Chrome("signal.kind.disc_id"),
        "barcode" => Loc.Chrome("signal.kind.barcode"),
        "catalog" => Loc.Chrome("signal.kind.catalog"),
        _ => kind,
    };

    // A small count pill — a colored digit, the badge's settled-state readout.
    private static Border CountPill(string text, global::Windows.UI.Color color)
    {
        return new Border
        {
            Child = new TextBlock
            {
                Text = text,
                FontSize = 11,
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
                Foreground = new SolidColorBrush(color),
            },
            Padding = new Thickness(6, 1, 6, 1),
            CornerRadius = new CornerRadius(6),
            VerticalAlignment = VerticalAlignment.Center,
        };
    }

    // Import-confirm dialog for a candidate: pick an identity (the auto-identified
    // matches, or one found by manual search when auto-identification came up
    // empty), choose a storage mode, and import.
    private async System.Threading.Tasks.Task ShowImportPicker(ImportCandidate candidate)
    {
        var results = new List<Candidate>(candidate.Matches);
        var resultsList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 240,
        };
        void RenderResults() => resultsList.ItemsSource = results.Select(result => result.Summary).ToList();
        RenderResults();

        var artistBox = new TextBox { Header = Loc.Chrome("import.field.artist_manual") };
        var albumBox = new TextBox { Header = Loc.Chrome("search.field.album"), Text = candidate.Name };
        var sourceBox = new ComboBox { Header = Loc.Chrome("search.field.source") };
        sourceBox.Items.Add("discogs");
        sourceBox.Items.Add("musicbrainz");
        sourceBox.SelectedIndex = 0;
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(new TextBlock { Text = candidate.Name });

        // Preview the candidate's audio before committing to an identity.
        if (candidate.AudioPaths.Count > 0)
        {
            var preview = new Button { Content = "▶ " + Loc.Chrome("import.preview") };
            preview.Click += (_, _) => WithCurrentHandle(
                handle => NativeBae.PreviewPlay(handle, candidate.AudioPaths[0]));
            var pause = new Button { Content = "⏸" };
            pause.Click += (_, _) => WithCurrentHandle(NativeBae.PreviewTogglePause);
            var stop = new Button { Content = "⏹" };
            stop.Click += (_, _) => WithCurrentHandle(NativeBae.PreviewStop);
            // Live preview position, updated by PreviewProgress while the picker is open.
            var previewElapsed = new TextBlock { VerticalAlignment = VerticalAlignment.Center };
            _previewElapsed = previewElapsed;
            var previewRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            previewRow.Children.Add(preview);
            previewRow.Children.Add(pause);
            previewRow.Children.Add(stop);
            previewRow.Children.Add(previewElapsed);
            content.Children.Add(previewRow);
        }

        content.Children.Add(resultsList);
        content.Children.Add(artistBox);
        content.Children.Add(albumBox);
        content.Children.Add(sourceBox);
        content.Children.Add(searchButton);
        content.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.release_title"),
            Content = new ScrollViewer { Content = content },
            PrimaryButtonText = Loc.Chrome("action.import"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
            IsPrimaryButtonEnabled = false,
        };

        searchButton.Click += async (_, _) =>
        {
            var source = (string)sourceBox.SelectedItem;
            var artist = artistBox.Text;
            var album = albumBox.Text;
            searchButton.IsEnabled = false;
            var (current, json) = await RunForCurrentHandle(
                handle => NativeBae.SearchReleasesJson(handle, source, artist, album));
            searchButton.IsEnabled = true;
            if (!current)
            {
                return;
            }
            var parsed = json is null ? null : JsonSerializer.Deserialize<List<Candidate>>(json, JsonOptions);
            if (parsed is null)
            {
                status.Text = Loc.Chrome("search.failed");
                status.Visibility = Visibility.Visible;
                return;
            }

            results = parsed;
            RenderResults();
            dialog.IsPrimaryButtonEnabled = false;
        };

        resultsList.SelectionChanged += (_, _) =>
        {
            dialog.IsPrimaryButtonEnabled = resultsList.SelectedIndex >= 0;
        };

        // Clicking Import here doesn't commit — it advances to the metadata edit
        // step. A second ContentDialog can't open while this one shows, so the
        // picker closes on Primary (the selection + storage mode are captured) and
        // the confirm step opens after ShowAsync returns. The loop re-opens this
        // picker — with its search results and selection intact — when the confirm
        // step's "Back to Search" is chosen; Import or Cancel ends the flow.
        try
        {
            while (true)
            {
                var pickerResult = await dialog.ShowAsync();
                WithCurrentHandle(NativeBae.PreviewStop);
                if (pickerResult != ContentDialogResult.Primary)
                {
                    return;
                }

                var index = resultsList.SelectedIndex;
                if (index < 0 || index >= results.Count)
                {
                    return;
                }

                var backToSearch = await ShowImportConfirm(candidate, results[index]);
                if (!backToSearch)
                {
                    return;
                }
            }
        }
        finally
        {
            _previewElapsed = null;
            _previewDurationLabel = null;
        }
    }

    /// <summary>
    /// A dismiss-only error dialog: a title, an optional detail body, and a single
    /// "OK" button that closes it.
    /// </summary>
    private async System.Threading.Tasks.Task ShowError(string title, string? message = null)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            CloseButtonText = Loc.Chrome("action.ok"),
            XamlRoot = Content.XamlRoot,
        };
        if (message is not null)
        {
            dialog.Content = message;
        }

        await dialog.ShowAsync();
    }

    /// <summary>
    /// The import confirmation step: seed the album/pressing/track edit form from
    /// the chosen candidate release, let the user revise it, then commit the import
    /// with those edits overlaid. Errors stay in-dialog (the window banner is
    /// occluded by the modal). Mirrors macOS's ImportConfirmationView.
    /// </summary>
    // Returns true when the user chose "Back to Search" — the caller re-opens the
    // picker so they can pick or search for a different release.
    private async System.Threading.Tasks.Task<bool> ShowImportConfirm(
        ImportCandidate candidate, Candidate chosen)
    {
        var (current, prefetched) = await RunForCurrentHandle(
            handle => NativeBae.PrefetchCandidateEdit(
                handle, chosen.ReleaseId, chosen.Source, candidate.FolderPath).Prefetched);
        if (!current)
        {
            return false;
        }
        if (prefetched is null)
        {
            await ShowError(Loc.Chrome("import.error.load_release"));
            return false;
        }

        var (settingsCurrent, settingsJson) = await RunForCurrentHandle(NativeBae.SettingsJson);
        if (!settingsCurrent)
        {
            return false;
        }
        if (settingsJson is null)
        {
            await ShowError(Loc.Chrome("import.error.load_storage_settings"));
            return false;
        }
        Settings settings;
        try
        {
            settings = JsonSerializer.Deserialize<Settings>(settingsJson, JsonOptions)
                ?? throw new JsonException("settings payload was empty");
        }
        catch (JsonException ex)
        {
            await ShowError(Loc.Chrome("import.error.read_storage_settings"), ex.Message);
            return false;
        }
        var form = new ReleaseEditForm(prefetched.Edit, 520);

        // The cover the user picks; empty means "let the import choose its
        // default cover" (the source's first cover art, else a folder image).
        var selectedCoverJson = string.Empty;
        var storageRemote = new CheckBox
        {
            Content = Loc.Chrome("import.storage.managed"),
            IsChecked = true,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var storagePinned = new CheckBox
        {
            Content = Loc.Chrome("import.storage.keep_local"),
            IsChecked = true,
            VerticalAlignment = VerticalAlignment.Center,
        };
        bool StorageRemoteSelected() => settings.HasCloudHome && storageRemote.IsChecked == true;
        // Storage state and pinned-ness are orthogonal: the mode tag is purely
        // the remote-vs-local storage choice; the pin choice rides alongside as
        // its own arg, meaningful only for a remote import.
        string StorageModeTag() => StorageRemoteSelected() ? "managed" : "unmanaged";
        bool StoragePinSelected() => StorageRemoteSelected() && storagePinned.IsChecked == true;

        void RefreshStorageControls()
        {
            storagePinned.Visibility = StorageRemoteSelected()
                ? Visibility.Visible
                : Visibility.Collapsed;
        }
        storageRemote.Checked += (_, _) => RefreshStorageControls();
        storageRemote.Unchecked += (_, _) => RefreshStorageControls();
        RefreshStorageControls();

        var panel = new StackPanel { Spacing = 8, MinWidth = 520 };
        if (settings.HasCloudHome)
        {
            var storageRow = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                Spacing = 8,
                HorizontalAlignment = HorizontalAlignment.Right,
                VerticalAlignment = VerticalAlignment.Center,
            };
            storageRow.Children.Add(storageRemote);
            storageRow.Children.Add(storagePinned);
            panel.Children.Add(storageRow);
        }
        panel.Children.Add(BuildCoverPicker(
            prefetched.RemoteCovers, prefetched.LocalArtwork, picked => selectedCoverJson = picked));
        panel.Children.Add(form.Panel);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.confirm_title"),
            Content = new ScrollViewer { Content = panel },
            PrimaryButtonText = Loc.Chrome("action.import"),
            SecondaryButtonText = Loc.Chrome("import.back_to_search"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };

        // Whether the user clicked "view in library" on the already-in-library
        // banner. A nested ContentDialog can't open over this one, so the banner
        // closes the confirm dialog and the album opens after ShowAsync returns
        // (the gallery/edit/re-identify pattern).
        string? viewInLibraryAlbumId = null;

        // Tell the user when the chosen release is already in the library before
        // they import a duplicate. The check is by release identity (the confirm
        // flow has no group id), so it reports the exact pressing — album_in_library
        // tracks release_in_library here. Reads the database — run it off the UI
        // thread. A failure leaves the banner absent; the import still proceeds
        // (the banner is advisory, not a gate).
        var (statusCurrent, libraryStatus) = await RunForCurrentHandle(
            handle => NativeBae.CheckReleaseInLibrary(handle, chosen.ReleaseId).Status);
        if (!statusCurrent)
        {
            return false;
        }
        if (libraryStatus is not null && libraryStatus.ReleaseInLibrary)
        {
            panel.Children.Insert(0, BuildLibraryStatusBanner(libraryStatus, () =>
            {
                viewInLibraryAlbumId = libraryStatus.AlbumId;
                dialog.Hide();
            }));
        }

        // The import runs in the background; its result updates the candidate row
        // and library grid through invalidations.
        // Shape (validation) of the edit happens in Rust — on failure keep the
        // dialog open and show the reason.
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var payload = form.ReadBack();
            var storageMode = StorageModeTag();
            var pin = StoragePinSelected();
            var (importCurrent, error) = await RunForCurrentHandle(
                handle => NativeBae.ImportCandidate(
                    handle, candidate.Key, candidate.FolderPath, chosen.ReleaseId, chosen.Source, storageMode, pin, payload, selectedCoverJson));
            if (!importCurrent)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                form.ErrorText.Text = error;
                form.ErrorText.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        var result = await dialog.ShowAsync();

        // "view in library" closed the dialog above; open that album now that the
        // confirm dialog is gone (a nested ContentDialog can't open over it).
        if (viewInLibraryAlbumId is not null)
        {
            await ShowAlbumDetail(viewInLibraryAlbumId);
            return false;
        }

        // "Back to Search" (Secondary) returns the user to the picker; Import and
        // Cancel both end the flow.
        return result == ContentDialogResult.Secondary;
    }

    /// <summary>
    /// The already-in-library banner shown at the top of the import confirmation
    /// when the chosen release (<see cref="BridgeLibraryStatus.ReleaseInLibrary"/>) is
    /// already in the library. When an album id is present it offers a "view in
    /// library" button that invokes <paramref name="onViewInLibrary"/>.
    /// </summary>
    private static InfoBar BuildLibraryStatusBanner(BridgeLibraryStatus status, Action onViewInLibrary)
    {
        var banner = new InfoBar
        {
            Severity = InfoBarSeverity.Warning,
            IsOpen = true,
            IsClosable = false,
            Message = Loc.Chrome("import.already_in_library"),
        };

        if (!string.IsNullOrEmpty(status.AlbumId))
        {
            var viewButton = new Button { Content = Loc.Chrome("import.view_in_library") };
            viewButton.Click += (_, _) => onViewInLibrary();
            banner.ActionButton = viewButton;
        }

        return banner;
    }

    /// <summary>
    /// The import confirmation's cover-art picker: a gallery of the chosen
    /// release's remote covers (thumbnails by URL) and the candidate folder's
    /// local artwork (thumbnails by disk path). Clicking a tile selects it,
    /// highlights it, and reports its cover-selection JSON via
    /// <paramref name="onPick"/>; nothing is picked by default, so the import
    /// uses its own default cover. Renders inline (not a nested dialog, which
    /// can't open over the confirm dialog).
    /// </summary>
    private static StackPanel BuildCoverPicker(
        List<BridgeRemoteCover> remoteCovers, List<LocalArtwork> localArtwork, Action<string> onPick)
    {
        var section = new StackPanel { Spacing = 4 };
        section.Children.Add(new TextBlock { Text = Loc.Chrome("cover.section_title") });

        if (remoteCovers.Count == 0 && localArtwork.Count == 0)
        {
            section.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("cover.none_available"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                TextWrapping = TextWrapping.Wrap,
            });
            return section;
        }

        section.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("cover.pick_hint"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
        });

        var grid = new VariableSizedWrapGrid
        {
            Orientation = Orientation.Horizontal,
            ItemWidth = 140,
            ItemHeight = 160,
        };

        // Highlight only the picked tile: clear every tile's border, then mark
        // the clicked one. Each tile carries its own cover-selection JSON.
        var tiles = new List<Button>();
        void Select(Button picked, string selectionJson)
        {
            foreach (var tile in tiles)
            {
                tile.BorderThickness = new Thickness(0);
            }

            picked.BorderThickness = new Thickness(2);
            picked.BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.DeepSkyBlue);
            onPick(selectionJson);
        }

        void AddTile(ImageSource? source, string caption, string selectionJson)
        {
            var tile = CoverTile(source, caption);
            tile.Click += (_, _) => Select(tile, selectionJson);
            tiles.Add(tile);
            grid.Children.Add(tile);
        }

        // A malformed cover URL or artwork path would throw synchronously out of
        // this static builder (and the async-void caller) — skip that tile instead
        // of crashing the picker, matching the change-cover gallery's guard.
        foreach (var cover in remoteCovers)
        {
            BitmapImage source;
            try
            {
                source = new BitmapImage(new Uri(NativeBae.RemoteCoverThumbnailUrl(cover)));
            }
            catch (UriFormatException)
            {
                continue;
            }

            var selection = NativeBae.RemoteCoverSelectionJson(cover);
            AddTile(source, cover.Label, selection);
        }

        foreach (var art in localArtwork)
        {
            BitmapImage source;
            try
            {
                source = new BitmapImage(new Uri(art.Path));
            }
            catch (UriFormatException)
            {
                continue;
            }

            var selection = JsonSerializer.Serialize(
                new { type = "release_image", file_id = art.FileId });
            AddTile(source, System.IO.Path.GetFileName(art.FileId), selection);
        }

        section.Children.Add(grid);
        return section;
    }

    // Springs the +N badge in over the queue button, holds it ~1.4s, then fades
    // it out. A fresh add while it's visible replaces the count and restarts the
    // hold timer instead of re-springing.
    private void FlashQueueAddBadge(int count)
    {
        QueueAddBadgeText.Text = $"+{count}";

        var springIn = new Storyboard();

        var fadeIn = new DoubleAnimation
        {
            To = 1.0,
            Duration = new Duration(TimeSpan.FromMilliseconds(150)),
            EnableDependentAnimation = true,
        };
        Storyboard.SetTarget(fadeIn, QueueAddBadge);
        Storyboard.SetTargetProperty(fadeIn, "Opacity");
        springIn.Children.Add(fadeIn);

        foreach (var axis in new[] { "ScaleX", "ScaleY" })
        {
            var scaleUp = new DoubleAnimation
            {
                To = 1.0,
                Duration = new Duration(TimeSpan.FromMilliseconds(250)),
                EasingFunction = new BackEase { EasingMode = EasingMode.EaseOut, Amplitude = 0.4 },
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(scaleUp, QueueAddBadgeScale);
            Storyboard.SetTargetProperty(scaleUp, axis);
            springIn.Children.Add(scaleUp);
        }

        springIn.Begin();

        _queueBadgeTimer?.Stop();
        _queueBadgeTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(1400) };
        _queueBadgeTimer.Tick += (_, _) =>
        {
            _queueBadgeTimer?.Stop();

            var fadeOut = new DoubleAnimation
            {
                To = 0.0,
                Duration = new Duration(TimeSpan.FromMilliseconds(250)),
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(fadeOut, QueueAddBadge);
            Storyboard.SetTargetProperty(fadeOut, "Opacity");

            var hide = new Storyboard();
            hide.Children.Add(fadeOut);
            hide.Begin();
        };
        _queueBadgeTimer.Start();
    }

    private async void OnQueueClick(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        // Clear empties only the manual lane (the context survives), so it
        // disables on an empty manual lane regardless of the context.
        var clear = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            IsEnabled = _queueManual.Count > 0,
        };
        clear.Click += (_, _) => WithCurrentHandle(NativeBae.QueueClear);

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(clear);

        // Two distinct sections: the manual lane ("Up Next"), then the context
        // (the release being played from). Each is its own reorderable list —
        // entry ids are unique across both and core no-ops a cross-lane move.
        if (_queueManual.Count > 0)
        {
            content.Children.Add(QueueSectionLabel(Loc.Chrome("queue.section.up_next")));
            content.Children.Add(BuildQueueLaneList(_queueManual));
        }

        if (_queueContext is { Upcoming.Count: > 0 } ctx)
        {
            // The context section names what's playing — a release ("Playing From")
            // vs the whole library — by the source kind the wire shape carries.
            var labelKey = ctx.Kind == BridgePlaybackSourceKind.Library
                ? "queue.section.your_library"
                : "queue.section.playing_from";
            content.Children.Add(ContextSectionLabel(Loc.Chrome(labelKey), ctx.Shuffled));
            content.Children.Add(BuildQueueLaneList(ctx.Upcoming));
        }

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("queue.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        await dialog.ShowAsync();
    }

    // A plain section header for the queue dialog (the manual "Up Next" lane,
    // which is never shuffled and has no shuffle control).
    private static TextBlock QueueSectionLabel(string text) => new()
    {
        Text = text,
        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
    };

    // The context section's header, with a shuffle toggle that flips the context
    // between sequential and shuffled order while the current track keeps playing.
    private StackPanel ContextSectionLabel(string text, bool shuffled)
    {
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        row.Children.Add(new TextBlock
        {
            Text = text,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        });
        // Segoe MDL2 Assets "Shuffle" glyph (U+E8B1), accented when on.
        var toggle = new Button
        {
            Content = new FontIcon
            {
                Glyph = "\uE8B1",
                FontSize = 14,
                Foreground = shuffled
                    ? (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"]
                    : (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
            },
            Padding = new Thickness(6, 2, 6, 2),
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            toggle, Loc.Chrome(shuffled ? "queue.shuffle.off" : "queue.shuffle.on"));
        ToolTipService.SetToolTip(
            toggle, Loc.Chrome(shuffled ? "queue.shuffle.off" : "queue.shuffle.on"));
        toggle.Click += (_, _) => WithCurrentHandle(
            handle => NativeBae.SetShuffle(handle, !shuffled));
        row.Children.Add(toggle);
        return row;
    }

    // One lane's reorderable list: click skips, right-tap removes, drag reorders
    // within the lane (the framework raises a Move, forwarded to core by entry id).
    private ListView BuildQueueLaneList(IEnumerable<BridgeQueueEntry> items)
    {
        var queueItems = new ObservableCollection<QueueEntryRow>(items.Select(item => new QueueEntryRow(item)));
        queueItems.CollectionChanged += (_, args) =>
        {
            if (args.Action == System.Collections.Specialized.NotifyCollectionChangedAction.Move)
            {
                // The collection already reflects the move: the entry now sits at
                // NewStartingIndex and lands before whatever follows it (null when
                // it's now last = move to the lane's end).
                var moved = queueItems[args.NewStartingIndex];
                var beforeIndex = args.NewStartingIndex + 1;
                var beforeEntryId = beforeIndex < queueItems.Count
                    ? queueItems[beforeIndex].EntryId
                    : null;
                WithCurrentHandle(
                    handle => NativeBae.QueueReorder(handle, moved.EntryId, beforeEntryId));
            }
        };

        var list = new ListView
        {
            ItemsSource = queueItems,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            CanReorderItems = true,
            CanDragItems = true,
            AllowDrop = true,
        };
        list.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is QueueEntryRow clicked)
            {
                WithCurrentHandle(handle => NativeBae.QueueSkipTo(handle, clicked.EntryId));
            }
        };
        // Right-tap a row to drop it from the queue. Removing locally too keeps the
        // open dialog in sync; the Move-only reorder handler ignores this Remove.
        list.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element
                || element.DataContext is not QueueEntryRow item)
            {
                return;
            }

            var index = queueItems.IndexOf(item);
            if (index < 0)
            {
                return;
            }

            var menu = new MenuFlyout();
            var remove = new MenuFlyoutItem { Text = Loc.Chrome("queue.remove_item") };
            remove.Click += (_, _) =>
            {
                // The generated bridge removes by entry id, so a reorder between the right-tap and
                // the click can't target the wrong row. The local index is only to keep
                // the open dialog's collection in sync.
                var idx = queueItems.IndexOf(item);
                if (idx < 0)
                {
                    return;
                }
                WithCurrentHandle(handle => NativeBae.QueueRemove(handle, item.EntryId));
                queueItems.RemoveAt(idx);
            };
            menu.Items.Add(remove);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        return list;
    }

    private async System.Threading.Tasks.Task ShowSidePauseDialog(PlaybackPauseReason? reason)
    {
        if (reason?.AlertTitle is not { } title || reason.AlertMessage is not { } message)
        {
            return;
        }

        var dialog = new ContentDialog
        {
            Title = title,
            Content = new TextBlock
            {
                Text = message,
                TextWrapping = TextWrapping.Wrap,
            },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        try
        {
            await dialog.ShowAsync();
        }
        catch (Exception ex)
        {
            BaeDiagnostics.Logger.Warning("Failed to show side-pause dialog", ex);
        }
    }

    /// <summary>
    /// A cover-art thumbnail tile: an image over a one-line caption, the whole
    /// tile a borderless button. The caller wires <c>Click</c> — the
    /// change-cover gallery applies the selection immediately, the import
    /// confirm gallery carries it and highlights the picked tile via the
    /// button's border.
    /// </summary>
    private static Button CoverTile(ImageSource? source, string caption)
    {
        var thumb = new Image
        {
            Source = source,
            Stretch = Stretch.UniformToFill,
            Width = 120,
            Height = 120,
        };
        var label = new TextBlock
        {
            Text = caption,
            TextTrimming = TextTrimming.CharacterEllipsis,
            MaxWidth = 120,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var stack = new StackPanel { Spacing = 4 };
        stack.Children.Add(thumb);
        stack.Children.Add(label);
        return new Button
        {
            Content = stack,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4),
        };
    }

    private void OnPlayPause(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.PlayPause);
        }
    }

    private void OnNext(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.Next);
        }
    }

    private void OnPrevious(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.Previous);
        }
    }

    private void OnRepeat(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.CycleRepeatMode);
        }
    }

    private void OnMute(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.ToggleMute);
        }
    }

    // Ctrl+F focuses the search box from anywhere in the window.
    private void OnFocusSearchAccelerator(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        SearchBox.Focus(FocusState.Programmatic);
        args.Handled = true;
    }

    // Ctrl+L jumps to whatever's playing: open its album's detail and scroll the
    // track into view, flashing it. No-op when nothing is playing.
    private async void OnGoToNowPlaying(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        var albumId = _nowPlaying?.AlbumId;
        if (CurrentHandleOrNull() == null || string.IsNullOrEmpty(albumId))
        {
            return;
        }

        await ShowAlbumDetail(albumId, scrollToTrackId: _nowPlaying?.TrackId);
    }

    // Space toggles play/pause from anywhere — except while typing in a text
    // field, where space must insert a space. Handled here, not as a button
    // accelerator, so a bare Space key never steals input from a text box.
    // Dialog/flyout text inputs are safe for free: they live in separate popups,
    // not under this root Grid, so their KeyDown never bubbles here. The focus
    // check only has to cover text inputs in the main tree — the search box and
    // the welcome chooser's restore-code box.
    private void OnGlobalKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Space)
        {
            return;
        }

        var focused = FocusManager.GetFocusedElement(Content.XamlRoot);
        if (focused is TextBox || focused is AutoSuggestBox)
        {
            return;
        }

        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.PlayPause);
            e.Handled = true;
        }
    }

    private void OnVolumeChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (!_suppressVolume && CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(handle => NativeBae.SetVolume(handle, (float)NpVolume.Value));
        }
    }

    private void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var query = args.QueryText?.Trim() ?? string.Empty;
        if (query.Length == 0)
        {
            LoadCurrentBrowserMode();
        }
        else
        {
            var (current, json) = WithCurrentHandle(handle => NativeBae.SearchJson(handle, query));
            if (!current)
            {
                return;
            }
            SetSearchResults(json, query);
        }
    }

    private void SetSearchResults(string? json, string query)
    {
        var handle = CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }

        ShowSearchBrowser();
        SearchResultsPanel.Children.Clear();
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("search.failed");
            return;
        }

        LibrarySearchResults? results;
        try
        {
            results = JsonSerializer.Deserialize<LibrarySearchResults>(json, JsonOptions);
        }
        catch (JsonException)
        {
            StatusText.Text = Loc.Chrome("search.failed");
            return;
        }
        if (results is null)
        {
            StatusText.Text = Loc.Chrome("search.failed");
            return;
        }
        foreach (var album in results.Albums)
        {
            album.Handle = handle;
        }
        foreach (var composer in results.Composers)
        {
            composer.Handle = handle;
        }
        foreach (var work in results.Works)
        {
            work.Handle = handle;
        }

        if (results.Albums.Count == 0 && results.Tracks.Count == 0
            && results.Composers.Count == 0 && results.Works.Count == 0)
        {
            StatusText.Text = Loc.Chrome("search.no_matches");
            return;
        }

        StatusText.Text = string.Empty;
        AddSearchSection(Loc.Chrome("search.section.albums"), results.Albums, album =>
            SearchButton(album.Title, album.Artist, async () => await ShowAlbumDetail(album.Id)));
        AddSearchSection(Loc.Chrome("search.section.tracks"), results.Tracks, track =>
            SearchButton(track.Title, $"{track.ArtistName} — {track.AlbumTitle}", async () => await ShowAlbumDetail(track.AlbumId)));
        AddSearchSection(Loc.Chrome("search.section.composers"), results.Composers, composer =>
            SearchButton(composer.Name, composer.WorkCountText, async () => await ShowComposerDetail(composer.ArtistId)));
        AddSearchSection(Loc.Chrome("search.section.works"), results.Works, work =>
            SearchButton(work.Title, work.ComposerNames ?? string.Empty, async () => await ShowWorkDetail(work.WorkId)));
    }

    private void AddSearchSection<T>(
        string title,
        IReadOnlyList<T> rows,
        Func<T, Button> button)
    {
        if (rows.Count == 0)
        {
            return;
        }

        SearchResultsPanel.Children.Add(new TextBlock
        {
            Text = title,
            Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"],
        });
        foreach (var row in rows)
        {
            SearchResultsPanel.Children.Add(button(row));
        }
    }

    private static Button SearchButton(string title, string subtitle, Func<System.Threading.Tasks.Task> action)
    {
        var panel = new StackPanel { Spacing = 2 };
        panel.Children.Add(new TextBlock { Text = title, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis });
        if (!string.IsNullOrWhiteSpace(subtitle))
        {
            panel.Children.Add(new TextBlock
            {
                Text = subtitle,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
        }
        var button = new Button
        {
            Content = panel,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
        };
        button.Click += async (_, _) => await action();
        return button;
    }

    /// <summary>Replace the grid's albums from an generated bridge JSON array (or show a status).</summary>
    private void SetAlbums(string? json, string emptyMessage)
    {
        var handle = CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }

        ShowAlbumBrowser();
        Albums.Clear();
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("library.load_failed");
            return;
        }

        var albums = JsonSerializer.Deserialize<List<Album>>(json, JsonOptions)
            ?? new List<Album>();
        foreach (var album in albums)
        {
            // The grid tile fetches its cover bytes by id, which needs the handle;
            // the wire shape carries none, so inject it before the tile binds.
            album.Handle = handle;
            Albums.Add(album);
        }

        StatusText.Text = albums.Count == 0 ? emptyMessage : string.Empty;
    }

    private void LoadComposers()
    {
        ShowComposerBrowser();
        Composers.Clear();
        ComposerDetailPane.Children.Clear();
        var (current, json) = WithCurrentHandle(
            handle => NativeBae.ComposerPageJson(
                handle,
                0,
                FirstPageSize,
                _composerSort.Field,
                _composerSort.Ascending));
        if (!current)
        {
            return;
        }
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("library.load_failed");
            return;
        }

        List<ComposerSummary>? composers;
        try
        {
            composers = JsonSerializer.Deserialize<List<ComposerSummary>>(json, JsonOptions);
        }
        catch (JsonException)
        {
            StatusText.Text = Loc.Chrome("library.load_failed");
            return;
        }
        if (composers is null)
        {
            StatusText.Text = Loc.Chrome("library.load_failed");
            return;
        }
        var handle = CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }
        foreach (var composer in composers)
        {
            composer.Handle = handle;
            Composers.Add(composer);
        }
        StatusText.Text = composers.Count == 0 ? Loc.Chrome("library.no_composers") : string.Empty;
    }

    private async void OnComposerClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not ComposerSummary composer)
        {
            return;
        }

        await ShowComposerDetail(composer.ArtistId);
    }

    private async System.Threading.Tasks.Task ShowComposerDetail(string artistId)
    {
        var (current, json) = await RunForCurrentHandle(
            handle => NativeBae.ComposerDetailJson(handle, artistId));
        if (!current)
        {
            return;
        }
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("composer.open_failed");
            return;
        }

        var detail = JsonSerializer.Deserialize<ComposerDetail>(json, JsonOptions);
        if (detail is null)
        {
            StatusText.Text = Loc.Chrome("composer.open_failed");
            return;
        }
        var handle = CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }
        detail.Composer.Handle = handle;
        foreach (var group in detail.WorkGroups)
        {
            if (group.Parent is not null)
            {
                group.Parent.Handle = handle;
            }
            foreach (var work in group.Works)
            {
                work.Handle = handle;
            }
        }

        ShowComposerBrowser();
        ComposerDetailPane.Children.Clear();
        ComposerDetailPane.Children.Add(new TextBlock
        {
            Text = detail.Composer.Name,
            Style = (Style)Application.Current.Resources["TitleTextBlockStyle"],
        });
        if (detail.WorkGroups.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.works"));
            foreach (var group in detail.WorkGroups)
            {
                if (group.Parent is not null)
                {
                    ComposerDetailPane.Children.Add(WorkButton(group.Parent));
                }
                foreach (var work in group.Works)
                {
                    ComposerDetailPane.Children.Add(WorkButton(work));
                }
            }
        }
        if (detail.UnlinkedReleaseRoles.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.credits"));
            foreach (var role in detail.UnlinkedReleaseRoles)
            {
                ComposerDetailPane.Children.Add(PaneButton(role.AlbumTitle, role.SourceCredit ?? string.Empty, async () =>
                    await ShowAlbumDetail(role.AlbumId)));
            }
        }
        if (detail.UnlinkedTrackRoles.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.recordings"));
            foreach (var role in detail.UnlinkedTrackRoles)
            {
                ComposerDetailPane.Children.Add(PaneButton(role.TrackTitle, role.AlbumTitle, async () => await ShowAlbumDetail(role.AlbumId)));
            }
        }
        if (detail.DefaultWorkId is not null)
        {
            await ShowWorkDetail(detail.DefaultWorkId, replacePane: false);
        }
    }

    private Button WorkButton(WorkSummary work) =>
        PaneButton(work.Title, work.ComposerNames ?? string.Empty, async () => await ShowWorkDetail(work.WorkId));

    private void AddPaneHeader(string title)
    {
        ComposerDetailPane.Children.Add(new TextBlock
        {
            Text = title,
            Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"],
        });
    }

    private static Button PaneButton(string title, string subtitle, Func<System.Threading.Tasks.Task> action)
    {
        var panel = new StackPanel { Spacing = 2 };
        panel.Children.Add(new TextBlock { Text = title, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis });
        if (!string.IsNullOrWhiteSpace(subtitle))
        {
            panel.Children.Add(new TextBlock
            {
                Text = subtitle,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
        }
        var button = new Button
        {
            Content = panel,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
        };
        button.Click += async (_, _) => await action();
        return button;
    }

    private async System.Threading.Tasks.Task ShowWorkDetail(string workId, bool replacePane = true)
    {
        var (current, json) = await RunForCurrentHandle(
            handle => NativeBae.WorkDetailJson(handle, workId));
        if (!current)
        {
            return;
        }
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("work.open_failed");
            return;
        }

        var detail = JsonSerializer.Deserialize<WorkDetail>(json, JsonOptions);
        if (detail is null)
        {
            StatusText.Text = Loc.Chrome("work.open_failed");
            return;
        }
        var handle = CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }
        detail.Work.Handle = handle;
        foreach (var work in detail.ChildWorks)
        {
            work.Handle = handle;
        }
        foreach (var release in detail.Releases)
        {
            release.Handle = handle;
        }

        ShowComposerBrowser();
        if (replacePane)
        {
            ComposerDetailPane.Children.Clear();
        }
        ComposerDetailPane.Children.Add(new TextBlock
        {
            Text = detail.Work.Title,
            Style = (Style)Application.Current.Resources["TitleTextBlockStyle"],
        });
        if (detail.ChildWorks.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.works"));
            foreach (var work in detail.ChildWorks)
            {
                ComposerDetailPane.Children.Add(WorkButton(work));
            }
        }
        if (detail.Releases.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.releases"));
            foreach (var release in detail.Releases)
            {
                ComposerDetailPane.Children.Add(PaneButton(release.AlbumTitle, release.DisplaySubtitle, async () =>
                    await ShowAlbumDetail(release.AlbumId, initialReleaseId: release.ReleaseId)));
            }
        }
        if (detail.Tracks.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.recordings"));
            foreach (var track in detail.Tracks)
            {
                ComposerDetailPane.Children.Add(PaneButton(track.TrackTitle, track.AlbumTitle, async () => await ShowAlbumDetail(track.AlbumId)));
            }
        }
    }

    private async void OnAlbumClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not Album album)
        {
            return;
        }

        await ShowAlbumDetail(album.Id);
    }

    /// <summary>
    /// Open the album-detail dialog for <paramref name="albumId"/>: the album's
    /// releases (with a picker when there's more than one), the track list, and
    /// the per-release actions (play / queue / edit / re-identify / gallery /
    /// change cover / delete). Reused by the album grid (<see cref="OnAlbumClick"/>)
    /// and the import confirmation's "view in library" banner.
    /// </summary>
    private async System.Threading.Tasks.Task ShowAlbumDetail(
        string albumId,
        string? scrollToTrackId = null,
        string? initialReleaseId = null)
    {
        var (current, json) = await RunForCurrentHandle(
            handle => NativeBae.AlbumDetailJson(handle, albumId));
        if (!current)
        {
            return;
        }
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("album.open_failed");
            return;
        }

        var detail = JsonSerializer.Deserialize<AlbumDetail>(json, JsonOptions);
        if (detail is null)
        {
            StatusText.Text = Loc.Chrome("album.open_failed");
            return;
        }

        if (detail.Releases.Count == 0)
        {
            StatusText.Text = Loc.Chrome("album.open_failed");
            return;
        }

        // The release the dialog acts on: the user's primary, or the first. The
        // picker (added below when there's more than one) reassigns it, and every
        // per-release action and the track list read it, so switching release
        // retargets play / queue / edit / gallery / delete to that release.
        // When revealing a now-playing track, start on the release that actually
        // contains it — which may not be the primary.
        Release? trackRelease = scrollToTrackId is null
            ? null
            : detail.Releases.FirstOrDefault(r => r.Tracks.Any(t => t.TrackId == scrollToTrackId));
        Release selectedRelease = trackRelease
            ?? detail.Releases.FirstOrDefault(r => r.ReleaseId == initialReleaseId)
            ?? detail.Releases.FirstOrDefault(r => r.ReleaseId == detail.PrimaryReleaseId)
            ?? detail.Releases[0];

        var header = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
        };
        header.Children.Add(new TextBlock
        {
            Text = detail.Artist,
            VerticalAlignment = VerticalAlignment.Center,
        });
        var shuffleButton = new Button { Content = Loc.Chrome("album.shuffle") };
        var editButton = new Button { Content = Loc.Chrome("album.edit.label") };
        var reidentifyButton = new Button { Content = Loc.Chrome("album.reidentify.label") };
        header.Children.Add(shuffleButton);
        header.Children.Add(editButton);
        header.Children.Add(reidentifyButton);

        // Overflow menu: queueing + delete (release-level, so they need no track ids).
        var deleteRequested = false;
        var moreButton = new Button { Content = "⋯" };
        var moreMenu = new MenuFlyout();
        var playNextItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.play_next") };
        playNextItem.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                WithCurrentHandle(
                    handle => NativeBae.AddReleaseNext(handle, selectedRelease.ReleaseId));
            }
        };
        var addQueueItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
        addQueueItem.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                WithCurrentHandle(
                    handle => NativeBae.AddReleaseToQueue(handle, selectedRelease.ReleaseId));
            }
        };
        // Set the selected release as the album's primary (canonical) one — only
        // meaningful when the album has more than one release.
        var setPrimaryItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.set_primary") };
        setPrimaryItem.Click += async (_, _) =>
        {
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetPrimaryRelease(
                    handle,
                    detail.Id,
                    selectedRelease.ReleaseId));
            if (!current)
            {
                return;
            }
            StatusText.Text = error ?? Loc.Chrome("menu.set_primary_done");
        };
        var changeCoverItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.change_cover") };
        var exportReleaseItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.export") };
        var deleteItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.delete") };
        moreMenu.Items.Add(playNextItem);
        moreMenu.Items.Add(addQueueItem);
        if (detail.Releases.Count > 1)
        {
            moreMenu.Items.Add(setPrimaryItem);
        }
        moreMenu.Items.Add(exportReleaseItem);
        moreMenu.Items.Add(changeCoverItem);
        moreMenu.Items.Add(deleteItem);
        moreButton.Flyout = moreMenu;
        header.Children.Add(moreButton);

        // Export failures surface here, inside the dialog: the window-level banner
        // is occluded by this modal album-detail dialog, so a banner error would be
        // invisible until the dialog is dismissed.
        var exportStatus = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // Track list: click a row to play the release from that track; right-tap
        // for per-track queueing. The play index is the track's position in the
        // release's track list, which is what PlayRelease expects.
        var trackList = new ListView
        {
            ItemsSource = selectedRelease.Tracks,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
        };
        // "Go to now playing" reveal: once the list is realized, scroll the target
        // track into view and flash it. selectedRelease was chosen to contain it.
        if (scrollToTrackId is not null
            && selectedRelease.Tracks.FirstOrDefault(t => t.TrackId == scrollToTrackId) is { } revealTrack)
        {
            trackList.Loaded += (_, _) =>
            {
                trackList.ScrollIntoView(revealTrack);
                FlashTrackRowWhenRealized(trackList, revealTrack, attemptsLeft: 8);
            };
        }
        void PlayFromTrack(Track track)
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                WithCurrentHandle(
                    handle => NativeBae.PlayRelease(
                        handle, selectedRelease.ReleaseId, selectedRelease.Tracks.IndexOf(track), false));
            }
        }
        void QueueTrack(Track track, bool next)
        {
            var (current, error) = WithCurrentHandle(handle => next
                ? NativeBae.AddNext(handle, new[] { track.TrackId })
                : NativeBae.AddToQueue(handle, new[] { track.TrackId }));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                StatusText.Text = error;
            }
        }
        trackList.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is Track track)
            {
                PlayFromTrack(track);
            }
        };
        trackList.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element || element.DataContext is not Track track)
            {
                return;
            }

            var menu = new MenuFlyout();
            var play = new MenuFlyoutItem { Text = Loc.Chrome("menu.play") };
            play.Click += (_, _) => PlayFromTrack(track);
            var playNextTrack = new MenuFlyoutItem { Text = Loc.Chrome("menu.play_next") };
            playNextTrack.Click += (_, _) => QueueTrack(track, next: true);
            var addQueueTrack = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
            addQueueTrack.Click += (_, _) => QueueTrack(track, next: false);
            var exportTrack = new MenuFlyoutItem { Text = Loc.Chrome("menu.export") };
            exportTrack.Click += async (_, _) =>
            {
                exportStatus.Visibility = Visibility.Collapsed;
                var picker = new global::Windows.Storage.Pickers.FileSavePicker();
                WinRT.Interop.InitializeWithWindow.Initialize(
                    picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
                var (settingsCurrent, settingsJson) = await RunForCurrentHandle(NativeBae.SettingsJson);
                if (!settingsCurrent)
                {
                    return;
                }
                if (settingsJson is null)
                {
                    exportStatus.Text = Loc.Chrome("track.export.prepare_failed");
                    exportStatus.Visibility = Visibility.Visible;
                    return;
                }
                var settings = JsonSerializer.Deserialize<Settings>(settingsJson, JsonOptions);
                if (settings is null)
                {
                    exportStatus.Text = Loc.Chrome("track.export.prepare_failed");
                    exportStatus.Visibility = Visibility.Visible;
                    return;
                }
                var trackPresets = settings.ExportPresets
                    .Where(preset => preset.AppliesToTrack)
                    .ToList();
                var originalSelection = ExportSelection.Original();
                var originalSelectionJson = JsonSerializer.Serialize(originalSelection, JsonOptions);
                var (extensionCurrent, originalExtension) = await RunForCurrentHandle(
                    handle => NativeBae.ExportTrackExtension(handle, track.TrackId, originalSelectionJson));
                if (!extensionCurrent)
                {
                    return;
                }
                if (originalExtension is null)
                {
                    exportStatus.Text = Loc.Chrome("track.export.prepare_failed");
                    exportStatus.Visibility = Visibility.Visible;
                    return;
                }
                var choices = new List<(string Label, string Extension, ExportSelection Selection)>
                {
                    (Loc.Chrome("track.export.original"), $".{originalExtension}", originalSelection),
                };
                choices.AddRange(trackPresets.Select(preset => (
                    preset.TrackPickerLabel,
                    preset.FileExtension,
                    ExportSelection.Preset(preset.Id))));
                var formatPicker = new ComboBox
                {
                    Header = Loc.Chrome("settings.export.default_track_format"),
                    MinWidth = 260,
                };
                var defaultIndex = -1;
                for (var index = 0; index < choices.Count; index++)
                {
                    var choice = choices[index];
                    if (ExportSelectionsEqual(choice.Selection, settings.DefaultTrackExportSelection))
                    {
                        defaultIndex = index;
                    }
                    formatPicker.Items.Add(new ComboBoxItem
                    {
                        Content = choice.Label,
                    });
                }
                if (defaultIndex < 0)
                {
                    exportStatus.Text = Loc.Chrome("track.export.prepare_failed");
                    exportStatus.Visibility = Visibility.Visible;
                    return;
                }
                formatPicker.SelectedIndex = defaultIndex;
                var formatDialog = new ContentDialog
                {
                    Title = Loc.Chrome("settings.export.label"),
                    Content = formatPicker,
                    PrimaryButtonText = Loc.Chrome("settings.export.label"),
                    CloseButtonText = Loc.Chrome("action.cancel"),
                    XamlRoot = Content.XamlRoot,
                };
                var formatResult = await formatDialog.ShowAsync();
                if (formatResult != ContentDialogResult.Primary)
                {
                    return;
                }
                if (formatPicker.SelectedIndex < 0)
                {
                    exportStatus.Text = Loc.Chrome("track.export.prepare_failed");
                    exportStatus.Visibility = Visibility.Visible;
                    return;
                }
                var selectedChoice = choices[formatPicker.SelectedIndex];
                picker.FileTypeChoices.Add(
                    selectedChoice.Label,
                    new List<string> { selectedChoice.Extension });
                // Seed the suggested name from the configured filename template,
                // which the core renders and sanitizes from this track's metadata
                // (falling back to the title, then a fixed stem, on its own). A
                // null return — or a throw — means that render failed (the core
                // logged the cause); surface it and abort rather than exporting
                // under a guessed name.
                string? stem;
                try
                {
                    var (nameCurrent, suggestedName) = await RunForCurrentHandle(
                        handle => NativeBae.ExportTrackSuggestedName(handle, track.TrackId));
                    if (!nameCurrent)
                    {
                        return;
                    }
                    stem = suggestedName;
                }
                catch (Exception ex)
                {
                    BaeDiagnostics.Logger.Error("export suggested-name lookup threw", ex);
                    stem = null;
                }
                if (stem is null)
                {
                    exportStatus.Text = Loc.Chrome("track.export.prepare_failed");
                    exportStatus.Visibility = Visibility.Visible;
                    return;
                }
                picker.SuggestedFileName = stem;
                var file = await picker.PickSaveFileAsync();
                if (file is null)
                {
                    return;
                }

                var selectionJson = JsonSerializer.Serialize(selectedChoice.Selection, JsonOptions);
                var path = file.Path;
                var (exportCurrent, error) = await RunForCurrentHandle(
                    handle => NativeBae.ExportTrack(handle, track.TrackId, path, selectionJson));
                if (!exportCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    exportStatus.Text = error;
                    exportStatus.Visibility = Visibility.Visible;
                }
            };
            menu.Items.Add(play);
            menu.Items.Add(playNextTrack);
            menu.Items.Add(addQueueTrack);
            menu.Items.Add(exportTrack);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(header);
        // Release picker, only when the album has more than one pressing. Choosing
        // a release swaps the track list and retargets every per-release action.
        if (detail.Releases.Count > 1)
        {
            var releasePicker = new ComboBox
            {
                Header = Loc.Chrome("album.release_picker"),
                ItemsSource = detail.Releases,
                SelectedItem = selectedRelease,
            };
            releasePicker.SelectionChanged += (_, _) =>
            {
                if (releasePicker.SelectedItem is Release release)
                {
                    selectedRelease = release;
                    trackList.ItemsSource = selectedRelease.Tracks;
                }
            };
            content.Children.Add(releasePicker);
        }
        content.Children.Add(trackList);
        content.Children.Add(exportStatus);

        var dialog = new ContentDialog
        {
            Title = detail.Title,
            Content = content,
            PrimaryButtonText = Loc.Chrome("action.play"),
            SecondaryButtonText = Loc.Chrome("album.gallery"),
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };

        // Edit and re-identify each replace the detail dialog: close it, then open
        // the chosen sheet on the primary release. ShowAsync resolves with None
        // when Hide() is called.
        var editRequested = false;
        var reidentifyRequested = false;
        var shuffleRequested = false;
        editButton.Click += (_, _) =>
        {
            editRequested = true;
            dialog.Hide();
        };
        reidentifyButton.Click += (_, _) =>
        {
            reidentifyRequested = true;
            dialog.Hide();
        };
        shuffleButton.Click += (_, _) =>
        {
            shuffleRequested = true;
            dialog.Hide();
        };
        // Changing the cover opens its own gallery dialog; a nested ContentDialog
        // can't open over this one, so close it first and open the gallery after
        // ShowAsync returns (the gallery/edit/re-identify pattern).
        var changeCoverRequested = false;
        var exportReleaseRequested = false;
        changeCoverItem.Click += (_, _) =>
        {
            changeCoverRequested = true;
            dialog.Hide();
        };
        exportReleaseItem.Click += (_, _) =>
        {
            exportReleaseRequested = true;
            dialog.Hide();
        };
        deleteItem.Click += (_, _) =>
        {
            deleteRequested = true;
            dialog.Hide();
        };

        var result = await dialog.ShowAsync();
        if (editRequested)
        {
            await ShowEditMetadata(selectedRelease.ReleaseId);
        }
        else if (reidentifyRequested)
        {
            await ShowReidentify(selectedRelease.ReleaseId, detail.Artist, detail.Title);
        }
        else if (shuffleRequested)
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                WithCurrentHandle(
                    handle => NativeBae.PlayRelease(handle, selectedRelease.ReleaseId, -1, true));
            }
        }
        else if (changeCoverRequested)
        {
            await ShowChangeCover(detail.Id, selectedRelease.ReleaseId);
        }
        else if (exportReleaseRequested && !string.IsNullOrEmpty(selectedRelease.ReleaseId))
        {
            await ShowExportRelease(selectedRelease.ReleaseId);
        }
        else if (deleteRequested)
        {
            await ConfirmDeleteRelease(selectedRelease.ReleaseId);
        }
        else if (result == ContentDialogResult.Primary && !string.IsNullOrEmpty(selectedRelease.ReleaseId))
        {
            WithCurrentHandle(
                handle => NativeBae.PlayRelease(handle, selectedRelease.ReleaseId, -1, false));
        }
        else if (result == ContentDialogResult.Secondary)
        {
            await ShowGallery(selectedRelease.ReleaseId);
        }
    }

    // ScrollIntoView realizes the target row's container only after a later layout
    // pass, so poll for it across a few UI ticks before flashing. Without this the
    // flash silently no-ops for any track below the initial viewport — the common
    // case for "go to now playing". Gives up after a bounded number of attempts.
    private static void FlashTrackRowWhenRealized(ListView list, Track track, int attemptsLeft)
    {
        if (list.ContainerFromItem(track) is ListViewItem row)
        {
            FlashRow(row);
            return;
        }

        if (attemptsLeft <= 0)
        {
            return;
        }

        list.DispatcherQueue.TryEnqueue(() => FlashTrackRowWhenRealized(list, track, attemptsLeft - 1));
    }

    // Tint a row with the system accent and fade it out over three seconds — the
    // "go to now playing" flash, mirroring macOS.
    private static void FlashRow(ListViewItem row)
    {
        var accent = new global::Windows.UI.ViewManagement.UISettings()
            .GetColorValue(global::Windows.UI.ViewManagement.UIColorType.Accent);
        var brush = new SolidColorBrush(accent) { Opacity = 0.35 };
        row.Background = brush;

        var fade = new DoubleAnimation
        {
            To = 0,
            Duration = new Duration(TimeSpan.FromSeconds(3)),
            EnableDependentAnimation = true,
        };
        Storyboard.SetTarget(fade, brush);
        Storyboard.SetTargetProperty(fade, "Opacity");
        var storyboard = new Storyboard();
        storyboard.Children.Add(fade);
        storyboard.Begin();
    }

    private async System.Threading.Tasks.Task ShowExportRelease(string releaseId)
    {
        var (settingsCurrent, settingsJson) = await RunForCurrentHandle(NativeBae.SettingsJson);
        if (!settingsCurrent)
        {
            return;
        }
        if (settingsJson is null)
        {
            StatusText.Text = Loc.Chrome("track.export.prepare_failed");
            return;
        }
        var settings = JsonSerializer.Deserialize<Settings>(settingsJson, JsonOptions);
        if (settings is null)
        {
            StatusText.Text = Loc.Chrome("track.export.prepare_failed");
            return;
        }
        var choices = new List<(string Label, ExportSelection Selection)>
        {
            (Loc.Chrome("track.export.original"), ExportSelection.Original()),
        };
        choices.AddRange(settings.ExportPresets
            .Where(preset => preset.AppliesToRelease)
            .Select(preset => (preset.Name, ExportSelection.Preset(preset.Id))));

        var formatPicker = new ComboBox
        {
            Header = Loc.Chrome("settings.export.default_release_format"),
            MinWidth = 260,
        };
        var defaultIndex = -1;
        for (var index = 0; index < choices.Count; index++)
        {
            var choice = choices[index];
            if (ExportSelectionsEqual(choice.Selection, settings.DefaultReleaseExportSelection))
            {
                defaultIndex = index;
            }
            formatPicker.Items.Add(new ComboBoxItem
            {
                Content = choice.Label,
                Tag = choice.Selection,
            });
        }
        if (defaultIndex < 0)
        {
            StatusText.Text = Loc.Chrome("track.export.prepare_failed");
            return;
        }
        formatPicker.SelectedIndex = defaultIndex;

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.export.label"),
            Content = formatPicker,
            PrimaryButtonText = Loc.Chrome("settings.export.label"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary)
        {
            return;
        }
        if (formatPicker.SelectedItem is not ComboBoxItem selectedItem
            || selectedItem.Tag is not ExportSelection selection)
        {
            StatusText.Text = Loc.Chrome("track.export.prepare_failed");
            return;
        }

        var picker = new global::Windows.Storage.Pickers.FolderPicker();
        picker.FileTypeFilter.Add("*");
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var folder = await picker.PickSingleFolderAsync();
        if (folder is null)
        {
            return;
        }

        var selectionJson = JsonSerializer.Serialize(selection, JsonOptions);
        var (exportCurrent, error) = await RunForCurrentHandle(
            handle => NativeBae.ExportRelease(handle, releaseId, folder.Path, selectionJson));
        if (!exportCurrent)
        {
            return;
        }
        if (error is not null)
        {
            StatusText.Text = error;
        }
    }

    private static bool ExportSelectionsEqual(ExportSelection a, ExportSelection b) =>
        a.Kind == b.Kind && a.PresetId == b.PresetId;

    private async System.Threading.Tasks.Task ConfirmDeleteRelease(string releaseId)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var confirm = new ContentDialog
        {
            Title = Loc.Chrome("album.delete.title"),
            Content = Loc.Chrome("album.delete.body"),
            PrimaryButtonText = Loc.Chrome("action.delete"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };
        if (await confirm.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }

        // On success an invalidation refreshes the grid.
        var (current, error) = await RunForCurrentHandle(
            handle => NativeBae.DeleteRelease(handle, releaseId));
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            await ShowError(Loc.Chrome("album.delete.failed"), error);
        }
    }

    private async System.Threading.Tasks.Task ShowEditMetadata(string releaseId)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var (current, seeded) = WithCurrentHandle(
            handle => NativeBae.ReleaseEditSeed(handle, releaseId).Edit);
        if (!current)
        {
            return;
        }
        if (seeded is null)
        {
            await ShowError(Loc.Chrome("album.edit.load_failed"));
            return;
        }

        var form = new ReleaseEditForm(seeded, 520);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("album.edit.title"),
            Content = new ScrollViewer { Content = form.Panel },
            PrimaryButtonText = Loc.Chrome("action.save"),
            SecondaryButtonText = Loc.Chrome("album.edit.reset"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
        };

        // Shape + write happen in Rust; on a validation/write error keep the
        // dialog open and show the reason instead of committing.
        dialog.PrimaryButtonClick += (_, args) =>
        {
            var (editCurrent, error) = WithCurrentHandle(
                handle => NativeBae.ApplyReleaseEdit(handle, releaseId, form.ReadBack()));
            if (!editCurrent)
            {
                args.Cancel = true;
                return;
            }
            if (error is not null)
            {
                form.ErrorText.Text = error;
                form.ErrorText.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
        };

        // Reset to Source discards the in-progress edits and re-seeds the form
        // from the release's stored metadata source (its original identity)
        // without writing the DB. Keep the dialog open regardless; a deferral
        // holds it through the async re-projection so it can't close mid-await.
        dialog.SecondaryButtonClick += async (_, args) =>
        {
            args.Cancel = true;
            var deferral = args.GetDeferral();
            try
            {
                var (resetCurrent, fresh) = await RunForCurrentHandle(
                    handle => NativeBae.ResetMetadataToSource(handle, releaseId).Edit);
                if (!resetCurrent)
                {
                    return;
                }
                if (fresh is null)
                {
                    form.ErrorText.Text = Loc.Chrome("album.edit.reset_failed");
                    form.ErrorText.Visibility = Visibility.Visible;
                    return;
                }

                form.ErrorText.Visibility = Visibility.Collapsed;
                form.Seed(fresh);
            }
            finally
            {
                deferral.Complete();
            }
        };

        await dialog.ShowAsync();
    }

    private async System.Threading.Tasks.Task ShowReidentify(string releaseId, string seedArtist, string seedAlbum)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var artistBox = new TextBox { Header = Loc.Chrome("search.field.artist"), Text = seedArtist };
        var albumBox = new TextBox { Header = Loc.Chrome("search.field.album"), Text = seedAlbum };
        var sourceBox = new ComboBox { Header = Loc.Chrome("search.field.source") };
        sourceBox.Items.Add("discogs");
        sourceBox.Items.Add("musicbrainz");
        sourceBox.SelectedIndex = 0;
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        var resultsList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 280,
        };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        var form = new StackPanel { Spacing = 8, Width = 420 };
        form.Children.Add(artistBox);
        form.Children.Add(albumBox);
        form.Children.Add(sourceBox);
        form.Children.Add(searchButton);
        form.Children.Add(resultsList);
        form.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("album.reidentify.title"),
            Content = new ScrollViewer { Content = form },
            PrimaryButtonText = Loc.Chrome("album.reidentify.confirm"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = Content.XamlRoot,
            IsPrimaryButtonEnabled = false,
        };

        var candidates = new List<Candidate>();

        // The generated bridge search and commit both block on network/DB work; run them off
        // the UI thread so the dialog stays responsive.
        searchButton.Click += async (_, _) =>
        {
            var source = (string)sourceBox.SelectedItem;
            var artist = artistBox.Text;
            var album = albumBox.Text;
            searchButton.IsEnabled = false;
            var (current, json) = await RunForCurrentHandle(
                handle => NativeBae.SearchReleasesJson(handle, source, artist, album));
            searchButton.IsEnabled = true;
            if (!current)
            {
                return;
            }

            var parsed = json is null ? null : JsonSerializer.Deserialize<List<Candidate>>(json, JsonOptions);
            if (parsed is null)
            {
                status.Text = Loc.Chrome("search.failed");
                status.Visibility = Visibility.Visible;
                return;
            }

            candidates = parsed;
            resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
            status.Text = Loc.Chrome("search.no_matches");
            status.Visibility = candidates.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
            dialog.IsPrimaryButtonEnabled = false;
        };

        resultsList.SelectionChanged += (_, _) =>
        {
            dialog.IsPrimaryButtonEnabled = resultsList.SelectedIndex >= 0;
        };

        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var index = resultsList.SelectedIndex;
            if (index < 0 || index >= candidates.Count)
            {
                args.Cancel = true;
                return;
            }

            var chosen = candidates[index];
            var deferral = args.GetDeferral();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.ReidentifyRelease(handle, releaseId, chosen.ReleaseId, chosen.Source));
            if (!current)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        await dialog.ShowAsync();
    }

    private async System.Threading.Tasks.Task ShowGallery(string releaseId)
    {
        var (current, images) = WithCurrentHandle(
            handle => NativeBae.Gallery(handle, releaseId).Items);
        if (!current)
        {
            return;
        }
        if (images is null)
        {
            StatusText.Text = Loc.Chrome("gallery.load_failed");
            return;
        }

        if (images.Length == 0)
        {
            return;
        }

        var index = 0;
        var image = new Image { Stretch = Stretch.Uniform, MinHeight = 360, MinWidth = 360 };
        var label = new TextBlock { HorizontalAlignment = HorizontalAlignment.Center };
        void Show()
        {
            var item = images[index];
            var handle = CurrentHandleOrNull();
            if (handle == null)
            {
                return;
            }
            image.Source = CoverImage.LoadGalleryBytes(handle, releaseId, item.Source);
            label.Text = $"{item.Label} ({index + 1}/{images.Length})";
        }
        Show();

        var prev = new Button { Content = "‹" };
        var next = new Button { Content = "›" };
        prev.Click += (_, _) => { index = (index - 1 + images.Length) % images.Length; Show(); };
        next.Click += (_, _) => { index = (index + 1) % images.Length; Show(); };

        var nav = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Center,
            Spacing = 12,
        };
        nav.Children.Add(prev);
        nav.Children.Add(label);
        nav.Children.Add(next);

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(image);
        content.Children.Add(nav);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("gallery.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        await dialog.ShowAsync();
    }

    /// <summary>
    /// Pick a new cover for <paramref name="releaseId"/>: the release's own image
    /// files plus remote candidates fetched from MusicBrainz / Discogs. Selecting
    /// one writes it as the release's cover; the album grid refreshes via the
    /// invalidation the change emits. Errors surface inside this dialog,
    /// since the window banner is occluded by the modal.
    /// </summary>
    private async System.Threading.Tasks.Task ShowChangeCover(string albumId, string releaseId)
    {
        if (string.IsNullOrEmpty(albumId) || string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var (imagesCurrent, releaseImages) = WithCurrentHandle(
            handle => NativeBae.GetReleaseImages(handle, releaseId).Images);
        if (!imagesCurrent)
        {
            return;
        }
        if (releaseImages is null)
        {
            StatusText.Text = Loc.Chrome("cover.images_load_failed");
            return;
        }

        var content = new StackPanel { Spacing = 8, MinWidth = 460 };

        // Errors from a failed remote fetch or a failed change surface here; the
        // window-level banner is hidden behind this modal dialog.
        var statusText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(statusText);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("cover.change_title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 520 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };

        // Apply a selection off the UI thread (a remote cover downloads bytes),
        // then close on success or show the error in place.
        async System.Threading.Tasks.Task Apply(string selectionJson)
        {
            statusText.Visibility = Visibility.Collapsed;
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.ChangeCover(handle, albumId, releaseId, selectionJson));
            if (!current)
            {
                return;
            }
            if (error is null)
            {
                dialog.Hide();
            }
            else
            {
                statusText.Text = error;
                statusText.Visibility = Visibility.Visible;
            }
        }

        // A thumbnail tile that applies the selection when clicked.
        Button Tile(ImageSource? source, string caption, string selectionJson)
        {
            var button = CoverTile(source, caption);
            button.Click += async (_, _) => await Apply(selectionJson);
            return button;
        }

        if (releaseImages.Length > 0)
        {
            content.Children.Add(new TextBlock { Text = Loc.Chrome("cover.release_files") });
            var fileGrid = new VariableSizedWrapGrid
            {
                Orientation = Orientation.Horizontal,
                ItemWidth = 140,
                ItemHeight = 160,
            };
            foreach (var file in releaseImages)
            {
                var handle = CurrentHandleOrNull();
                if (handle == null)
                {
                    return;
                }
                var source = CoverImage.LoadGalleryBytes(
                    handle, releaseId, new BridgeGallerySource.ReleaseFile(file.Id));
                var selection = JsonSerializer.Serialize(
                    new { type = "release_image", file_id = file.Id });
                fileGrid.Children.Add(Tile(source, file.OriginalFilename, selection));
            }

            content.Children.Add(fileGrid);
        }

        content.Children.Add(new TextBlock { Text = Loc.Chrome("cover.remote_sources") });
        var loading = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        loading.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        loading.Children.Add(new TextBlock { Text = Loc.Chrome("cover.fetching") });
        content.Children.Add(loading);

        var remoteGrid = new VariableSizedWrapGrid
        {
            Orientation = Orientation.Horizontal,
            ItemWidth = 140,
            ItemHeight = 160,
        };
        content.Children.Add(remoteGrid);

        // Fetch the remote candidates off the UI thread, then fill the grid on
        // resume. The dialog opens immediately with the release files shown and a
        // spinner where the remote covers will land.
        async System.Threading.Tasks.Task LoadRemote()
        {
            var (current, covers) = await RunForCurrentHandle(
                handle => NativeBae.FetchRemoteCovers(handle, releaseId).Covers);
            if (!current)
            {
                return;
            }
            loading.Visibility = Visibility.Collapsed;
            if (covers is null)
            {
                statusText.Text = Loc.Chrome("cover.fetch_failed");
                statusText.Visibility = Visibility.Visible;
                return;
            }

            try
            {
                if (covers.Length == 0)
                {
                    remoteGrid.Children.Add(new TextBlock { Text = Loc.Chrome("cover.none_remote") });
                    return;
                }

                foreach (var cover in covers)
                {
                    var source = new BitmapImage(new Uri(NativeBae.RemoteCoverThumbnailUrl(cover)));
                    var selection = NativeBae.RemoteCoverSelectionJson(cover);
                    remoteGrid.Children.Add(Tile(source, cover.Label, selection));
                }
            }
            catch (Exception ex)
            {
                // Fire-and-forget: a malformed cover URL or unexpected payload must
                // surface here, not as an unobserved task exception.
                statusText.Text = Loc.Chrome("cover.show_failed", "detail", ex.Message);
                statusText.Visibility = Visibility.Visible;
            }
        }

        _ = LoadRemote();
        await dialog.ShowAsync();
    }

    private async void OnStorageClick(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var listPanel = new StackPanel { Spacing = 4, MinWidth = 460 };
        var storageStatus = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // The releases whose rows are selected. Right-clicking applies the
        // chosen action to the whole selection (or to just the right-tapped row
        // when it isn't part of it). Releases that vanish on reload (e.g. an
        // local release moved out of the library) drop out below.
        var selected = new HashSet<string>();
        // The current rows, kept so the right-tap menu can resolve a release's
        // allowed actions (for the multi-select intersection) by id.
        var rowsById = new Dictionary<string, BridgeStorageRow>();

        // Each row shows its summary; a left-click toggles its selection and a
        // right-click opens a menu of the transitions the core says it allows
        // (carried on the row, gated on cloud-home + pending uploads), plus
        // cancel for any queued uploads. The same actions run on every selected
        // release.
        async System.Threading.Tasks.Task LoadStorageRows()
        {
            var (current, result) = await RunForCurrentHandle(NativeBae.StorageRows);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            if (result.Rows is null)
            {
                storageStatus.Text = Loc.Chrome("storage.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            storageStatus.Visibility = Visibility.Collapsed;
            rowsById.Clear();
            foreach (var row in result.Rows)
            {
                rowsById[row.Release.Id] = row;
            }
            // Drop selections for releases no longer present after a transition.
            selected.IntersectWith(rowsById.Keys);

            listPanel.Children.Clear();
            foreach (var row in result.Rows)
            {
                var text = new TextBlock
                {
                    Text = StorageRowSummary(row),
                    VerticalAlignment = VerticalAlignment.Center,
                    TextWrapping = TextWrapping.Wrap,
                };
                var releaseId = row.Release.Id;
                var rowBorder = new Border
                {
                    Child = text,
                    // The release id rides on Tag so RefreshRowHighlights can
                    // recolor each row from the current selection.
                    Tag = releaseId,
                    Padding = new Thickness(6, 4, 6, 4),
                    CornerRadius = new CornerRadius(4),
                    Background = RowBackground(selected.Contains(releaseId)),
                };

                rowBorder.Tapped += (_, _) =>
                {
                    if (!selected.Add(releaseId))
                    {
                        selected.Remove(releaseId);
                    }
                    rowBorder.Background = RowBackground(selected.Contains(releaseId));
                };
                rowBorder.RightTapped += async (_, args) =>
                {
                    // The args are only valid synchronously; capture the tap
                    // position before any await.
                    var position = args.GetPosition(rowBorder);

                    // Act on the selection when this row is part of it, else on
                    // just this row (and select it, matching the macOS menu).
                    if (!selected.Contains(releaseId))
                    {
                        selected.Clear();
                        selected.Add(releaseId);
                        RefreshRowHighlights();
                    }

                    var menu = await BuildStorageRowMenu(
                        selected.ToList(), rowsById, storageStatus, LoadStorageRows);
                    // Nothing to offer (e.g. no cloud home, or uploads in flight)
                    // — skip the empty popup.
                    if (menu.Items.Count > 0)
                    {
                        menu.ShowAt(rowBorder, new FlyoutShowOptions { Position = position });
                    }
                };

                listPanel.Children.Add(rowBorder);
            }
        }

        // Repaint every row's background from the current selection. The storage
        // list is a flat StackPanel of Borders tagged with their release id.
        void RefreshRowHighlights()
        {
            foreach (var child in listPanel.Children)
            {
                if (child is Border border && border.Tag is string id)
                {
                    border.Background = RowBackground(selected.Contains(id));
                }
            }
        }

        await LoadStorageRows();

        // Cloud outbox: the upload/delete queue with a summary band, a Retry-now
        // button, and per-item Cancel. Hidden (empty panel) when nothing is queued.
        // Reloaded after retry/cancel so the panel reflects the new queue state.
        var downloadsPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadDownloads()
        {
            downloadsPanel.Children.Clear();
            var (current, result) = await RunForCurrentHandle(NativeBae.DownloadSnapshot);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            var snapshot = result.Snapshot;
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("storage.read_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            // Hidden when the pin queue is idle, like the outbox panel.
            if (snapshot.Downloads.Length == 0)
            {
                return;
            }

            string StateLabel(BridgeDownloadOp op) => op.State switch
            {
                BridgeDownloadState.Active => Loc.Chrome("download.state.downloading"),
                BridgeDownloadState.Failed => Loc.Chrome("download.state.failed"),
                _ => Loc.Chrome("download.state.queued"),
            };

            string DownloadDetail(BridgeDownloadOp op)
            {
                static string DisplayBytes(ulong bytes) =>
                    Loc.Bytes(checked((long)bytes));

                var parts = new List<string>
                {
                    Loc.Chrome("storage.files", "count", op.FileCount),
                    Loc.Bytes(op.TotalSize),
                    StateLabel(op),
                };
                if (DownloadProgress(op.State) is { } progress)
                {
                    parts.Add(
                        Loc.Core(
                            "core.download.bytes_progress",
                            new Dictionary<string, object?>
                            {
                                ["done"] = DisplayBytes(progress.BytesDone),
                                ["total"] = DisplayBytes(progress.BytesTotal),
                            }));
                }
                return string.Join(" · ", parts);
            }

            // Header: a label (or "paused"), Retry (only with failures), and a
            // pause/resume toggle — mirroring the outbox panel's band.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = snapshot.Paused ? Loc.Chrome("download.paused") : Loc.Chrome("download.title"),
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button
            {
                Content = Loc.Chrome("outbox.retry_now"),
                IsEnabled = snapshot.Total.Failed > 0,
            };
            retry.Click += async (_, _) =>
            {
                retry.IsEnabled = false;
                await RunForCurrentHandle(NativeBae.RetryDownloads);
                await LoadDownloads();
            };
            band.Children.Add(retry);
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await RunForCurrentHandle(
                    handle => NativeBae.SetDownloadsPaused(handle, !paused));
                await LoadDownloads();
            };
            band.Children.Add(pause);
            downloadsPanel.Children.Add(band);

            // One row per release: title, "N files · size · state", and a cancel.
            foreach (var op in snapshot.Downloads)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = op.Title, TextWrapping = TextWrapping.Wrap });
                labelColumn.Children.Add(new TextBlock
                {
                    Text = DownloadDetail(op),
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                });
                if (DownloadProgress(op.State) is { } progress)
                {
                    labelColumn.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = 1,
                        Value = progress.Fraction,
                        Height = 4,
                    });
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                var releaseId = op.ReleaseId;
                var cancel = new Button { Content = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    storageStatus.Visibility = Visibility.Collapsed;
                    cancel.IsEnabled = false;
                    var (cancelCurrent, error) = await RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!cancelCurrent)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
                        cancel.IsEnabled = true;
                        return;
                    }

                    await LoadDownloads();
                };
                Grid.SetColumn(cancel, 1);
                itemGrid.Children.Add(cancel);
                downloadsPanel.Children.Add(itemGrid);
            }
        }

        var outboxPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadOutbox()
        {
            outboxPanel.Children.Clear();
            var (current, result) = await RunForCurrentHandle(NativeBae.OutboxSnapshot);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            var snapshot = result.Snapshot;
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("outbox.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            if (snapshot.UploadGroups.Length == 0 && snapshot.Deletes.Length == 0)
            {
                return;
            }

            // With work queued at least one count is non-zero, so compose the
            // localized queue summary from the generated snapshot counts.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = OutboxSummary(snapshot),
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button { Content = Loc.Chrome("outbox.retry_now") };
            retry.Click += async (_, _) =>
            {
                storageStatus.Visibility = Visibility.Collapsed;
                retry.IsEnabled = false;
                var (retryCurrent, error) = await RunForCurrentHandle(NativeBae.RetryOutbox);
                if (!retryCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                    retry.IsEnabled = true;
                    return;
                }

                await LoadOutbox();
            };
            band.Children.Add(retry);
            // Pause/resume the upload pipeline. Paused leaves items queued but stops
            // the sync cycle from draining them.
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await RunForCurrentHandle(handle => NativeBae.SetSyncPaused(handle, !paused));
                await LoadOutbox();
            };
            band.Children.Add(pause);
            outboxPanel.Children.Add(band);

            // Master progress strip: a byte-progress bar (dimmed while paused) and
            // locale-formatted byte / throughput / ETA labels.
            if (snapshot.Total.BytesTotal > 0)
            {
                outboxPanel.Children.Add(new ProgressBar
                {
                    Minimum = 0,
                    Maximum = checked((long)snapshot.Total.BytesTotal),
                    Value = checked((long)snapshot.Total.BytesDone),
                    Opacity = paused ? 0.4 : 1.0,
                });
                var detail = new List<string>();
                var bytesLabel = OutboxBytesLabel(snapshot);
                if (!string.IsNullOrEmpty(bytesLabel))
                {
                    detail.Add(bytesLabel);
                }
                var throughputLabel = OutboxThroughputLabel(snapshot);
                if (!string.IsNullOrEmpty(throughputLabel))
                {
                    detail.Add(throughputLabel);
                }
                var etaLabel = OutboxEtaLabel(snapshot);
                if (!string.IsNullOrEmpty(etaLabel))
                {
                    detail.Add(etaLabel);
                }
                if (detail.Count > 0)
                {
                    outboxPanel.Children.Add(new TextBlock
                    {
                        Text = string.Join(" · ", detail),
                        FontSize = 12,
                        Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    });
                }
            }

            // A queue row: a label (with an optional progress bar), an optional
            // trailing button, and an optional right-click menu.
            void AddOutboxRow(string label, ProgressBar? progress, Button? trailing, MenuFlyout? contextMenu)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = label, TextWrapping = TextWrapping.Wrap });
                if (progress is not null)
                {
                    labelColumn.Children.Add(progress);
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                if (trailing is not null)
                {
                    Grid.SetColumn(trailing, 1);
                    itemGrid.Children.Add(trailing);
                }
                if (contextMenu is not null)
                {
                    itemGrid.ContextFlyout = contextMenu;
                }
                outboxPanel.Children.Add(itemGrid);
            }

            // Runs `action` off-thread, surfaces any error to the status line, and
            // reloads the panel on success — shared by the row button and menu.
            async System.Threading.Tasks.Task RunCancel(Func<AppHandle, string?> action)
            {
                storageStatus.Visibility = Visibility.Collapsed;
                var (current, error) = await RunForCurrentHandle(action);
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                    return;
                }

                await LoadOutbox();
            }

            // A right-click "Cancel" menu, matching the storage table's per-release
            // cancel. Used for the upload release rows.
            MenuFlyout CancelFlyout(Func<AppHandle, string?> action)
            {
                var menu = new MenuFlyout();
                var item = new MenuFlyoutItem { Text = Loc.Chrome("action.cancel") };
                item.Click += async (_, _) => await RunCancel(action);
                menu.Items.Add(item);
                return menu;
            }

            // Uploads: one row per release (matching the storage table) — title,
            // aggregate progress, and a right-click "Cancel" that stops the
            // release's transition. The orphaned-files bucket (no release id) has
            // no release to cancel.
            foreach (var group in snapshot.UploadGroups)
            {
                ProgressBar? progress = group.Progress is { Active: > 0, BytesTotal: > 0 }
                    ? new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = checked((long)group.Progress.BytesTotal),
                        Value = checked((long)group.Progress.BytesDone),
                    }
                    : null;
                MenuFlyout? menu = group.ReleaseId is string releaseId
                    ? CancelFlyout(handle => NativeBae.CancelReleaseTransition(handle, releaseId))
                    : null;
                AddOutboxRow(group.DisplayTitle, progress, trailing: null, contextMenu: menu);
            }
            // A pending delete is genuinely a single-file operation, so it keeps
            // its own per-file cancel button.
            foreach (var delete in snapshot.Deletes)
            {
                var cancel = new Button { Content = Loc.Chrome("outbox.cancel_item") };
                var id = delete.Id;
                cancel.Click += async (_, _) => await RunCancel(
                    handle => NativeBae.CancelOutboxItem(handle, id));
                AddOutboxRow(DeleteLabel(delete), null, trailing: cancel, contextMenu: null);
            }
        }

        await LoadDownloads();
        await LoadOutbox();

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(storageStatus);
        content.Children.Add(downloadsPanel);
        content.Children.Add(outboxPanel);
        content.Children.Add(listPanel);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("storage.title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 480 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        // Refresh both the outbox panel and the storage rows live while the dialog
        // is open as uploads/deletes progress. Stops once the dialog closes.
        _refreshOutbox = () =>
        {
            _ = LoadOutbox();
            _ = LoadStorageRows();
        };
        // Refresh the Downloads pane live as pins progress and the storage rows
        // with them (a row's badge/state changes as a pin completes).
        _refreshDownloads = () =>
        {
            _ = LoadDownloads();
            _ = LoadStorageRows();
        };
        // An album/release invalidation that isn't an outbox change can still
        // alter a release's storage state — refresh the rows for it too.
        _refreshStorageRows = () => _ = LoadStorageRows();
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            _refreshOutbox = null;
            _refreshDownloads = null;
            _refreshStorageRows = null;
        }
    }

    // Selected-row highlight: a faint accent tint, or transparent when not
    // selected. Static so LoadStorageRows and RefreshRowHighlights agree.
    private static Brush RowBackground(bool isSelected) =>
        isSelected
            ? new SolidColorBrush(Microsoft.UI.Colors.SteelBlue) { Opacity = 0.25 }
            : new SolidColorBrush(Microsoft.UI.Colors.Transparent);

    // User-facing label for a storage transition, matching the macOS
    // "Storage…" sheet / context menu wording.
    private static string StorageRowSummary(BridgeStorageRow row)
    {
        var format = string.IsNullOrEmpty(row.Release.Format) ? string.Empty : $" · {row.Release.Format}";
        var files = Loc.Chrome("storage.files", "count", row.Release.FileCount);
        return $"{row.Album.Title} — {row.Album.ArtistNames}{format} · {files} · {Loc.Bytes(row.Release.TotalSize)} · {StorageStateLabel(row.Release.StorageState)}{PinIndicator(row.Release)}";
    }

    private static string StorageStateLabel(BridgeReleaseStorageState state) => state switch
    {
        BridgeReleaseStorageState.Remote => Loc.Chrome("storage.state.managed"),
        BridgeReleaseStorageState.Local => Loc.Chrome("storage.state.unmanaged"),
        _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown storage state"),
    };

    private static string PinIndicator(BridgeReleaseSummary release) =>
        release.Pinned ? $" · {Loc.Chrome("storage.pinned")}" : string.Empty;

    private static BridgeDownloadTransferProgress? DownloadProgress(BridgeDownloadState state) =>
        state is BridgeDownloadState.Active active ? active.Progress : null;

    private static string OutboxSummary(BridgeOutboxSnapshot snapshot)
    {
        var parts = new List<string>();
        if (snapshot.Total.Active > 0) parts.Add(Loc.Core("core.queue.uploading", "count", snapshot.Total.Active));
        if (snapshot.Total.Failed > 0) parts.Add(Loc.Core("core.queue.failed", "count", snapshot.Total.Failed));
        if (snapshot.Total.Queued > 0) parts.Add(Loc.Core("core.queue.queued", "count", snapshot.Total.Queued));
        if (snapshot.Deletes.Length > 0)
            parts.Add(Loc.Core("core.outbox.pending_deletes", "count", snapshot.Deletes.Length));
        return string.Join(" · ", parts);
    }

    private static string OutboxThroughputLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.ThroughputBps > 0
            ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(checked((long)snapshot.ThroughputBps)))
            : string.Empty;

    private static string OutboxEtaLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.EtaSeconds is { } seconds
            ? Loc.Core("core.outbox.eta", "duration", Loc.Duration(checked(checked((long)seconds) * 1000)))
            : string.Empty;

    private static string OutboxBytesLabel(BridgeOutboxSnapshot snapshot)
    {
        if (snapshot.Total.BytesTotal == 0) return string.Empty;
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)snapshot.Total.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)snapshot.Total.BytesTotal)),
            });
    }

    private static string DeleteLabel(BridgeDeleteOp delete) =>
        $"{delete.CloudKey} — {Loc.Chrome("outbox.delete.kind")}";

    private static string StorageActionLabel(BridgeReleaseStorageAction action) => action switch
    {
        BridgeReleaseStorageAction.MakeRemote => Loc.Chrome("storage.action.manage"),
        BridgeReleaseStorageAction.MakeLocal => Loc.Chrome("storage.action.unmanage"),
        BridgeReleaseStorageAction.Pin => Loc.Chrome("storage.action.pin"),
        BridgeReleaseStorageAction.Unpin => Loc.Chrome("storage.action.unpin"),
        _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
    };

    // The transitions every release in the selection allows, intersected so the
    // menu only offers actions applicable to all. Order follows the first
    // release's action list (the core's order). The caller suppresses actions
    // when any targeted release has a transition in flight, matching the macOS
    // "Storage…" sheet.
    private static List<BridgeReleaseStorageAction> IntersectedStorageActions(
        List<string> releaseIds, Dictionary<string, BridgeStorageRow> rowsById)
    {
        var perRelease = releaseIds
            .Select(id => rowsById.TryGetValue(id, out var row)
                ? new HashSet<BridgeReleaseStorageAction>(row.Release.StorageActions)
                : new HashSet<BridgeReleaseStorageAction>())
            .ToList();
        if (perRelease.Count == 0)
        {
            return new List<BridgeReleaseStorageAction>();
        }

        var common = perRelease[0];
        foreach (var set in perRelease.Skip(1))
        {
            common.IntersectWith(set);
        }
        // Preserve the core's action order from the first release's row.
        var order = rowsById.TryGetValue(releaseIds[0], out var firstRow)
            ? firstRow.Release.StorageActions
            : [];
        return order.Where(common.Contains).ToList();
    }

    // Build the right-tap menu for the targeted releases: the intersected
    // storage transitions plus a cancel for any of their queued uploads. Each
    // item runs the action on every targeted release, then reloads the rows.
    private async System.Threading.Tasks.Task<MenuFlyout> BuildStorageRowMenu(
        List<string> releaseIds,
        Dictionary<string, BridgeStorageRow> rowsById,
        TextBlock storageStatus,
        Func<System.Threading.Tasks.Task> reload)
    {
        var menu = new MenuFlyout();

        // A release with a transition in flight offers only "Cancel" — the
        // storage actions (pin/unmanage/…) would race it. Each transition
        // surfaces differently: an upload sits in the outbox snapshot, a pin in
        // the download queue snapshot, and an unmanage (a blocking foreground
        // transfer with no queue) is tracked locally while it runs. Core
        // dispatches to whichever is running.
        var transitioning = new HashSet<string>(
            await UploadingReleases(releaseIds, storageStatus));
        transitioning.UnionWith(await DownloadingReleases(releaseIds));
        transitioning.UnionWith(releaseIds.Where(_unmanagingReleases.Contains));
        if (transitioning.Count > 0)
        {
            var cancel = new MenuFlyoutItem { Text = Loc.Chrome("action.cancel") };
            cancel.Click += async (_, _) =>
            {
                foreach (var releaseId in transitioning)
                {
                    var (current, error) = await RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!current)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
                        return;
                    }
                }

                await reload();
            };
            menu.Items.Add(cancel);
            return menu;
        }

        foreach (var action in IntersectedStorageActions(releaseIds, rowsById))
        {
            var act = action;
            var item = new MenuFlyoutItem { Text = StorageActionLabel(act) };
            item.Click += async (_, _) =>
            {
                var error = await RunStorageActionForReleases(act, releaseIds);
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                }
                else
                {
                    await reload();
                }
            };
            menu.Items.Add(item);
        }

        return menu;
    }

    // Of the given releases, those with uploads queued or in flight. Core omits
    // idle releases from the per-release map, so presence there is the signal.
    private async System.Threading.Tasks.Task<List<string>> UploadingReleases(
        List<string> releaseIds,
        TextBlock storageStatus)
    {
        var (current, result) = await RunForCurrentHandle(NativeBae.OutboxSnapshot);
        if (!current)
        {
            return new List<string>();
        }
        if (result.Error is not null)
        {
            storageStatus.Text = result.Error;
            storageStatus.Visibility = Visibility.Visible;
            return new List<string>();
        }
        var snapshot = result.Snapshot;
        if (snapshot is null)
        {
            // Couldn't read the outbox; surface it like the panel load does
            // rather than silently dropping the cancel action.
            storageStatus.Text = Loc.Chrome("outbox.read_failed");
            storageStatus.Visibility = Visibility.Visible;
            return new List<string>();
        }

        return releaseIds.Where(snapshot.PerRelease.ContainsKey).ToList();
    }

    // Of the given releases, those queued or downloading in the pin queue.
    private async System.Threading.Tasks.Task<List<string>> DownloadingReleases(
        List<string> releaseIds)
    {
        var (current, result) = await RunForCurrentHandle(NativeBae.DownloadSnapshot);
        if (!current)
        {
            return new List<string>();
        }
        if (result.Error is not null)
        {
            BaeDiagnostics.Logger.Warning(
                $"couldn't read the download snapshot; pin-cancel unavailable: {result.Error}");
            return new List<string>();
        }
        var snapshot = result.Snapshot;
        if (snapshot is null)
        {
            // The pin queue is in-memory and read is infallible bar a dropped
            // handle, so this is an app-state fault, not a user-facing read
            // error — log it and offer no pin-cancel rather than a toast.
            BaeDiagnostics.Logger.Warning(
                "couldn't read the download snapshot; pin-cancel unavailable");
            return new List<string>();
        }

        var pinning = snapshot.Downloads.Select(op => op.ReleaseId).ToHashSet();
        return releaseIds.Where(pinning.Contains).ToList();
    }

    // Run a storage transition on every release in the selection off the UI
    // thread. "unmanage" asks once for a destination folder, then moves each
    // release into it. Returns null on success (or a cancelled picker), else the
    // first error message.
    private async System.Threading.Tasks.Task<string?> RunStorageActionForReleases(
        BridgeReleaseStorageAction action, List<string> releaseIds)
    {
        if (action == BridgeReleaseStorageAction.MakeLocal)
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                return null;
            }

            var path = folder.Path;
            // Mark these releases as unmanaging so a right-click can cancel them
            // while the blocking transfer runs; cleared when it returns.
            foreach (var releaseId in releaseIds)
            {
                _unmanagingReleases.Add(releaseId);
            }
            try
            {
                var (storageActionCurrent, error) = await RunForCurrentHandle(handle =>
                {
                    foreach (var releaseId in releaseIds)
                    {
                        var error = NativeBae.MakeReleaseLocal(handle, releaseId, path);
                        if (error is not null)
                        {
                            return error;
                        }
                    }

                    return (string?)null;
                });
                return storageActionCurrent ? error : null;
            }
            finally
            {
                foreach (var releaseId in releaseIds)
                {
                    _unmanagingReleases.Remove(releaseId);
                }
            }
        }

        var (current, actionError) = await RunForCurrentHandle(handle =>
        {
            foreach (var releaseId in releaseIds)
            {
                var error = action switch
                {
                    BridgeReleaseStorageAction.Pin => NativeBae.PinRelease(handle, releaseId),
                    BridgeReleaseStorageAction.Unpin => NativeBae.UnpinRelease(handle, releaseId),
                    BridgeReleaseStorageAction.MakeRemote => NativeBae.MakeReleaseRemote(handle, releaseId, pin: false),
                    BridgeReleaseStorageAction.MakeLocal => throw new InvalidOperationException(
                        "make-local storage actions must choose a destination before running"),
                    _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
                };
                if (error is not null)
                {
                    return error;
                }
            }

            return (string?)null;
        });
        return current ? actionError : null;
    }

    private async void OnSettingsClick(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var (current, json) = WithCurrentHandle(NativeBae.SettingsJson);
        if (!current)
        {
            return;
        }
        if (json is null)
        {
            return;
        }

        var s = JsonSerializer.Deserialize<Settings>(json, JsonOptions);
        if (s is null)
        {
            return;
        }

        // Discogs key state machine. The token input is the only local draft state;
        // the configured/valid state comes from generated bridge settings, re-read on
        // a config invalidation. not_configured/rejected → editable input + Save; valid →
        // "connected" + Remove; unvalidated → that label + Re-check + Remove. Save
        // and Re-check validate over the network, so they run off the UI thread and
        // show "Validating…" while in flight.
        //
        // Two text lines: `status` is the persisted state (driven only by
        // RenderDiscogs from the settings re-read, plus the in-flight "Validating…");
        // `settingsErrorText` is local feedback for an action — a rejected key,
        // a settings write failure, a re-check / remove failure — cleared when
        // the next action starts. Keeping them apart means an unrelated
        // a config-invalidation re-render can't wipe the rejection note.
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray) };
        var settingsErrorText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        var tokenBox = new TextBox { PlaceholderText = Loc.Chrome("settings.discogs.token_placeholder") };
        var save = new Button { Content = Loc.Chrome("settings.discogs.save") };
        var recheck = new Button { Content = Loc.Chrome("settings.discogs.recheck") };
        var remove = new Button { Content = Loc.Chrome("settings.discogs.remove") };
        var discogsBusy = false;

        void ShowSettingsError(string message)
        {
            settingsErrorText.Text = message;
            settingsErrorText.Visibility = Visibility.Visible;
        }

        void ClearSettingsError()
        {
            settingsErrorText.Text = string.Empty;
            settingsErrorText.Visibility = Visibility.Collapsed;
        }

        // Drive the controls from the persisted status: which buttons show, whether
        // the input is editable, and the status line. Called on open and on every
        // config-invalidation re-read. The draft text and the local error line are left
        // alone — they belong to the user's in-progress input, not the stored state.
        void RenderDiscogs(Settings settings)
        {
            if (discogsBusy)
            {
                return;
            }

            tokenBox.Visibility = settings.DiscogsConfigured ? Visibility.Collapsed : Visibility.Visible;
            save.Visibility = settings.DiscogsConfigured ? Visibility.Collapsed : Visibility.Visible;
            remove.Visibility = settings.DiscogsConfigured ? Visibility.Visible : Visibility.Collapsed;
            recheck.Visibility = settings.DiscogsNeedsRecheck ? Visibility.Visible : Visibility.Collapsed;
            status.Text = settings.DiscogsConfigured ? settings.DiscogsStatusText : string.Empty;
        }

        save.Click += async (_, _) =>
        {
            var token = tokenBox.Text ?? string.Empty;
            if (string.IsNullOrEmpty(token) || discogsBusy)
            {
                return;
            }

            discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, outcome) = await RunForCurrentHandle(
                handle => NativeBae.SaveDiscogsToken(handle, token));
            discogsBusy = false;
            if (!current)
            {
                return;
            }
            switch (outcome)
            {
                case "valid":
                case "unvalidated":
                    // Stored: a config-invalidation re-read settles the controls and label.
                    status.Text = string.Empty;
                    break;
                case "rejected":
                    // Nothing stored, so no config invalidation fires — keep the draft and
                    // surface the rejection.
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.rejected"));
                    break;
                default:
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.save_failed"));
                    break;
            }
        };
        recheck.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, error) = await RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);
            discogsBusy = false;
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // On success a config-invalidation re-read settles the controls and label.
        };
        remove.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            ClearSettingsError();
            // Removing clears the config flag, firing a config invalidation — the re-read
            // restores the editable input. Nothing is patched inline here.
            var (current, error) = await RunForCurrentHandle(NativeBae.DeleteDiscogsToken);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
        };

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        buttons.Children.Add(save);
        buttons.Children.Add(recheck);
        buttons.Children.Add(remove);

        var syncStatus = new TextBlock { Text = s.SyncStatusText };
        // Two-step disconnect: the first click surfaces the data-loss warning (when
        // releases live only in the cloud) inline and arms; the second confirms.
        // A nested ContentDialog can't open over the settings dialog.
        var disconnect = new Button { Content = Loc.Chrome("settings.sync.disconnect") };
        var disconnectArmed = false;
        disconnect.Click += async (_, _) =>
        {
            if (!disconnectArmed)
            {
                var (warningCurrent, warning) = await RunForCurrentHandle(NativeBae.DisconnectWarning);
                if (!warningCurrent)
                {
                    return;
                }
                if (warning is not null)
                {
                    syncStatus.Text = Loc.Chrome("settings.sync.disconnect_confirm", "warning", warning);
                    disconnectArmed = true;
                    return;
                }
            }

            disconnectArmed = false;
            var (disconnectCurrent, error) = WithCurrentHandle(NativeBae.DisconnectCloud);
            if (!disconnectCurrent)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _refreshSettings?.Invoke();
            }
        };
        var syncNow = new Button { Content = Loc.Chrome("settings.sync.now") };
        syncNow.Click += (_, _) => WithCurrentHandle(NativeBae.TriggerSync);
        var syncButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        syncButtons.Children.Add(disconnect);
        syncButtons.Children.Add(syncNow);

        // Opaque (encrypted) vs browsable (stored in the clear), applied to
        // whichever provider is connected below. Defaults to the secure choice.
        // Not access control — the bucket's own credentials gate it either way.
        var storagePicker = new ComboBox { Header = Loc.Chrome("settings.storage.mode"), SelectedIndex = 0 };
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.opaque"), Tag = "opaque" });
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.browsable"), Tag = "browsable" });
        string SelectedStorage() =>
            (storagePicker.SelectedItem as ComboBoxItem)?.Tag as string ?? "opaque";

        // OAuth providers: signing in runs the browser flow in the core, so it
        // blocks until the user finishes — run it off the UI thread.
        Button CloudButton(string label, string provider)
        {
            var button = new Button { Content = label };
            button.Click += async (_, _) =>
            {
                if (!OAuthCreds.Available)
                {
                    syncStatus.Text = OAuthCreds.RegistrationError
                        ?? Loc.Chrome("cloud.signin.not_configured");
                    return;
                }
                syncStatus.Text = Loc.Chrome("cloud.signin.in_progress", "provider", label);
                var storage = SelectedStorage();
                var (current, error) = await RunForCurrentHandle(
                    handle => NativeBae.SignInCloud(handle, provider, storage));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    syncStatus.Text = error;
                }
                else
                {
                    _refreshSettings?.Invoke();
                }
            };
            return button;
        }

        // Only offer the OAuth providers this build's native library supports.
        // An S3-only build returns just S3, so no sign-in button renders.
        var available = NativeBae.AvailableCloudProviders();
        var oauthButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var wire in new[] { "google_drive", "dropbox", "onedrive" })
        {
            if (available.Contains(wire))
            {
                oauthButtons.Children.Add(CloudButton(ProviderDisplayName(wire), wire));
            }
        }

        var content = new StackPanel { Spacing = 8, MinWidth = 360 };
        var libraryLabel = new TextBlock { Text = Loc.Chrome("settings.library_label", "name", s.LibraryName) };
        var pauseBetweenSides = new CheckBox
        {
            Content = Loc.Chrome("settings.playback.pause_between_sides"),
            IsChecked = s.PauseBetweenSides,
        };
        var refreshingSettings = false;
        async System.Threading.Tasks.Task SetPauseBetweenSides(bool enabled)
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetPauseBetweenSides(handle, enabled));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                refreshingSettings = true;
                pauseBetweenSides.IsChecked = !enabled;
                refreshingSettings = false;
            }
        }

        pauseBetweenSides.Checked += async (_, _) => await SetPauseBetweenSides(true);
        pauseBetweenSides.Unchecked += async (_, _) => await SetPauseBetweenSides(false);

        // Export: the single-track "Save As…" suggested-filename template and the
        // metadata tags an export embeds. Windows has no release export, so this is
        // the per-track section only. Writes round-trip through config invalidation into
        // the settings re-read (RenderExport); the checkboxes send the whole seven-
        // bool set (set-state), never one mutated field.
        var exportLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.export.label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        var exportTemplate = new TextBox
        {
            Header = Loc.Chrome("settings.export.filename_format"),
            Text = s.ExportFilenameTemplate,
        };
        var exportTokensHelp = new TextBlock
        {
            Text = Loc.Chrome("settings.export.tokens_help"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        var defaultTrackExport = new ComboBox { Header = Loc.Chrome("settings.export.default_track_format") };
        var defaultReleaseExport = new ComboBox { Header = Loc.Chrome("settings.export.default_release_format") };
        var presetPanel = new StackPanel { Spacing = 8 };
        var addPresetButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var addFlacPreset = new Button { Content = Loc.Chrome("settings.export.add_flac") };
        var addMp3Preset = new Button { Content = Loc.Chrome("settings.export.add_mp3") };
        var addOpusPreset = new Button { Content = Loc.Chrome("settings.export.add_opus") };
        var addWavPreset = new Button { Content = Loc.Chrome("settings.export.add_wav") };
        var addAiffPreset = new Button { Content = Loc.Chrome("settings.export.add_aiff") };
        addPresetButtons.Children.Add(addFlacPreset);
        addPresetButtons.Children.Add(addMp3Preset);
        addPresetButtons.Children.Add(addOpusPreset);
        addPresetButtons.Children.Add(addWavPreset);
        addPresetButtons.Children.Add(addAiffPreset);

        void RenderExport(Settings settings)
        {
            if (refreshingSettings)
            {
                return;
            }

            refreshingSettings = true;
            exportTemplate.Text = settings.ExportFilenameTemplate;
            PopulateExportSelection(defaultTrackExport, settings, release: false);
            PopulateExportSelection(defaultReleaseExport, settings, release: true);
            RenderExportPresets(settings);
            refreshingSettings = false;
        }

        void PopulateExportSelection(ComboBox combo, Settings settings, bool release)
        {
            combo.Items.Clear();
            var original = ExportSelection.Original();
            var selected = release ? settings.DefaultReleaseExportSelection : settings.DefaultTrackExportSelection;
            combo.Items.Add(new ComboBoxItem
            {
                Content = Loc.Chrome("track.export.original"),
                Tag = original,
                IsSelected = SameExportSelection(selected, original),
            });
            foreach (var preset in settings.ExportPresets.Where(p => release ? p.AppliesToRelease : p.AppliesToTrack))
            {
                var selection = ExportSelection.Preset(preset.Id);
                combo.Items.Add(new ComboBoxItem
                {
                    Content = preset.Name,
                    Tag = selection,
                    IsSelected = SameExportSelection(selected, selection),
                });
            }
        }

        bool SameExportSelection(ExportSelection a, ExportSelection b) =>
            a.Kind == b.Kind && a.PresetId == b.PresetId;

        string CodecLabel(ExportPresetCodec codec) => codec.Kind switch
        {
            "flac" => "FLAC",
            "mp3" => $"MP3 {codec.BitrateKbps} kbps",
            "opus_ogg" => $"Opus {codec.BitrateKbps} kbps",
            "wav" => "WAV",
            "aiff" => "AIFF",
            _ => codec.Kind,
        };

        void RenderExportPresets(Settings settings)
        {
            presetPanel.Children.Clear();
            foreach (var preset in settings.ExportPresets)
            {
                var name = new TextBox
                {
                    Header = Loc.Chrome("settings.export.preset_name"),
                    Text = preset.Name,
                };
                var filenameTemplate = new TextBox
                {
                    Header = Loc.Chrome("settings.export.filename_format"),
                    Text = preset.FilenameTemplate,
                };
                var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                var codec = new TextBlock
                {
                    Text = CodecLabel(preset.Codec),
                    VerticalAlignment = VerticalAlignment.Center,
                    MinWidth = 120,
                };
                var track = new CheckBox
                {
                    Content = Loc.Chrome("settings.export.preset_track"),
                    IsChecked = preset.AppliesToTrack,
                };
                var release = new CheckBox
                {
                    Content = Loc.Chrome("settings.export.preset_release"),
                    IsChecked = preset.AppliesToRelease,
                };
                row.Children.Add(codec);
                row.Children.Add(track);
                row.Children.Add(release);
                var codecEditor = BuildPresetCodecEditor(preset);

                var pregap = new ComboBox { Header = Loc.Chrome("settings.export.preset_pregap") };
                foreach (var item in ExportPregapChoices(preset.Codec))
                {
                    pregap.Items.Add(new ComboBoxItem
                    {
                        Content = item.Label,
                        Tag = item.Value,
                        IsSelected = preset.PregapPlacement == item.Value,
                    });
                }
                void ApplyPregapApplicability()
                {
                    var singleFileCue = pregap.SelectedItem is ComboBoxItem selected
                        && selected.Tag is string placement
                        && placement == "single_file_with_cue";
                    if (singleFileCue)
                    {
                        track.IsChecked = false;
                        release.IsChecked = true;
                    }
                    track.IsEnabled = !singleFileCue;
                    release.IsEnabled = !singleFileCue;
                }
                pregap.SelectionChanged += (_, _) => ApplyPregapApplicability();
                ApplyPregapApplicability();
                var save = new Button { Content = Loc.Chrome("action.save") };
                var remove = new Button { Content = Loc.Chrome("action.remove") };
                var editor = new StackPanel { Spacing = 6 };
                editor.Children.Add(name);
                editor.Children.Add(filenameTemplate);
                editor.Children.Add(row);
                editor.Children.Add(codecEditor.View);
                editor.Children.Add(pregap);
                var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                buttons.Children.Add(save);
                buttons.Children.Add(remove);
                editor.Children.Add(buttons);
                presetPanel.Children.Add(editor);

                save.Click += async (_, _) =>
                {
                    preset.Name = name.Text ?? string.Empty;
                    if (filenameTemplate.Text is not string template)
                    {
                        ShowSettingsError(Loc.Chrome("settings.export.save_failed"));
                        return;
                    }
                    preset.FilenameTemplate = template;
                    preset.AppliesToTrack = track.IsChecked == true;
                    preset.AppliesToRelease = release.IsChecked == true;
                    if (pregap.SelectedItem is ComboBoxItem selected && selected.Tag is string placement)
                    {
                        preset.PregapPlacement = placement;
                    }
                    codecEditor.Apply();
                    await SaveExportPresets(settings.ExportPresets);
                };
                remove.Click += async (_, _) =>
                {
                    settings.ExportPresets.Remove(preset);
                    await SaveExportPresets(settings.ExportPresets);
                };
            }
        }

        (StackPanel View, Action Apply) BuildPresetCodecEditor(ExportPreset preset)
        {
            var panel = new StackPanel { Spacing = 6 };
            switch (preset.Codec.Kind)
            {
                case "flac":
                case "wav":
                case "aiff":
                    var bitDepth = new ComboBox { Header = Loc.Chrome("settings.export.bit_depth_label") };
                    foreach (var item in ExportBitDepthChoices())
                    {
                        bitDepth.Items.Add(new ComboBoxItem
                        {
                            Content = item.Label,
                            Tag = item.Value,
                            IsSelected = preset.Codec.BitDepth == item.Value,
                        });
                    }
                    panel.Children.Add(bitDepth);
                    return (
                        panel,
                        () =>
                        {
                            if (bitDepth.SelectedItem is ComboBoxItem selected && selected.Tag is string selectedBitDepth)
                            {
                                preset.Codec.BitDepth = selectedBitDepth;
                            }
                        }
                    );
                case "mp3":
                case "opus_ogg":
                    var bitrate = new TextBox
                    {
                        Header = Loc.Chrome("settings.export.bitrate"),
                        Text = preset.Codec.BitrateKbps.ToString(CultureInfo.InvariantCulture),
                    };
                    panel.Children.Add(bitrate);
                    return (
                        panel,
                        () =>
                        {
                            if (uint.TryParse(bitrate.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var bitrateKbps))
                            {
                                preset.Codec.BitrateKbps = bitrateKbps;
                                return;
                            }
                            preset.Codec.BitrateKbps = 0;
                        }
                    );
                default:
                    return (panel, () => { });
            }
        }

        List<(string Label, string Value)> ExportPregapChoices(ExportPresetCodec codec)
        {
            var choices = new List<(string Label, string Value)>
            {
                (
                    Loc.Chrome("settings.export.pregap.append_except_htoa"),
                    "append_to_previous_except_htoa"
                ),
                (
                    Loc.Chrome("settings.export.pregap.append_including_htoa"),
                    "append_to_previous_including_htoa"
                ),
                (Loc.Chrome("settings.export.pregap.exclude"), "exclude"),
            };

            if (ExportCodecSupportsSingleFileCue(codec))
            {
                choices.Add((
                    Loc.Chrome("settings.export.pregap.single_file_with_cue"),
                    "single_file_with_cue"
                ));
            }

            return choices;
        }

        static bool ExportCodecSupportsSingleFileCue(ExportPresetCodec codec) =>
            codec.Kind != "opus_ogg";

        List<(string Label, string Value)> ExportBitDepthChoices() => new()
        {
            (Loc.Chrome("settings.export.bit_depth.source"), "source"),
            (Loc.Chrome("settings.export.bit_depth.bits16"), "bits16"),
            (Loc.Chrome("settings.export.bit_depth.bits24"), "bits24"),
            (Loc.Chrome("settings.export.bit_depth.bits32"), "bits32"),
        };

        async System.Threading.Tasks.Task SaveExportTemplate()
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var template = exportTemplate.Text ?? string.Empty;
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetExportFilenameTemplate(handle, template));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // On success a config-invalidation re-read settles the field via RenderExport.
        }

        async System.Threading.Tasks.Task SaveExportPresets(List<ExportPreset> presets)
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var json = JsonSerializer.Serialize(presets, JsonOptions);
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetExportPresets(handle, json));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _refreshSettings?.Invoke();
            }
        }

        async System.Threading.Tasks.Task SaveDefaultExportSelection(ComboBox combo, bool release)
        {
            if (refreshingSettings || combo.SelectedItem is not ComboBoxItem item || item.Tag is not ExportSelection selection)
            {
                return;
            }

            ClearSettingsError();
            var json = JsonSerializer.Serialize(selection, JsonOptions);
            var (current, error) = await RunForCurrentHandle(handle => release
                ? NativeBae.SetDefaultReleaseExportSelection(handle, json)
                : NativeBae.SetDefaultTrackExportSelection(handle, json));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _refreshSettings?.Invoke();
            }
        }

        ExportPreset MakeExportPreset(string kind)
        {
            var codec = kind switch
            {
                "mp3" => new ExportPresetCodec { Kind = "mp3", BitrateKbps = 320 },
                "opus_ogg" => new ExportPresetCodec { Kind = "opus_ogg", BitrateKbps = 192 },
                "wav" => new ExportPresetCodec { Kind = "wav", BitDepth = "source" },
                "aiff" => new ExportPresetCodec { Kind = "aiff", BitDepth = "source" },
                _ => new ExportPresetCodec { Kind = "flac", BitDepth = "source" },
            };
            var extension = kind switch
            {
                "mp3" => "mp3",
                "opus_ogg" => "ogg",
                "wav" => "wav",
                "aiff" => "aiff",
                _ => "flac",
            };
            var label = kind switch
            {
                "mp3" => "MP3",
                "opus_ogg" => "Opus",
                "wav" => "WAV",
                "aiff" => "AIFF",
                _ => "FLAC",
            };
            return new ExportPreset
            {
                Id = Guid.NewGuid().ToString("N"),
                Name = label,
                Codec = codec,
                Extension = extension,
                FilenameTemplate = exportTemplate.Text ?? string.Empty,
                PregapPlacement = "append_to_previous_except_htoa",
                AppliesToTrack = true,
                AppliesToRelease = true,
            };
        }

        exportTemplate.LostFocus += async (_, _) => await SaveExportTemplate();
        exportTemplate.KeyDown += async (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                await SaveExportTemplate();
            }
        };
        defaultTrackExport.SelectionChanged += async (_, _) =>
            await SaveDefaultExportSelection(defaultTrackExport, release: false);
        defaultReleaseExport.SelectionChanged += async (_, _) =>
            await SaveDefaultExportSelection(defaultReleaseExport, release: true);
        addFlacPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("flac"));
            await SaveExportPresets(s.ExportPresets);
        };
        addMp3Preset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("mp3"));
            await SaveExportPresets(s.ExportPresets);
        };
        addOpusPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("opus_ogg"));
            await SaveExportPresets(s.ExportPresets);
        };
        addWavPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("wav"));
            await SaveExportPresets(s.ExportPresets);
        };
        addAiffPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("aiff"));
            await SaveExportPresets(s.ExportPresets);
        };

        var automationLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.automation.label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        var mcpEnabled = new CheckBox
        {
            Content = Loc.Chrome("settings.automation.enable_mcp"),
            IsChecked = s.McpEnabled,
        };
        var mcpPort = new TextBox
        {
            Header = Loc.Chrome("settings.automation.port"),
            Text = s.McpPort.ToString(CultureInfo.InvariantCulture),
        };
        var mcpStatus = new TextBlock
        {
            Text = s.McpStatusText,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        var saveMcp = new Button { Content = Loc.Chrome("action.save") };
        var refreshMcp = new Button { Content = Loc.Chrome("settings.automation.refresh") };
        var copyMcpToken = new Button { Content = Loc.Chrome("settings.automation.copy_token") };
        var rotateMcpToken = new Button { Content = Loc.Chrome("settings.automation.rotate_token") };
        var mcpButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        mcpButtons.Children.Add(saveMcp);
        mcpButtons.Children.Add(refreshMcp);
        mcpButtons.Children.Add(copyMcpToken);
        mcpButtons.Children.Add(rotateMcpToken);

        void RenderMcp(Settings settings)
        {
            if (refreshingSettings)
            {
                return;
            }

            refreshingSettings = true;
            mcpEnabled.IsChecked = settings.McpEnabled;
            mcpPort.Text = settings.McpPort.ToString(CultureInfo.InvariantCulture);
            mcpStatus.Text = settings.McpStatusText;
            refreshingSettings = false;
        }

        async System.Threading.Tasks.Task SetMcpConfig(bool enabled)
        {
            if (refreshingSettings)
            {
                return;
            }

            if (!ushort.TryParse(mcpPort.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var port) || port == 0)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.invalid_port"));
                return;
            }

            ClearSettingsError();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetMcpServerConfig(handle, enabled, port));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                refreshingSettings = true;
                mcpEnabled.IsChecked = !enabled;
                refreshingSettings = false;
                return;
            }
            _refreshSettings?.Invoke();
        }

        async System.Threading.Tasks.Task RefreshMcpStatus()
        {
            var (current, json) = await RunForCurrentHandle(NativeBae.McpServerStatusJson);
            if (!current)
            {
                return;
            }
            var parsed = json is null ? null : JsonSerializer.Deserialize<McpServerStatus>(json, JsonOptions);
            if (parsed is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.status_unavailable"));
                return;
            }
            mcpStatus.Text = new Settings { McpStatus = parsed }.McpStatusText;
        }

        async System.Threading.Tasks.Task CopyMcpToken(Func<AppHandle, string?> readToken, string successKey)
        {
            ClearSettingsError();
            var (current, token) = await RunForCurrentHandle(readToken);
            if (!current)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome(successKey);
        }

        mcpEnabled.Checked += async (_, _) => await SetMcpConfig(true);
        mcpEnabled.Unchecked += async (_, _) => await SetMcpConfig(false);
        saveMcp.Click += async (_, _) => await SetMcpConfig(mcpEnabled.IsChecked == true);
        refreshMcp.Click += async (_, _) => await RefreshMcpStatus();
        copyMcpToken.Click += async (_, _) =>
            await CopyMcpToken(NativeBae.GetMcpToken, "settings.automation.token_copied");
        rotateMcpToken.Click += async (_, _) =>
        {
            ClearSettingsError();
            var (tokenCurrent, token) = await RunForCurrentHandle(NativeBae.GenerateMcpToken);
            if (!tokenCurrent)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetMcpToken(handle, token));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }
            CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome("settings.automation.token_rotated");
        };

        var discogsLabel = new TextBlock { Text = Loc.Chrome("settings.discogs.label") };
        content.Children.Add(libraryLabel);
        content.Children.Add(pauseBetweenSides);
        content.Children.Add(exportLabel);
        content.Children.Add(exportTemplate);
        content.Children.Add(exportTokensHelp);
        content.Children.Add(defaultTrackExport);
        content.Children.Add(defaultReleaseExport);
        content.Children.Add(new TextBlock { Text = Loc.Chrome("settings.export.presets"), FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        content.Children.Add(addPresetButtons);
        content.Children.Add(presetPanel);
        content.Children.Add(automationLabel);
        content.Children.Add(mcpEnabled);
        content.Children.Add(mcpPort);
        content.Children.Add(mcpButtons);
        content.Children.Add(mcpStatus);
        content.Children.Add(discogsLabel);
        content.Children.Add(tokenBox);
        content.Children.Add(buttons);
        content.Children.Add(status);
        content.Children.Add(settingsErrorText);
        RenderDiscogs(s);
        // S3-compatible provider form. The core probes the bucket before saving.
        var s3Bucket = new TextBox { Header = Loc.Chrome("s3.field.bucket") };
        var s3Region = new TextBox { Header = Loc.Chrome("s3.field.region") };
        var s3Endpoint = new TextBox { Header = Loc.Chrome("s3.field.endpoint") };
        var s3KeyPrefix = new TextBox { Header = Loc.Chrome("s3.field.key_prefix") };
        var s3AccessKey = new TextBox { Header = Loc.Chrome("s3.field.access_key") };
        var s3SecretKey = new PasswordBox { Header = Loc.Chrome("s3.field.secret_key") };
        var connectS3 = new Button { Content = Loc.Chrome("settings.s3.connect") };
        connectS3.Click += async (_, _) =>
        {
            syncStatus.Text = Loc.Chrome("settings.s3.connecting");
            var storage = SelectedStorage();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SaveSyncConfig(
                    handle,
                    s3Bucket.Text ?? string.Empty,
                    s3Region.Text ?? string.Empty,
                    s3Endpoint.Text ?? string.Empty,
                    s3KeyPrefix.Text ?? string.Empty,
                    s3AccessKey.Text ?? string.Empty,
                    s3SecretKey.Password ?? string.Empty,
                    storage));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _refreshSettings?.Invoke();
            }
        };
        var s3Form = new StackPanel { Spacing = 6 };
        s3Form.Children.Add(s3Bucket);
        s3Form.Children.Add(s3Region);
        s3Form.Children.Add(s3Endpoint);
        s3Form.Children.Add(s3KeyPrefix);
        s3Form.Children.Add(s3AccessKey);
        s3Form.Children.Add(s3SecretKey);
        s3Form.Children.Add(connectS3);

        content.Children.Add(syncStatus);
        content.Children.Add(syncButtons);
        content.Children.Add(storagePicker);
        content.Children.Add(oauthButtons);
        content.Children.Add(s3Form);

        // Devices (membership): list the library's devices with their role and a
        // "this device" marker. The owner can add a device (which opens the
        // approve flow) or remove one (which rotates the library key). The list
        // loads off the UI thread; the add-device button only renders for an owner.
        var addDeviceRequested = false;
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("members.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var membersHost = new StackPanel { Spacing = 8 };
        membersHost.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        content.Children.Add(membersHost);

        // Recovery: the restore code is now a recovery secret only — it restores
        // this library on a new device when there's no other device available to
        // approve it. Anyone with it has full access, so it's revealed on demand,
        // behind a warning, never shown by default.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.recovery.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.recovery.intro"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        var recoveryCode = new TextBox
        {
            Header = Loc.Chrome("settings.recovery.label"),
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            Visibility = Visibility.Collapsed,
        };
        var showRecoveryCode = new Button { Content = Loc.Chrome("settings.recovery.show") };
        showRecoveryCode.Click += async (_, _) =>
        {
            var (current, code) = await RunForCurrentHandle(NativeBae.GenerateRestoreCode);
            if (!current)
            {
                return;
            }
            recoveryCode.Text = code ?? Loc.Chrome("settings.recovery.unavailable");
            recoveryCode.Visibility = Visibility.Visible;
        };
        content.Children.Add(showRecoveryCode);
        content.Children.Add(recoveryCode);

        // Lock this library: forget its encryption key on this device. Sync stops
        // and the library reopens to the unlock prompt; local files stay.
        var lockRequested = false;
        var lockButton = new Button { Content = Loc.Chrome("settings.lock_library") };
        content.Children.Add(lockButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.title"),
            Content = new ScrollViewer { Content = content },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        lockButton.Click += (_, _) =>
        {
            lockRequested = true;
            dialog.Hide();
        };

        // Now that the dialog exists, load the device list into its placeholder.
        // The add-device button (owner-only) arms the approve flow and closes the
        // settings dialog — a nested ContentDialog can't open over it, so the
        // approve flow runs after this one returns (mirroring the lock dance).
        _ = LoadMembersInto(membersHost, () =>
        {
            addDeviceRequested = true;
            dialog.Hide();
        });

        // Re-read the (generated bridge-pre-computed) settings into the live labels so a
        // config invalidation — or a connect/disconnect in this dialog — updates
        // them in place instead of requiring a reopen.
        _refreshSettings = () =>
        {
            var (current, freshJson) = WithCurrentHandle(NativeBae.SettingsJson);
            if (!current)
            {
                return;
            }
            if (freshJson is null)
            {
                return;
            }

            var fresh = JsonSerializer.Deserialize<Settings>(freshJson, JsonOptions);
            if (fresh is null)
            {
                return;
            }

            syncStatus.Text = fresh.SyncStatusText;
            libraryLabel.Text = Loc.Chrome("settings.library_label", "name", fresh.LibraryName);
            refreshingSettings = true;
            pauseBetweenSides.IsChecked = fresh.PauseBetweenSides;
            refreshingSettings = false;
            RenderExport(fresh);
            RenderMcp(fresh);
            RenderDiscogs(fresh);
        };

        // A key saved while offline lands "unvalidated"; opening settings is a
        // chance to settle it now that there may be connectivity. The core no-ops
        // unless the stored key is actually unvalidated, so call unconditionally;
        // on a result it changes the status, firing a config invalidation.
        _ = RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);

        await dialog.ShowAsync();
        _refreshSettings = null;
        if (lockRequested)
        {
            var (lockCurrent, error) = await RunForCurrentHandle(NativeBae.LockActiveLibrary);
            if (!lockCurrent)
            {
                return;
            }
            if (error is not null)
            {
                StatusText.Text = error;
                return;
            }

            // The key is forgotten now, so re-opening lands on the unlock prompt.
            await ShutdownAndFreeCurrentHandle();

            OpenLibrary(s.LibraryId);
            return;
        }

        // Add-a-device closed settings to open the approve flow (no nested
        // dialogs). Run it, then reopen settings so the refreshed device list
        // shows the newly-approved device.
        if (addDeviceRequested)
        {
            await ShowApproveDevice();
            OnSettingsClick(sender, e);
        }
    }

    // Load the library's devices into a host panel: one row per device (short
    // fingerprint + role + "this device" marker), and — for an owner — an
    // "Add a device…" button plus a Remove control on each other device. Runs the
    // blocking generated bridge off the UI thread. <paramref name="onAddDevice"/> arms the
    // approve flow (which the caller runs once the settings dialog closes).
    private async System.Threading.Tasks.Task LoadMembersInto(StackPanel host, Action onAddDevice)
    {
        var (current, result) = await RunForCurrentHandle(NativeBae.GetMembers);
        if (!current)
        {
            return;
        }
        host.Children.Clear();

        if (result.Error is not null)
        {
            host.Children.Add(new TextBlock
            {
                Text = result.Error,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        var membership = result.Membership;
        if (membership is null)
        {
            host.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.load_failed"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        foreach (var member in membership.Members)
        {
            host.Children.Add(BuildMemberRow(member, host, onAddDevice));
        }

        if (membership.SelfIsOwner)
        {
            var add = new Button { Content = Loc.Chrome("members.add") };
            add.Click += (_, _) => onAddDevice();
            host.Children.Add(add);
        }
    }

    // One device row: fingerprint + role badge + "this device" marker, plus a
    // two-step Remove for the owner on every other device. Removing rotates the
    // library key, so it confirms inline (a second click) — a nested ContentDialog
    // can't open over the settings dialog.
    private FrameworkElement BuildMemberRow(BridgeMember member, StackPanel host, Action onAddDevice)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };

        var labels = new StackPanel { Spacing = 0 };
        labels.Children.Add(new TextBlock
        {
            Text = member.Fingerprint,
            FontFamily = new FontFamily("Consolas"),
        });
        if (member.IsSelf)
        {
            labels.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.this_device"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
        }
        row.Children.Add(labels);

        row.Children.Add(new TextBlock
        {
            Text = MemberFormat.RoleLabel(member.Role),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            VerticalAlignment = VerticalAlignment.Center,
        });

        // The owner can remove any device but its own.
        if (member.CanRemove)
        {
            var remove = new Button { Content = Loc.Chrome("members.remove") };
            var status = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            var armed = false;
            remove.Click += async (_, _) =>
            {
                if (!armed)
                {
                    status.Text = Loc.Chrome("members.remove_confirm");
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                    status.Visibility = Visibility.Visible;
                    armed = true;
                    return;
                }

                remove.IsEnabled = false;
                var pubkey = member.Pubkey;
                var (current, error) = await RunForCurrentHandle(
                    handle => NativeBae.RemoveMember(handle, pubkey));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    status.Visibility = Visibility.Visible;
                    remove.IsEnabled = true;
                    armed = false;
                    return;
                }

                // Reload the list in place so the removed device disappears.
                await LoadMembersInto(host, onAddDevice);
            };
            row.Children.Add(remove);

            var rowWithStatus = new StackPanel { Spacing = 4 };
            rowWithStatus.Children.Add(row);
            rowWithStatus.Children.Add(status);
            return rowWithStatus;
        }

        return row;
    }

    private async void OnClosed(object sender, WindowEventArgs args)
    {
        if (CurrentHandleOrNull() != null)
        {
            // Persist the queue / current track / position before freeing the
            // handle, so the next launch can restore where playback left off.
            await ShutdownAndFreeCurrentHandle();
        }
        BaeDiagnostics.Flush();
    }
}
