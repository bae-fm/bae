using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
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
    // time; Field is the locale-free sort identifier the FFI expects (never
    // localized).
    private sealed record SortOption(string LabelKey, string Field, bool Ascending);

    private static readonly SortOption[] SortOptions =
    {
        new("sort.newest", "date_added", false),
        new("sort.title", "title", true),
        new("sort.artist", "artist", true),
        new("sort.year", "year", true),
    };

    private SortOption _sort = SortOptions[0];

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };

    private IntPtr _handle;

    // Held for the subscription's lifetime so the GC doesn't collect the delegate
    // while native code holds a pointer to it.
    private NativeBae.EventCallback? _eventCallback;

    // Latest queue snapshot from QueueUpdated; the queue dialog reads it on open.
    private List<QueueItem> _queue = new();

    // Holds the +N queue badge visible for ~1.4s after the last add; a fresh add
    // restarts it, replacing the count and resetting the timer.
    private DispatcherTimer? _queueBadgeTimer;

    // Scan candidates, populated live from CandidateAdded events while the import
    // dialog is open and bound to its list.
    private readonly ObservableCollection<ImportCandidate> _candidates = new();

    // The import dialog's status line, set while it's open so ScanFinished can
    // update it; null when the dialog is closed.
    private TextBlock? _scanStatus;

    // Reloads the storage dialog's outbox panel and storage rows; set while that
    // dialog is open so OutboxChanged refreshes them live, null when closed.
    private Action? _refreshOutbox;

    // Re-reads bae_settings into the open settings dialog's labels; set while that
    // dialog is open so ConfigChanged (provider connect/disconnect, sync readiness,
    // rename, Discogs token) refreshes it live, null when closed.
    private Action? _refreshSettings;

    // Reloads the storage dialog's release rows; set while that dialog is open so a
    // ReleaseUpdated (mapped to LibraryChanged) — a storage-state change pulled in
    // by sync, or an async manage→cloud-only that finishes when its uploads land —
    // refreshes each row's state badge and actions, not just the album grid.
    private Action? _refreshStorageRows;

    // Toolbar sync indicator state, accumulated from sync events. The last-sync time
    // and syncing flag arrive only via events (as on macOS), so the indicator stays
    // blank until the first sync activity after a library opens.
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

    // Suppresses the volume slider's ValueChanged while we set it programmatically
    // (seeding + VolumeChanged events), so it doesn't echo back as a SetVolume.
    private bool _suppressVolume;

    // The album and track of whatever is currently playing, tracked from the
    // playback events so "go to now playing" (Ctrl+L) can open that album and
    // reveal the track. Null when nothing is playing.
    private string? _nowPlayingAlbumId;
    private string? _nowPlayingTrackId;

    public ObservableCollection<Album> Albums { get; } = new();

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

        foreach (var option in SortOptions)
        {
            SortBox.Items.Add(Loc.Chrome(option.LabelKey));
        }
        SortBox.SelectedIndex = 0;

        // Seek on release, not on every drag tick. The Slider handles pointer
        // events internally, so register with handledEventsToo.
        NpProgress.AddHandler(UIElement.PointerPressedEvent,
            new PointerEventHandler((_, _) => _userSeeking = true), true);
        NpProgress.AddHandler(UIElement.PointerReleasedEvent,
            new PointerEventHandler((_, _) =>
            {
                _userSeeking = false;
                if (_handle != IntPtr.Zero)
                {
                    NativeBae.SeekByRatio(_handle, NpProgress.Value);
                }
            }), true);

        LoadLibrary();
    }

    private void OnSortChanged(object sender, SelectionChangedEventArgs e)
    {
        if (SortBox.SelectedIndex < 0)
        {
            return;
        }

        _sort = SortOptions[SortBox.SelectedIndex];

        // Sort drives the full-library view; search results keep their relevance
        // order. Reload only when no search is active and the library is open.
        if (_handle != IntPtr.Zero && string.IsNullOrEmpty(SearchBox.Text))
        {
            SetAlbums(NativeBae.AlbumPageJson(_handle, 0, FirstPageSize, _sort.Field, _sort.Ascending), Loc.Chrome("library.empty"));
        }
    }

    // Create a new library, reporting failure through the caller's surface (the
    // welcome status line, or the library manager's). Returns the new id, or null
    // on failure. Callers diverge only in how they open it.
    private string? CreateLibraryOrReport(Action<string> reportError)
    {
        var id = NativeBae.CreateLibrary();
        if (id is null)
        {
            reportError(Loc.Chrome("library.create_failed"));
        }

        return id;
    }

    // The libraries discovered on this device. Empty when discovery fails or none
    // exist; callers pick the active one, or list them.
    private List<Library> LoadLibraries()
    {
        var json = NativeBae.LibrariesJson();
        return json is null
            ? new List<Library>()
            : JsonSerializer.Deserialize<List<Library>>(json, JsonOptions) ?? new List<Library>();
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
        _handle = NativeBae.Init(libraryId, PositionUpdateIntervalMs);
        if (_handle == IntPtr.Zero)
        {
            StatusText.Text = Loc.Chrome("library.open_failed");
            return;
        }

        if (!NativeBae.HasEncryptionKey(_handle))
        {
            // Encrypted library whose key isn't on this device: the handle works
            // locally but sync is deferred. Free it and prompt for the key rather
            // than show a half-open library; unlocking re-opens with sync online.
            NativeBae.HandleFree(_handle);
            _handle = IntPtr.Zero;
            _ = ShowUnlock(libraryId);
            return;
        }

        // Committed to showing this library: drop the welcome chooser if it's up.
        // Done here (not at the call sites) so a failed open or an unlock detour
        // above leaves the welcome in place rather than stranding the user.
        DismissWelcome();

        SetAlbums(NativeBae.AlbumPageJson(_handle, 0, FirstPageSize, _sort.Field, _sort.Ascending), Loc.Chrome("library.empty"));

        _suppressVolume = true;
        NpVolume.Value = NativeBae.GetVolume(_handle);
        _suppressVolume = false;

        // Reset the toolbar sync indicator for the newly-opened library; it
        // repopulates from this library's own sync events.
        ResetSyncIndicator();

        _eventCallback = OnNativeEvent;
        NativeBae.Subscribe(_handle, _eventCallback);
    }

    // Clear the toolbar sync indicator back to blank — on library open (the
    // next library repopulates it from its own sync events) and on teardown
    // (no library, nothing to report).
    private void ResetSyncIndicator()
    {
        _syncing = false;
        _lastSyncTime = null;
        _syncErrorText = null;
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
    // down its handle and view state, then open the target. bae_init writes the
    // target as the new active library; a locked target lands on the unlock
    // prompt. Used for switching to an existing library and for a freshly
    // created one.
    private void SwitchLibrary(string libraryId)
    {
        TearDownLibrary();
        OpenLibrary(libraryId);
    }

    // Tear down the open library: shut down and free its handle, and reset every
    // piece of per-library view state so nothing from it bleeds into the next
    // library or the welcome chooser. Leaves the window with no library open.
    private void TearDownLibrary()
    {
        if (_handle != IntPtr.Zero)
        {
            NativeBae.Shutdown(_handle);
            NativeBae.HandleFree(_handle);
            _handle = IntPtr.Zero;
        }

        _eventCallback = null;
        _queue = new List<QueueItem>();
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
    private void CloseLibrary()
    {
        if (_handle == IntPtr.Zero)
        {
            return;
        }

        TearDownLibrary();
        ShowWelcome();
    }

    private void OnCloseLibraryClick(object sender, RoutedEventArgs e)
    {
        CloseLibrary();
    }

    // The library manager: switch between the libraries on this device, or add a
    // new one. Restore-from-code lives only in the first-run flow. Once a library
    // is open the first-run flow never shows, so this is the only way to reach
    // other libraries or create another.
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
            switchButton.Click += (_, _) =>
            {
                dialog.Hide();
                SwitchLibrary(id);
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

                var error = NativeBae.RenameLibrary(_handle, id, newName);
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
        newButton.Click += (_, _) =>
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
            SwitchLibrary(newId);
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

        var infoJson = NativeBae.DecodeRestoreCode(code);
        var info = infoJson is null ? null : JsonSerializer.Deserialize<RestoreCodeInfo>(infoJson, JsonOptions);
        if (info is null)
        {
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
            // The baeium (S3-only) native library has no OAuth entry points, so a
            // code for an OAuth provider can't be restored here — check the
            // supported set before reaching the (absent) sign-in call.
            if (!NativeBae.AvailableCloudProviders().Contains(info.Provider))
            {
                StatusText.Text = Loc.Chrome("cloud.unsupported_provider", "provider", ProviderDisplayName(info.Provider));
                return;
            }
            if (!OAuthCreds.Available)
            {
                StatusText.Text = OAuthCreds.RegistrationError
                    ?? Loc.Chrome("cloud.signin.not_configured");
                return;
            }
            StatusText.Text = Loc.Chrome("cloud.signin.in_progress", "provider", ProviderDisplayName(info.Provider));
            var oauthJson = await System.Threading.Tasks.Task.Run(() => NativeBae.OAuthAuthorize(info.Provider));
            var oauth = oauthJson is null ? null : JsonSerializer.Deserialize<OAuthResult>(oauthJson, JsonOptions);
            if (oauth?.Token is null)
            {
                StatusText.Text = oauth?.Error ?? Loc.Chrome("cloud.signin.failed");
                return;
            }
            oauthTokenJson = oauth.Token;
        }

        StatusText.Text = Loc.Chrome("restore.in_progress_named", "name", info.LibraryName);
        var resultJson = await System.Threading.Tasks.Task.Run(() => NativeBae.RestoreFromCode(code, oauthTokenJson));
        var result = resultJson is null ? null : JsonSerializer.Deserialize<RestoreResult>(resultJson, JsonOptions);
        if (result?.LibraryId is null)
        {
            StatusText.Text = result?.Error ?? Loc.Chrome("restore.failed");
            return;
        }

        DismissWelcome();
        OpenLibrary(result.LibraryId);
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
            object source = new
            {
                type = "s3",
                bucket = s3Bucket.Text?.Trim() ?? string.Empty,
                region = s3Region.Text?.Trim() ?? string.Empty,
                endpoint = s3Endpoint.Text?.Trim() ?? string.Empty,
                access_key = s3AccessKey.Password?.Trim() ?? string.Empty,
                secret_key = s3SecretKey.Password ?? string.Empty,
            };
            var sourceJson = JsonSerializer.Serialize(source);

            restoreButton.IsEnabled = false;
            status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            status.Text = Loc.Chrome("restore.in_progress");
            status.Visibility = Visibility.Visible;

            var libraryId = libraryIdBox.Text?.Trim() ?? string.Empty;
            var key = keyBox.Password?.Trim() ?? string.Empty;
            var name = nameBox.Text?.Trim() ?? string.Empty;
            var resultJson = await System.Threading.Tasks.Task.Run(
                () => NativeBae.RestoreFromCloud(libraryId, key, name, sourceJson));
            var result = resultJson is null
                ? null
                : JsonSerializer.Deserialize<RestoreResult>(resultJson, JsonOptions);
            if (result?.LibraryId is null)
            {
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                status.Text = result?.Error ?? Loc.Chrome("restore.failed");
                restoreButton.IsEnabled = true;
                return;
            }

            dialog.Hide();
            DismissWelcome();
            OpenLibrary(result.LibraryId);
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

    // Fires on a background thread; copy the JSON and hop to the UI thread.
    private void OnNativeEvent(IntPtr jsonPtr)
    {
        var json = Marshal.PtrToStringUTF8(jsonPtr);
        if (json is null)
        {
            return;
        }

        DispatcherQueue.TryEnqueue(() => HandleEvent(json));
    }

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

    private void HandleEvent(string json)
    {
        var evt = JsonSerializer.Deserialize<BaeEvent>(json, JsonOptions);
        if (evt is null)
        {
            return;
        }

        switch (evt.Type)
        {
            case "PlaybackPlaying":
            case "PlaybackPaused":
                NowPlayingBar.Visibility = Visibility.Visible;
                _nowPlayingAlbumId = evt.AlbumId;
                _nowPlayingTrackId = evt.TrackId;
                NpTitle.Text = evt.TrackTitle ?? string.Empty;
                NpArtist.Text = evt.Artist ?? string.Empty;
                NpPlayPause.Content = evt.Type == "PlaybackPlaying" ? "⏸" : "▶";
                NpCover.Source = LoadCover(evt.CoverImageId);
                // Audio is flowing: drop the buffering spinner, restore the
                // play/pause control.
                NpLoading.IsActive = false;
                NpLoading.Visibility = Visibility.Collapsed;
                NpPlayPause.Visibility = Visibility.Visible;
                break;
            case "PlaybackStopped":
                NowPlayingBar.Visibility = Visibility.Collapsed;
                _nowPlayingAlbumId = null;
                _nowPlayingTrackId = null;
                NpLoading.IsActive = false;
                NpLoading.Visibility = Visibility.Collapsed;
                NpPlayPause.Visibility = Visibility.Visible;
                break;
            case "PlaybackProgress":
                if (!_userSeeking)
                {
                    NpProgress.Value = evt.Progress;
                }
                NpElapsed.Text = evt.ElapsedLabel;
                NpRemaining.Text = evt.RemainingLabel;
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
                    "album" => "🔁",
                    _ => "↻",
                };
                break;
            case "SyncError":
                // Sync uses its own banner so a general ErrorCleared can't dismiss a
                // still-broken sync (and vice versa), matching the macOS slot split.
                // evt.Error is null when a prior failure cleared; otherwise it's the
                // structured diagnostic whose generic line we render for the locale.
                var syncLine = evt.Error?.LocalizedLine;
                if (syncLine is null)
                {
                    SyncBanner.IsOpen = false;
                }
                else
                {
                    var reconnect = new Button { Content = Loc.Chrome("sync.reconnect") };
                    reconnect.Click += (_, _) => NativeBae.TriggerSync(_handle);
                    SyncBanner.Severity = InfoBarSeverity.Error;
                    SyncBanner.Title = Loc.Chrome("sync.error_title");
                    SyncBanner.Message = syncLine;
                    SyncBanner.ActionButton = reconnect;
                    SyncBanner.IsOpen = true;
                }
                _syncErrorText = syncLine;
                UpdateSyncIndicator();
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
            case "SyncingChanged":
                _syncing = evt.Syncing;
                UpdateSyncIndicator();
                break;
            case "SyncTimeChanged":
                _lastSyncTime = FormatSyncTime(evt.SyncTime);
                UpdateSyncIndicator();
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
                NowPlayingBar.Visibility = Visibility.Visible;
                NpPlayPause.Visibility = Visibility.Collapsed;
                NpLoading.IsActive = true;
                NpLoading.Visibility = Visibility.Visible;
                break;
            case "LibraryChanged":
                if (string.IsNullOrEmpty(SearchBox.Text))
                {
                    SetAlbums(NativeBae.AlbumPageJson(_handle, 0, FirstPageSize, _sort.Field, _sort.Ascending), Loc.Chrome("library.empty"));
                }
                _refreshStorageRows?.Invoke();
                break;
            case "OutboxChanged":
                _refreshOutbox?.Invoke();
                break;
            case "ConfigChanged":
                _refreshSettings?.Invoke();
                break;
            case "QueueUpdated":
                _queue = evt.Items ?? new List<QueueItem>();
                NpPrev.IsEnabled = evt.HasPrevious;
                NpNext.IsEnabled = evt.HasNext;
                break;
            case "QueueItemsAdded":
                if (evt.Count is int added && added > 0)
                {
                    FlashQueueAddBadge(added);
                }
                break;
            case "CandidateAdded":
                _candidates.Add(new ImportCandidate
                {
                    Key = evt.Key ?? string.Empty,
                    Name = evt.Name ?? string.Empty,
                    TrackCount = evt.TrackCount,
                    Format = evt.Format ?? string.Empty,
                    AudioPaths = evt.AudioPaths ?? new List<string>(),
                    FolderPath = evt.Key ?? string.Empty,
                });
                break;
            case "CandidateRemoved":
                var removed = _candidates.FirstOrDefault(candidate => candidate.Key == evt.Key);
                if (removed is not null)
                {
                    _candidates.Remove(removed);
                }
                break;
            case "ScanFinished":
                if (_scanStatus is not null)
                {
                    _scanStatus.Text = _candidates.Count == 0 ? Loc.Chrome("import.no_releases") : string.Empty;
                }
                break;
            case "CandidateIdentifyState":
                UpdateCandidate(evt.Key, existing =>
                {
                    existing.Matches = evt.Matches ?? new List<Candidate>();
                    existing.Signals = evt.Signals ?? new List<SignalBadge>();
                }, DescribeIdentify(evt));
                break;
            case "CandidateImportProgress":
                // The percent is localized as a chrome message; the step (when
                // known) resolves its localized verb from the catalog.
                var stepLabel = evt.Step?.LocalizedLabel;
                var importing = Loc.Chrome("import.progress.percent", "percent", evt.ProgressPercent);
                UpdateCandidate(evt.Key, null,
                    string.IsNullOrEmpty(stepLabel) ? importing : $"{importing} — {stepLabel}");
                break;
            case "CandidateImportComplete":
                UpdateCandidate(evt.Key, null, Loc.Chrome("import.complete"));
                break;
            case "CandidateImportError":
                // The structured diagnostic resolves its generic localized line;
                // the failure word is chrome.
                var failLine = evt.Error?.LocalizedLine;
                UpdateCandidate(evt.Key, null,
                    failLine is null
                        ? Loc.Chrome("import.failed")
                        : $"{Loc.Chrome("import.failed")}: {failLine}");
                break;
        }
    }

    // Replace a candidate row in place (ObservableCollection raises Replace so the
    // bound list re-renders), applying an optional mutation and a new status.
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
            Status = status,
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

    private static string DescribeIdentify(BaeEvent evt) => evt.Status switch
    {
        "identifying" => Loc.Chrome("identify.identifying"),
        "found" => Loc.Chrome("identify.found", "count", evt.Matches?.Count ?? 0),
        "conflict" => Loc.Chrome("identify.conflict"),
        "not_found" => Loc.Chrome("identify.not_found"),
        "manual" => Loc.Chrome("identify.manual"),
        "error" => Loc.Chrome("identify.error"),
        _ => string.Empty,
    };

    private async void OnImportClick(object sender, RoutedEventArgs e)
    {
        await ShowImportDialog();
    }

    // Build and show the import dialog: the folder scan source plus the live
    // candidate list (bound to _candidates, which scan events populate).
    // Shared by the toolbar import button and the folder-drop handler.
    private async System.Threading.Tasks.Task ShowImportDialog()
    {
        if (_handle == IntPtr.Zero)
        {
            return;
        }

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
        // once it has a result, clicking opens the import dialog (auto matches plus
        // a manual-search fallback). The row reflects status as
        // CandidateIdentifyState / CandidateImport* events arrive.
        list.ItemClick += async (_, args) =>
        {
            if (args.ClickedItem is not ImportCandidate candidate)
            {
                return;
            }

            if (string.IsNullOrEmpty(candidate.Status))
            {
                NativeBae.AutoIdentifyFolder(_handle, candidate.Key, candidate.FolderPath);
            }
            else
            {
                await ShowImportPicker(candidate);
            }
        };

        // The picker needs the app window's handle in an unpackaged app. Scanning
        // is fire-and-forget: bae_scan_folder enqueues, candidates stream back as
        // events into _candidates (bound to the list).
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
            var error = await System.Threading.Tasks.Task.Run(() => NativeBae.ScanFolder(_handle, path, true));
            if (error is not null)
            {
                status.Text = error;
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
        if (_handle != IntPtr.Zero && e.DataView.Contains(StandardDataFormats.StorageItems))
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
        if (_handle == IntPtr.Zero || !e.DataView.Contains(StandardDataFormats.StorageItems))
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

        var error = await System.Threading.Tasks.Task.Run(() => NativeBae.ScanFolder(_handle, folderPath, true));
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
    // streams back as CandidateIdentifyState events, so no manual refresh.
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
            if (_handle != IntPtr.Zero)
            {
                System.Threading.Tasks.Task.Run(() =>
                    NativeBae.RerunIdentifyForCandidate(_handle, candidateKey));
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
    // on click); the re-derived toolbar streams back as a CandidateIdentifyState
    // event. Mirrors the macOS SignalBadge anatomy in plain WinUI primitives.
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
            if (_handle != IntPtr.Zero)
            {
                var kind = signal.Kind;
                var value = signal.Value ?? string.Empty;
                System.Threading.Tasks.Task.Run(() =>
                    NativeBae.ToggleSignalForCandidate(_handle, candidateKey, kind, value));
            }
        };
        return badge;
    }

    // The badge's trailing state visual, chosen by the pre-shaped SignalState the
    // FFI carried over. An excluded badge shows the exclusion mark regardless.
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
    // the wire kind names come from the FFI's snake_case mapping, resolved to a
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
            preview.Click += (_, _) => NativeBae.PreviewPlay(_handle, candidate.AudioPaths[0]);
            var pause = new Button { Content = "⏸" };
            pause.Click += (_, _) => NativeBae.PreviewTogglePause(_handle);
            var stop = new Button { Content = "⏹" };
            stop.Click += (_, _) => NativeBae.PreviewStop(_handle);
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
            var json = await System.Threading.Tasks.Task.Run(
                () => NativeBae.SearchReleasesJson(_handle, source, artist, album));
            searchButton.IsEnabled = true;
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
                NativeBae.PreviewStop(_handle);
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
        var json = await System.Threading.Tasks.Task.Run(
            () => NativeBae.PrefetchCandidateEditJson(
                _handle, chosen.ReleaseId, chosen.Source, candidate.FolderPath));
        var prefetched = json is null ? null : JsonSerializer.Deserialize<PrefetchedEdit>(json, JsonOptions);
        if (prefetched is null)
        {
            await ShowError(Loc.Chrome("import.error.load_release"));
            return false;
        }

        var settingsJson = await System.Threading.Tasks.Task.Run(() => NativeBae.SettingsJson(_handle));
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
        var storageManaged = new CheckBox
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
        bool StorageManagedSelected() => storageManaged.IsChecked == true;
        string StorageModeTag() =>
            !settings.HasCloudHome || !StorageManagedSelected()
                ? "unmanaged"
                : storagePinned.IsChecked == true
                    ? "managed_pinned"
                    : "managed_unpinned";

        void RefreshStorageControls()
        {
            storagePinned.Visibility = settings.HasCloudHome && StorageManagedSelected()
                ? Visibility.Visible
                : Visibility.Collapsed;
        }
        storageManaged.Checked += (_, _) => RefreshStorageControls();
        storageManaged.Unchecked += (_, _) => RefreshStorageControls();
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
            storageRow.Children.Add(storageManaged);
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
        var statusJson = await System.Threading.Tasks.Task.Run(
            () => NativeBae.CheckReleaseInLibraryJson(_handle, chosen.ReleaseId, chosen.Source));
        var libraryStatus = statusJson is null
            ? null
            : JsonSerializer.Deserialize<LibraryStatus>(statusJson, JsonOptions);
        if (libraryStatus is not null && libraryStatus.ReleaseInLibrary)
        {
            panel.Children.Insert(0, BuildLibraryStatusBanner(libraryStatus, () =>
            {
                viewInLibraryAlbumId = libraryStatus.AlbumId;
                dialog.Hide();
            }));
        }

        // The import runs in the background; its result updates the candidate row
        // via CandidateImport* events and refreshes the grid via LibraryChanged.
        // Shape (validation) of the edit happens in Rust — on failure keep the
        // dialog open and show the reason.
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var payload = JsonSerializer.Serialize(form.ReadBack(), JsonOptions);
            var storageMode = StorageModeTag();
            var error = await System.Threading.Tasks.Task.Run(
                () => NativeBae.ImportCandidate(
                    _handle, candidate.Key, candidate.FolderPath, chosen.ReleaseId, chosen.Source, storageMode, payload, selectedCoverJson));
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
    /// when the chosen release (<see cref="LibraryStatus.ReleaseInLibrary"/>) is
    /// already in the library. When an album id is present it offers a "view in
    /// library" button that invokes <paramref name="onViewInLibrary"/>.
    /// </summary>
    private static InfoBar BuildLibraryStatusBanner(LibraryStatus status, Action onViewInLibrary)
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
        List<RemoteCover> remoteCovers, List<LocalArtwork> localArtwork, Action<string> onPick)
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
                source = new BitmapImage(new Uri(cover.ThumbnailUrl));
            }
            catch (UriFormatException)
            {
                continue;
            }

            var selection = JsonSerializer.Serialize(
                new { type = "remote_cover", url = cover.Url, source = cover.Source });
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
        if (_handle == IntPtr.Zero)
        {
            return;
        }

        // A live copy so drag-reorder mutates it; the framework raises a Move on
        // the collection, which we forward to the core as a queue reorder. Click
        // (not selection) drives skip-to so a drag doesn't start playback.
        var queueItems = new ObservableCollection<QueueItem>(_queue);
        queueItems.CollectionChanged += (_, args) =>
        {
            if (args.Action == System.Collections.Specialized.NotifyCollectionChangedAction.Move)
            {
                NativeBae.QueueReorder(_handle, (uint)args.OldStartingIndex, (uint)args.NewStartingIndex);
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
            var index = queueItems.IndexOf((QueueItem)args.ClickedItem);
            if (index >= 0)
            {
                NativeBae.QueueSkipTo(_handle, (uint)index);
            }
        };
        // Right-tap a row to drop it from the queue. Removing locally too keeps the
        // open dialog in sync; the Move-only reorder handler ignores this Remove.
        list.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element
                || element.DataContext is not QueueItem item)
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
                // Recompute: a reorder between the right-tap and the click could have
                // moved this item.
                var idx = queueItems.IndexOf(item);
                if (idx < 0)
                {
                    return;
                }
                NativeBae.QueueRemove(_handle, (uint)idx);
                queueItems.RemoveAt(idx);
            };
            menu.Items.Add(remove);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        var clear = new Button { Content = Loc.Chrome("queue.clear") };
        clear.Click += (_, _) => NativeBae.QueueClear(_handle);

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(clear);
        content.Children.Add(list);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("queue.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        await dialog.ShowAsync();
    }

    private ImageSource? LoadCover(string? imageId)
    {
        if (string.IsNullOrEmpty(imageId))
        {
            return null;
        }

        // ImagePath returns the cache-bustable identifier (<path>#v=<mtime>);
        // CoverImage.Load strips the version, opens the file, and bypasses
        // WinUI's URI cache so a changed cover at the same path reloads.
        return CoverImage.Load(NativeBae.ImagePath(_handle, imageId));
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
        if (_handle != IntPtr.Zero)
        {
            NativeBae.PlayPause(_handle);
        }
    }

    private void OnNext(object sender, RoutedEventArgs e)
    {
        if (_handle != IntPtr.Zero)
        {
            NativeBae.Next(_handle);
        }
    }

    private void OnPrevious(object sender, RoutedEventArgs e)
    {
        if (_handle != IntPtr.Zero)
        {
            NativeBae.Previous(_handle);
        }
    }

    private void OnRepeat(object sender, RoutedEventArgs e)
    {
        if (_handle != IntPtr.Zero)
        {
            NativeBae.CycleRepeatMode(_handle);
        }
    }

    private void OnMute(object sender, RoutedEventArgs e)
    {
        if (_handle != IntPtr.Zero)
        {
            NativeBae.ToggleMute(_handle);
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
        var albumId = _nowPlayingAlbumId;
        if (_handle == IntPtr.Zero || string.IsNullOrEmpty(albumId))
        {
            return;
        }

        await ShowAlbumDetail(albumId, scrollToTrackId: _nowPlayingTrackId);
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

        if (_handle != IntPtr.Zero)
        {
            NativeBae.PlayPause(_handle);
            e.Handled = true;
        }
    }

    private void OnVolumeChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (!_suppressVolume && _handle != IntPtr.Zero)
        {
            NativeBae.SetVolume(_handle, (float)NpVolume.Value);
        }
    }

    private void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        if (_handle == IntPtr.Zero)
        {
            return;
        }

        var query = args.QueryText?.Trim() ?? string.Empty;
        if (query.Length == 0)
        {
            SetAlbums(NativeBae.AlbumPageJson(_handle, 0, FirstPageSize, _sort.Field, _sort.Ascending), Loc.Chrome("library.empty"));
        }
        else
        {
            SetAlbums(NativeBae.SearchJson(_handle, query), Loc.Chrome("search.no_matches"));
        }
    }

    /// <summary>Replace the grid's albums from an FFI JSON array (or show a status).</summary>
    private void SetAlbums(string? json, string emptyMessage)
    {
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
            Albums.Add(album);
        }

        StatusText.Text = albums.Count == 0 ? emptyMessage : string.Empty;
    }

    private async void OnAlbumClick(object sender, ItemClickEventArgs e)
    {
        if (_handle == IntPtr.Zero || e.ClickedItem is not Album album)
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
    private async System.Threading.Tasks.Task ShowAlbumDetail(string albumId, string? scrollToTrackId = null)
    {
        var json = NativeBae.AlbumDetailJson(_handle, albumId);
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
        var editButton = new Button { Content = Loc.Chrome("album.edit") };
        var reidentifyButton = new Button { Content = Loc.Chrome("album.reidentify") };
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
                NativeBae.AddReleaseNext(_handle, selectedRelease.ReleaseId);
            }
        };
        var addQueueItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
        addQueueItem.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                NativeBae.AddReleaseToQueue(_handle, selectedRelease.ReleaseId);
            }
        };
        // Set the selected release as the album's primary (canonical) one — only
        // meaningful when the album has more than one release.
        var setPrimaryItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.set_primary") };
        setPrimaryItem.Click += (_, _) =>
        {
            var error = NativeBae.SetPrimaryRelease(_handle, detail.Id, selectedRelease.ReleaseId);
            StatusText.Text = error ?? Loc.Chrome("menu.set_primary_done");
        };
        var changeCoverItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.change_cover") };
        var deleteItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.delete") };
        moreMenu.Items.Add(playNextItem);
        moreMenu.Items.Add(addQueueItem);
        if (detail.Releases.Count > 1)
        {
            moreMenu.Items.Add(setPrimaryItem);
        }
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
        // release's track list, which is what bae_play_release expects.
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
                NativeBae.PlayRelease(_handle, selectedRelease.ReleaseId, selectedRelease.Tracks.IndexOf(track), false);
            }
        }
        void QueueTrack(Track track, bool next)
        {
            var error = next
                ? NativeBae.AddNext(_handle, new[] { track.TrackId })
                : NativeBae.AddToQueue(_handle, new[] { track.TrackId });
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
                // The chosen file type decides the export format; its extension
                // round-trips back to the format string the FFI expects.
                picker.FileTypeChoices.Add(Loc.Chrome("track.export.flac"), new List<string> { ".flac" });
                picker.FileTypeChoices.Add(Loc.Chrome("track.export.mp3"), new List<string> { ".mp3" });
                var invalid = System.IO.Path.GetInvalidFileNameChars();
                var suggested = new string(track.Title.Select(c => invalid.Contains(c) ? '-' : c).ToArray());
                picker.SuggestedFileName = string.IsNullOrWhiteSpace(suggested) ? "track" : suggested;
                var file = await picker.PickSaveFileAsync();
                if (file is null)
                {
                    return;
                }

                var format = file.FileType.Equals(".mp3", StringComparison.OrdinalIgnoreCase) ? "mp3" : "flac";
                var path = file.Path;
                var error = await System.Threading.Tasks.Task.Run(
                    () => NativeBae.ExportTrack(_handle, track.TrackId, path, format));
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
        changeCoverItem.Click += (_, _) =>
        {
            changeCoverRequested = true;
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
                NativeBae.PlayRelease(_handle, selectedRelease.ReleaseId, -1, true);
            }
        }
        else if (changeCoverRequested)
        {
            await ShowChangeCover(detail.Id, selectedRelease.ReleaseId);
        }
        else if (deleteRequested)
        {
            await ConfirmDeleteRelease(selectedRelease.ReleaseId);
        }
        else if (result == ContentDialogResult.Primary && !string.IsNullOrEmpty(selectedRelease.ReleaseId))
        {
            NativeBae.PlayRelease(_handle, selectedRelease.ReleaseId, -1, false);
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

        // On success the LibraryChanged event refreshes the grid.
        var error = await System.Threading.Tasks.Task.Run(() => NativeBae.DeleteRelease(_handle, releaseId));
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

        var json = NativeBae.ReleaseEditSeedJson(_handle, releaseId);
        var seeded = json is null ? null : JsonSerializer.Deserialize<ReleaseEdit>(json, JsonOptions);
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
            var payload = JsonSerializer.Serialize(form.ReadBack(), JsonOptions);
            var error = NativeBae.ApplyReleaseEdit(_handle, releaseId, payload);
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
                var resetJson = await System.Threading.Tasks.Task.Run(
                    () => NativeBae.ResetMetadataToSourceJson(_handle, releaseId));
                var fresh = resetJson is null
                    ? null
                    : JsonSerializer.Deserialize<ReleaseEdit>(resetJson, JsonOptions);
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

        // The FFI search and commit both block on network/DB work; run them off
        // the UI thread so the dialog stays responsive.
        searchButton.Click += async (_, _) =>
        {
            var source = (string)sourceBox.SelectedItem;
            var artist = artistBox.Text;
            var album = albumBox.Text;
            searchButton.IsEnabled = false;
            var json = await System.Threading.Tasks.Task.Run(
                () => NativeBae.SearchReleasesJson(_handle, source, artist, album));
            searchButton.IsEnabled = true;

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
            var error = await System.Threading.Tasks.Task.Run(
                () => NativeBae.ReidentifyRelease(_handle, releaseId, chosen.ReleaseId, chosen.Source));
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
        var json = NativeBae.GalleryJson(_handle, releaseId);
        if (json is null)
        {
            StatusText.Text = Loc.Chrome("gallery.load_failed");
            return;
        }

        var images = JsonSerializer.Deserialize<List<GalleryImage>>(json, JsonOptions);
        if (images is null)
        {
            StatusText.Text = Loc.Chrome("gallery.read_failed");
            return;
        }

        if (images.Count == 0)
        {
            return;
        }

        var index = 0;
        var image = new Image { Stretch = Stretch.Uniform, MinHeight = 360, MinWidth = 360 };
        var label = new TextBlock { HorizontalAlignment = HorizontalAlignment.Center };
        void Show()
        {
            image.Source = CoverImage.Load(images[index].Path);
            label.Text = $"{images[index].Label} ({index + 1}/{images.Count})";
        }
        Show();

        var prev = new Button { Content = "‹" };
        var next = new Button { Content = "›" };
        prev.Click += (_, _) => { index = (index - 1 + images.Count) % images.Count; Show(); };
        next.Click += (_, _) => { index = (index + 1) % images.Count; Show(); };

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
    /// LibraryChanged event the change emits. Errors surface inside this dialog,
    /// since the window banner is occluded by the modal.
    /// </summary>
    private async System.Threading.Tasks.Task ShowChangeCover(string albumId, string releaseId)
    {
        if (string.IsNullOrEmpty(albumId) || string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var imagesJson = NativeBae.GetReleaseImagesJson(_handle, releaseId);
        if (imagesJson is null)
        {
            StatusText.Text = Loc.Chrome("cover.images_load_failed");
            return;
        }

        var releaseImages = JsonSerializer.Deserialize<List<ReleaseImage>>(imagesJson, JsonOptions)
            ?? new List<ReleaseImage>();

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
            var error = await System.Threading.Tasks.Task.Run(
                () => NativeBae.ChangeCover(_handle, albumId, releaseId, selectionJson));
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

        if (releaseImages.Count > 0)
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
                var source = LoadCover(file.Id);
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
            var coversJson = await System.Threading.Tasks.Task.Run(
                () => NativeBae.FetchRemoteCoversJson(_handle, releaseId));
            loading.Visibility = Visibility.Collapsed;
            if (coversJson is null)
            {
                statusText.Text = Loc.Chrome("cover.fetch_failed");
                statusText.Visibility = Visibility.Visible;
                return;
            }

            try
            {
                var covers = JsonSerializer.Deserialize<List<RemoteCover>>(coversJson, JsonOptions)
                    ?? new List<RemoteCover>();
                if (covers.Count == 0)
                {
                    remoteGrid.Children.Add(new TextBlock { Text = Loc.Chrome("cover.none_remote") });
                    return;
                }

                foreach (var cover in covers)
                {
                    var source = new BitmapImage(new Uri(cover.ThumbnailUrl));
                    var selection = JsonSerializer.Serialize(
                        new { type = "remote_cover", url = cover.Url, source = cover.Source });
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
        if (_handle == IntPtr.Zero)
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
        // unmanaged release moved out of the library) drop out below.
        var selected = new HashSet<string>();
        // The current rows, kept so the right-tap menu can resolve a release's
        // allowed actions (for the multi-select intersection) by id.
        var rowsById = new Dictionary<string, StorageRow>();

        // Each row shows its summary; a left-click toggles its selection and a
        // right-click opens a menu of the transitions the core says it allows
        // (carried on the row, gated on cloud-home + pending uploads), plus
        // cancel for any queued uploads. The same actions run on every selected
        // release.
        async System.Threading.Tasks.Task LoadStorageRows()
        {
            var json = NativeBae.StorageJson(_handle);
            if (json is null)
            {
                storageStatus.Text = Loc.Chrome("storage.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            var rows = JsonSerializer.Deserialize<List<StorageRow>>(json, JsonOptions);
            if (rows is null)
            {
                storageStatus.Text = Loc.Chrome("storage.read_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            storageStatus.Visibility = Visibility.Collapsed;
            rowsById.Clear();
            foreach (var row in rows)
            {
                rowsById[row.ReleaseId] = row;
            }
            // Drop selections for releases no longer present after a transition.
            selected.IntersectWith(rowsById.Keys);

            listPanel.Children.Clear();
            foreach (var row in rows)
            {
                var text = new TextBlock
                {
                    Text = row.Summary,
                    VerticalAlignment = VerticalAlignment.Center,
                    TextWrapping = TextWrapping.Wrap,
                };
                var releaseId = row.ReleaseId;
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
        var outboxPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadOutbox()
        {
            outboxPanel.Children.Clear();
            var json = await System.Threading.Tasks.Task.Run(() => NativeBae.OutboxSnapshotJson(_handle));
            if (json is null)
            {
                storageStatus.Text = Loc.Chrome("outbox.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            var snapshot = JsonSerializer.Deserialize<OutboxSnapshot>(json, JsonOptions);
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("outbox.read_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            if (snapshot.Uploads.Count == 0 && snapshot.Deletes.Count == 0)
            {
                return;
            }

            // With work queued at least one count is non-zero, so the core's
            // summary is non-empty — render it directly.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = snapshot.Summary,
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button { Content = Loc.Chrome("outbox.retry_now") };
            retry.Click += async (_, _) =>
            {
                storageStatus.Visibility = Visibility.Collapsed;
                retry.IsEnabled = false;
                var error = await System.Threading.Tasks.Task.Run(() => NativeBae.RetryOutbox(_handle));
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
                await System.Threading.Tasks.Task.Run(() => NativeBae.SetSyncPaused(_handle, !paused));
                await LoadOutbox();
            };
            band.Children.Add(pause);
            outboxPanel.Children.Add(band);

            // Master progress strip: a byte-progress bar (dimmed while paused) and
            // the bytes / throughput / ETA labels the core pre-formats.
            if (snapshot.Total.BytesTotal > 0)
            {
                outboxPanel.Children.Add(new ProgressBar
                {
                    Minimum = 0,
                    Maximum = snapshot.Total.BytesTotal,
                    Value = snapshot.Total.BytesDone,
                    Opacity = paused ? 0.4 : 1.0,
                });
                var detail = new List<string>();
                if (!string.IsNullOrEmpty(snapshot.BytesLabel))
                {
                    detail.Add(snapshot.BytesLabel);
                }
                if (!string.IsNullOrEmpty(snapshot.ThroughputLabel))
                {
                    detail.Add(snapshot.ThroughputLabel);
                }
                if (!string.IsNullOrEmpty(snapshot.EtaLabel))
                {
                    detail.Add(snapshot.EtaLabel);
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

            // One row per queued operation: its label, an optional per-file
            // mini progress bar for an in-flight upload, and a Cancel that
            // dequeues it. `activeUpload` is set only for an upload that's
            // transferring right now, so its bar moves with the live byte count.
            void AddRow(long id, string label, UploadOp? activeUpload)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock
                {
                    Text = label,
                    TextWrapping = TextWrapping.Wrap,
                });
                if (activeUpload is { IsActive: true, BytesTotal: > 0 })
                {
                    labelColumn.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = activeUpload.BytesTotal,
                        Value = activeUpload.BytesDone,
                    });
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                var cancel = new Button { Content = Loc.Chrome("outbox.cancel_item") };
                cancel.Click += async (_, _) =>
                {
                    storageStatus.Visibility = Visibility.Collapsed;
                    cancel.IsEnabled = false;
                    var error = await System.Threading.Tasks.Task.Run(() => NativeBae.CancelOutboxItem(_handle, id));
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
                        cancel.IsEnabled = true;
                        return;
                    }

                    await LoadOutbox();
                };
                Grid.SetColumn(cancel, 1);
                itemGrid.Children.Add(cancel);
                outboxPanel.Children.Add(itemGrid);
            }

            foreach (var upload in snapshot.Uploads)
            {
                AddRow(upload.Id, upload.Label, upload);
            }
            foreach (var delete in snapshot.Deletes)
            {
                AddRow(delete.Id, delete.Label, null);
            }
        }

        await LoadOutbox();

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(storageStatus);
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
        // is open as uploads/deletes progress — the rows show each release's pending
        // upload count, which would otherwise go stale. Stops once the dialog closes.
        _refreshOutbox = () =>
        {
            _ = LoadOutbox();
            _ = LoadStorageRows();
        };
        // A library change (ReleaseUpdated → LibraryChanged) that isn't an outbox
        // change still alters a release's storage state — refresh the rows for it too.
        _refreshStorageRows = () => _ = LoadStorageRows();
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            _refreshOutbox = null;
            _refreshStorageRows = null;
        }
    }

    // Selected-row highlight: a faint accent tint, or transparent when not
    // selected. Static so LoadStorageRows and RefreshRowHighlights agree.
    private static Brush RowBackground(bool isSelected) =>
        isSelected
            ? new SolidColorBrush(Microsoft.UI.Colors.SteelBlue) { Opacity = 0.25 }
            : new SolidColorBrush(Microsoft.UI.Colors.Transparent);

    // User-facing label for a storage transition wire name, matching the macOS
    // "Storage…" sheet / context menu wording.
    private static string StorageActionLabel(string action) => action switch
    {
        "manage" => Loc.Chrome("storage.action.manage"),
        "unmanage" => Loc.Chrome("storage.action.unmanage"),
        "pin" => Loc.Chrome("storage.action.pin"),
        "unpin" => Loc.Chrome("storage.action.unpin"),
        _ => action,
    };

    // The transitions every release in the selection allows, intersected so the
    // menu only offers actions applicable to all. Order follows the first
    // release's action list (the core's order). Suppressed entirely when any
    // targeted release has uploads in flight: acting mid-upload races the
    // observer that completes the unmanaged → cloud step (the gate the core
    // leaves to the UI, mirroring the macOS "Storage…" sheet).
    private static List<string> IntersectedStorageActions(
        List<string> releaseIds, Dictionary<string, StorageRow> rowsById)
    {
        var anyUploading = releaseIds.Any(
            id => rowsById.TryGetValue(id, out var r) && r.PendingUploads > 0);
        if (anyUploading)
        {
            return new List<string>();
        }

        var perRelease = releaseIds
            .Select(id => rowsById.TryGetValue(id, out var row)
                ? new HashSet<string>(row.Actions)
                : new HashSet<string>())
            .ToList();
        if (perRelease.Count == 0)
        {
            return new List<string>();
        }

        var common = perRelease[0];
        foreach (var set in perRelease.Skip(1))
        {
            common.IntersectWith(set);
        }
        // Preserve the core's action order from the first release's row.
        var order = rowsById.TryGetValue(releaseIds[0], out var firstRow)
            ? firstRow.Actions
            : new List<string>();
        return order.Where(common.Contains).ToList();
    }

    // Build the right-tap menu for the targeted releases: the intersected
    // storage transitions plus a cancel for any of their queued uploads. Each
    // item runs the action on every targeted release, then reloads the rows.
    private async System.Threading.Tasks.Task<MenuFlyout> BuildStorageRowMenu(
        List<string> releaseIds,
        Dictionary<string, StorageRow> rowsById,
        TextBlock storageStatus,
        Func<System.Threading.Tasks.Task> reload)
    {
        var menu = new MenuFlyout();

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

        // Cancel the queued uploads of any targeted release (the Uploading-state
        // counterpart to macOS's Uploading tab). Resolved from the live outbox
        // snapshot since the storage row carries only a pending count.
        var cancelIds = await UploadOpIdsForReleases(releaseIds);
        if (cancelIds.Count > 0)
        {
            var cancel = new MenuFlyoutItem { Text = Loc.Chrome("storage.cancel_upload") };
            cancel.Click += async (_, _) =>
            {
                foreach (var id in cancelIds)
                {
                    var error = await System.Threading.Tasks.Task.Run(
                        () => NativeBae.CancelOutboxItem(_handle, id));
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
        }

        return menu;
    }

    // The queued upload op ids belonging to any of the given releases, read from
    // the current outbox snapshot. Empty when none are queued or the snapshot
    // can't be read.
    private async System.Threading.Tasks.Task<List<long>> UploadOpIdsForReleases(
        List<string> releaseIds)
    {
        var json = await System.Threading.Tasks.Task.Run(
            () => NativeBae.OutboxSnapshotJson(_handle));
        if (json is null)
        {
            return new List<long>();
        }

        var snapshot = JsonSerializer.Deserialize<OutboxSnapshot>(json, JsonOptions);
        if (snapshot is null)
        {
            return new List<long>();
        }

        var targets = new HashSet<string>(releaseIds);
        return snapshot.Uploads
            .Where(op => op.ReleaseId is not null && targets.Contains(op.ReleaseId))
            .Select(op => op.Id)
            .ToList();
    }

    // Run a storage transition on every release in the selection off the UI
    // thread. "unmanage" asks once for a destination folder, then moves each
    // release into it. Returns null on success (or a cancelled picker), else the
    // first error message.
    private async System.Threading.Tasks.Task<string?> RunStorageActionForReleases(
        string action, List<string> releaseIds)
    {
        if (action == "unmanage")
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
            return await System.Threading.Tasks.Task.Run(() =>
            {
                foreach (var releaseId in releaseIds)
                {
                    var error = NativeBae.UnmanageRelease(_handle, releaseId, path);
                    if (error is not null)
                    {
                        return error;
                    }
                }

                return (string?)null;
            });
        }

        return await System.Threading.Tasks.Task.Run(() =>
        {
            foreach (var releaseId in releaseIds)
            {
                var error = action switch
                {
                    "pin" => NativeBae.PinRelease(_handle, releaseId),
                    "unpin" => NativeBae.UnpinRelease(_handle, releaseId),
                    "manage" => NativeBae.ManageRelease(_handle, releaseId, false, false),
                    // A null return reads as success; an unknown action is a
                    // UI/core contract mismatch, so surface it rather than
                    // reload as if it worked.
                    _ => $"unknown storage action: {action}",
                };
                if (error is not null)
                {
                    return error;
                }
            }

            return (string?)null;
        });
    }

    private async void OnSettingsClick(object sender, RoutedEventArgs e)
    {
        if (_handle == IntPtr.Zero)
        {
            return;
        }

        var json = NativeBae.SettingsJson(_handle);
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
        // the configured/valid state comes from bae_settings, re-read on
        // ConfigChanged. not_configured/rejected → editable input + Save; valid →
        // "connected" + Remove; unvalidated → that label + Re-check + Remove. Save
        // and Re-check validate over the network, so they run off the UI thread and
        // show "Validating…" while in flight.
        //
        // Two text lines: `status` is the persisted state (driven only by
        // RenderDiscogs from the settings re-read, plus the in-flight "Validating…");
        // `errorText` is local feedback for an action — a rejected key, a re-check /
        // remove failure — cleared when the next action starts. Keeping them apart
        // means an unrelated ConfigChanged re-render can't wipe the rejection note.
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray) };
        var errorText = new TextBlock
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

        void ShowDiscogsError(string message)
        {
            errorText.Text = message;
            errorText.Visibility = Visibility.Visible;
        }

        void ClearDiscogsError()
        {
            errorText.Text = string.Empty;
            errorText.Visibility = Visibility.Collapsed;
        }

        // Drive the controls from the persisted status: which buttons show, whether
        // the input is editable, and the status line. Called on open and on every
        // ConfigChanged re-read. The draft text and the local error line are left
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
            ClearDiscogsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var outcome = await System.Threading.Tasks.Task.Run(
                () => NativeBae.SaveDiscogsToken(_handle, token));
            discogsBusy = false;
            switch (outcome)
            {
                case "valid":
                case "unvalidated":
                    // Stored: a ConfigChanged re-read settles the controls and label.
                    status.Text = string.Empty;
                    break;
                case "rejected":
                    // Nothing stored, so no ConfigChanged fires — keep the draft and
                    // surface the rejection.
                    status.Text = string.Empty;
                    ShowDiscogsError(Loc.Chrome("settings.discogs.rejected"));
                    break;
                default:
                    status.Text = string.Empty;
                    ShowDiscogsError(Loc.Chrome("settings.discogs.save_failed"));
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
            ClearDiscogsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var error = await System.Threading.Tasks.Task.Run(
                () => NativeBae.RevalidateDiscogsToken(_handle));
            discogsBusy = false;
            if (error is not null)
            {
                ShowDiscogsError(error);
            }
            // On success a ConfigChanged re-read settles the controls and label.
        };
        remove.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            ClearDiscogsError();
            // Removing clears the config flag, firing ConfigChanged — the re-read
            // restores the editable input. Nothing is patched inline here.
            var error = await System.Threading.Tasks.Task.Run(
                () => NativeBae.DeleteDiscogsToken(_handle));
            if (error is not null)
            {
                ShowDiscogsError(error);
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
                var warning = await System.Threading.Tasks.Task.Run(() => NativeBae.DisconnectWarning(_handle));
                if (warning is not null)
                {
                    syncStatus.Text = Loc.Chrome("settings.sync.disconnect_confirm", "warning", warning);
                    disconnectArmed = true;
                    return;
                }
            }

            disconnectArmed = false;
            var error = NativeBae.DisconnectCloud(_handle);
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
        syncNow.Click += (_, _) => NativeBae.TriggerSync(_handle);
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
                var error = await System.Threading.Tasks.Task.Run(() => NativeBae.SignInCloud(_handle, provider, storage));
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
        // The baeium (S3-only) DLL exports no OAuth entry points, so its available
        // set is just S3 and no sign-in button renders — there's no path to call a
        // missing symbol, independent of whether oauth-creds.json is present.
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
        var discogsLabel = new TextBlock { Text = Loc.Chrome("settings.discogs.label") };
        content.Children.Add(libraryLabel);
        content.Children.Add(discogsLabel);
        content.Children.Add(tokenBox);
        content.Children.Add(buttons);
        content.Children.Add(status);
        content.Children.Add(errorText);
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
            var error = await System.Threading.Tasks.Task.Run(() => NativeBae.SaveSyncConfig(
                _handle,
                s3Bucket.Text ?? string.Empty,
                s3Region.Text ?? string.Empty,
                s3Endpoint.Text ?? string.Empty,
                s3KeyPrefix.Text ?? string.Empty,
                s3AccessKey.Text ?? string.Empty,
                s3SecretKey.Password ?? string.Empty,
                storage));
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

        // Restore code: enter it on another device to restore this library.
        var restoreCode = new TextBox
        {
            Header = Loc.Chrome("settings.restore_code.label"),
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var showRestoreCode = new Button { Content = Loc.Chrome("settings.restore_code.show") };
        showRestoreCode.Click += (_, _) =>
        {
            restoreCode.Text = NativeBae.GenerateRestoreCode(_handle) ?? Loc.Chrome("settings.restore_code.unavailable");
            restoreCode.Visibility = Visibility.Visible;
        };
        content.Children.Add(showRestoreCode);
        content.Children.Add(restoreCode);

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

        // Re-read the (FFI-pre-computed) settings into the live labels so a
        // ConfigChanged event — or a connect/disconnect in this dialog — updates
        // them in place instead of requiring a reopen.
        _refreshSettings = () =>
        {
            var freshJson = NativeBae.SettingsJson(_handle);
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
            RenderDiscogs(fresh);
        };

        // A key saved while offline lands "unvalidated"; opening settings is a
        // chance to settle it now that there may be connectivity. The core no-ops
        // unless the stored key is actually unvalidated, so call unconditionally;
        // on a result it changes the status, firing ConfigChanged → RenderDiscogs.
        _ = System.Threading.Tasks.Task.Run(() => NativeBae.RevalidateDiscogsToken(_handle));

        await dialog.ShowAsync();
        _refreshSettings = null;
        if (lockRequested)
        {
            var error = await System.Threading.Tasks.Task.Run(() => NativeBae.LockActiveLibrary(_handle));
            if (error is not null)
            {
                StatusText.Text = error;
                return;
            }

            // The key is forgotten now, so re-opening lands on the unlock prompt.
            if (_handle != IntPtr.Zero)
            {
                NativeBae.HandleFree(_handle);
                _handle = IntPtr.Zero;
            }

            OpenLibrary(s.LibraryId);
        }
    }

    private void OnClosed(object sender, WindowEventArgs args)
    {
        if (_handle != IntPtr.Zero)
        {
            // Persist the queue / current track / position before freeing the
            // handle, so the next launch can restore where playback left off.
            NativeBae.Shutdown(_handle);
            NativeBae.HandleFree(_handle);
            _handle = IntPtr.Zero;
        }
    }
}
