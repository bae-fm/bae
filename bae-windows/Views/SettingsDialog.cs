using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.System;
// `Windows.System` (needed for VirtualKey) also declares a DispatcherQueue;
// alias the WinUI one so the unqualified name stays unambiguous (CS0104).
using DispatcherQueue = Microsoft.UI.Dispatching.DispatcherQueue;

namespace Bae.Windows;

// The settings dialog: Discogs key, cloud sync (disconnect / S3 / OAuth), export
// template and presets, MCP automation, devices, recovery code, updates, lock,
// and remove. Reads the current settings through the settings store and
// re-renders when a config invalidation (or an in-dialog connect/disconnect)
// reloads them; those registrations live only while the dialog is open. The
// lock, remove, add-device, and apply-update flows close the dialog and run
// after it returns (a nested ContentDialog can't open over it).
internal sealed class SettingsDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly DispatcherQueue _dispatcher;
    private readonly SettingsStore _settings;
    private readonly MembersPane _membersPane;
    private readonly ApproveDeviceDialog _approveDialog;
    private readonly UpdateService _updateService;
    private readonly ProjectionRegistry _projections;
    private readonly Action<string> _setStatus;
    private readonly Action<string> _openLibrary;
    private readonly Func<System.Threading.Tasks.Task> _closeToWelcome;

    public SettingsDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        DispatcherQueue dispatcher,
        SettingsStore settings,
        MembersPane membersPane,
        ApproveDeviceDialog approveDialog,
        UpdateService updateService,
        ProjectionRegistry projections,
        Action<string> setStatus,
        Action<string> openLibrary,
        Func<System.Threading.Tasks.Task> closeToWelcome)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _dispatcher = dispatcher;
        _settings = settings;
        _membersPane = membersPane;
        _approveDialog = approveDialog;
        _updateService = updateService;
        _projections = projections;
        _setStatus = setStatus;
        _openLibrary = openLibrary;
        _closeToWelcome = closeToWelcome;
    }

    public async System.Threading.Tasks.Task Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        var (current, s) = _session.WithCurrentHandle(NativeBae.GetSettings);
        if (!current)
        {
            return;
        }

        // Host-originated telemetry: the settings screen opened. Infallible.
        _session.WithCurrentHandle(handle => NativeBae.ReportScreen(handle, BridgeScreen.Settings));

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
            var (current, outcome) = await _session.RunForCurrentHandle(
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
            var (current, error) = await _session.RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);
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
            var (current, error) = await _session.RunForCurrentHandle(NativeBae.DeleteDiscogsToken);
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
                var (warningCurrent, warning) = await _session.RunForCurrentHandle(NativeBae.DisconnectWarning);
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
            var (disconnectCurrent, error) = _session.WithCurrentHandle(NativeBae.DisconnectCloud);
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
                _settings.Reload();
            }
        };
        var syncNow = new Button { Content = Loc.Chrome("settings.sync.now") };
        syncNow.Click += (_, _) => _session.WithCurrentHandle(NativeBae.TriggerSync);
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
                var (current, error) = await _session.RunForCurrentHandle(
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
                    _settings.Reload();
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
                oauthButtons.Children.Add(CloudButton(BridgeDisplay.ProviderDisplayName(wire), wire));
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
            var (current, error) = await _session.RunForCurrentHandle(
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

        // Whether quitting performs the graceful shutdown that saves the
        // current track, position, queue, and volume for the next launch to
        // restore. Device-local (no bridge round-trip), so no error snap-back
        // and no Refresh() involvement — a config invalidation doesn't carry it.
        var restoreOnLaunch = new CheckBox
        {
            Content = Loc.Chrome("settings.playback.restore_on_launch"),
            IsChecked = PersistPlaybackStore.Load(),
        };
        restoreOnLaunch.Checked += (_, _) => PersistPlaybackStore.Save(true);
        restoreOnLaunch.Unchecked += (_, _) => PersistPlaybackStore.Save(false);
        var restoreOnLaunchHelp = new TextBlock
        {
            Text = Loc.Chrome("settings.playback.restore_on_launch_help"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };

        // Export: the release-export destination policy (a fixed folder or a
        // prompt each time), then the single-track "Save As…" suggested-filename
        // template and the export presets. Writes round-trip through config
        // invalidation into the settings re-read (RenderExport) with no optimistic
        // mutation; the checkboxes send the whole set (set-state), never one
        // mutated field.
        var exportLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.export.label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        // Where release exports write. The authoritative destination is core's
        // export-location config, written through set_export_location and settled
        // by the config re-read; the remembered folder is a local convenience
        // memory (the greyed path under "ask each time", the folder restored when
        // re-selecting "save to a folder").
        var saveToFolder = new RadioButton
        {
            Content = Loc.Chrome("settings.export.save_to_folder"),
            GroupName = "exportLocation",
        };
        var askEachTime = new RadioButton
        {
            Content = Loc.Chrome("settings.export.ask_each_time"),
            GroupName = "exportLocation",
        };
        var locationPath = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            VerticalAlignment = VerticalAlignment.Center,
        };
        var changeFolder = new Button { Content = Loc.Chrome("settings.export.change_folder") };
        var locationRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        locationRow.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.export.location"),
            VerticalAlignment = VerticalAlignment.Center,
        });
        locationRow.Children.Add(locationPath);
        locationRow.Children.Add(changeFolder);
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
            RenderExportLocation(settings);
            exportTemplate.Text = settings.ExportFilenameTemplate;
            PopulateExportSelection(defaultTrackExport, settings, release: false);
            PopulateExportSelection(defaultReleaseExport, settings, release: true);
            RenderExportPresets(settings);
            refreshingSettings = false;
        }

        // Drive the export-location controls from the config: which radio is
        // checked, the Location-row path (the fixed folder, or the remembered
        // folder greyed under "ask each time"), and whether Change is available.
        void RenderExportLocation(Settings settings)
        {
            var fixedLocation = settings.ExportLocation as BridgeExportLocation.Fixed;
            var isFixed = fixedLocation is not null;
            saveToFolder.IsChecked = isFixed;
            askEachTime.IsChecked = !isFixed;
            locationPath.Text = ExportQueueModel.LocationRowPath(isFixed, fixedLocation?.Dir, ExportFolderStore.Load())
                ?? Loc.Chrome("settings.export.no_folder");
            changeFolder.IsEnabled = isFixed;
            locationPath.Opacity = isFixed ? 1.0 : 0.5;
        }

        // The folder picker for a fixed export location, run in the app window.
        // Returns the chosen path, or null when the user cancelled.
        async System.Threading.Tasks.Task<string?> PickExportFolder()
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
            var folder = await picker.PickSingleFolderAsync();
            return folder?.Path;
        }

        // Write the export location through the bridge and let the config
        // invalidation re-read settle the controls. A fixed folder is remembered
        // only after the write lands; an error snaps the controls back.
        async System.Threading.Tasks.Task SaveExportLocation(BridgeExportLocation location)
        {
            ClearSettingsError();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetExportLocation(handle, location));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _settings.Reload();
                return;
            }
            if (location is BridgeExportLocation.Fixed fixedLocation)
            {
                ExportFolderStore.Save(fixedLocation.Dir);
            }
        }

        askEachTime.Checked += async (_, _) =>
        {
            if (refreshingSettings)
            {
                return;
            }
            await SaveExportLocation(new BridgeExportLocation.AskEachTime());
        };
        saveToFolder.Checked += async (_, _) =>
        {
            if (refreshingSettings)
            {
                return;
            }
            var remembered = ExportFolderStore.Load();
            if (ExportQueueModel.FixedSelectionNeedsPrompt(remembered))
            {
                var dir = await PickExportFolder();
                if (dir is null)
                {
                    // Cancelled with no remembered folder: snap the radio back to
                    // the stored location.
                    _settings.Reload();
                    return;
                }
                await SaveExportLocation(new BridgeExportLocation.Fixed(dir));
            }
            else
            {
                await SaveExportLocation(new BridgeExportLocation.Fixed(remembered!));
            }
        };
        changeFolder.Click += async (_, _) =>
        {
            var dir = await PickExportFolder();
            if (dir is null)
            {
                return;
            }
            await SaveExportLocation(new BridgeExportLocation.Fixed(dir));
        };

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

        bool SameExportSelection(BridgeExportSelection a, BridgeExportSelection b) =>
            ExportSelection.Equal(a, b);

        string CodecLabel(BridgeExportPresetCodec codec) => codec switch
        {
            BridgeExportPresetCodec.Flac => "FLAC",
            BridgeExportPresetCodec.Mp3 mp3 => $"MP3 {mp3.BitrateKbps} kbps",
            BridgeExportPresetCodec.OpusOgg opus => $"Opus {opus.BitrateKbps} kbps",
            BridgeExportPresetCodec.Wav => "WAV",
            BridgeExportPresetCodec.Aiff => "AIFF",
            _ => string.Empty,
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
                        && selected.Tag is BridgeExportPregapPlacement placement
                        && placement == BridgeExportPregapPlacement.SingleFileWithCue;
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
                    if (pregap.SelectedItem is ComboBoxItem selected && selected.Tag is BridgeExportPregapPlacement placement)
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
            switch (preset.Codec)
            {
                case BridgeExportPresetCodec.Flac:
                case BridgeExportPresetCodec.Wav:
                case BridgeExportPresetCodec.Aiff:
                    var currentBitDepth = preset.Codec switch
                    {
                        BridgeExportPresetCodec.Flac current => current.BitDepth,
                        BridgeExportPresetCodec.Wav current => current.BitDepth,
                        BridgeExportPresetCodec.Aiff current => current.BitDepth,
                        _ => BridgeExportBitDepth.Source,
                    };
                    var bitDepth = new ComboBox { Header = Loc.Chrome("settings.export.bit_depth_label") };
                    foreach (var item in ExportBitDepthChoices())
                    {
                        bitDepth.Items.Add(new ComboBoxItem
                        {
                            Content = item.Label,
                            Tag = item.Value,
                            IsSelected = currentBitDepth == item.Value,
                        });
                    }
                    panel.Children.Add(bitDepth);
                    return (
                        panel,
                        () =>
                        {
                            if (bitDepth.SelectedItem is ComboBoxItem selected && selected.Tag is BridgeExportBitDepth selectedBitDepth)
                            {
                                preset.Codec = preset.Codec switch
                                {
                                    BridgeExportPresetCodec.Flac => new BridgeExportPresetCodec.Flac(selectedBitDepth),
                                    BridgeExportPresetCodec.Wav => new BridgeExportPresetCodec.Wav(selectedBitDepth),
                                    BridgeExportPresetCodec.Aiff => new BridgeExportPresetCodec.Aiff(selectedBitDepth),
                                    _ => preset.Codec,
                                };
                            }
                        }
                    );
                case BridgeExportPresetCodec.Mp3:
                case BridgeExportPresetCodec.OpusOgg:
                    var currentBitrate = preset.Codec switch
                    {
                        BridgeExportPresetCodec.Mp3 current => current.BitrateKbps,
                        BridgeExportPresetCodec.OpusOgg current => current.BitrateKbps,
                        _ => 0u,
                    };
                    var bitrate = new TextBox
                    {
                        Header = Loc.Chrome("settings.export.bitrate"),
                        Text = currentBitrate.ToString(CultureInfo.InvariantCulture),
                    };
                    panel.Children.Add(bitrate);
                    return (
                        panel,
                        () =>
                        {
                            if (uint.TryParse(bitrate.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var bitrateKbps))
                            {
                                preset.Codec = preset.Codec switch
                                {
                                    BridgeExportPresetCodec.Mp3 => new BridgeExportPresetCodec.Mp3(bitrateKbps),
                                    BridgeExportPresetCodec.OpusOgg => new BridgeExportPresetCodec.OpusOgg(bitrateKbps),
                                    _ => preset.Codec,
                                };
                                return;
                            }
                            preset.Codec = preset.Codec switch
                            {
                                BridgeExportPresetCodec.Mp3 => new BridgeExportPresetCodec.Mp3(0),
                                BridgeExportPresetCodec.OpusOgg => new BridgeExportPresetCodec.OpusOgg(0),
                                _ => preset.Codec,
                            };
                        }
                    );
                default:
                    return (panel, () => { });
            }
        }

        List<(string Label, BridgeExportPregapPlacement Value)> ExportPregapChoices(BridgeExportPresetCodec codec)
        {
            var choices = new List<(string Label, BridgeExportPregapPlacement Value)>
            {
                (
                    Loc.Chrome("settings.export.pregap.append_except_htoa"),
                    BridgeExportPregapPlacement.AppendToPreviousExceptHtoa
                ),
                (
                    Loc.Chrome("settings.export.pregap.append_including_htoa"),
                    BridgeExportPregapPlacement.AppendToPreviousIncludingHtoa
                ),
                (Loc.Chrome("settings.export.pregap.exclude"), BridgeExportPregapPlacement.Exclude),
            };

            if (ExportCodecSupportsSingleFileCue(codec))
            {
                choices.Add((
                    Loc.Chrome("settings.export.pregap.single_file_with_cue"),
                    BridgeExportPregapPlacement.SingleFileWithCue
                ));
            }

            return choices;
        }

        static bool ExportCodecSupportsSingleFileCue(BridgeExportPresetCodec codec) =>
            codec is not BridgeExportPresetCodec.OpusOgg;

        List<(string Label, BridgeExportBitDepth Value)> ExportBitDepthChoices() => new()
        {
            (Loc.Chrome("settings.export.bit_depth.source"), BridgeExportBitDepth.Source),
            (Loc.Chrome("settings.export.bit_depth.bits16"), BridgeExportBitDepth.Bits16),
            (Loc.Chrome("settings.export.bit_depth.bits24"), BridgeExportBitDepth.Bits24),
            (Loc.Chrome("settings.export.bit_depth.bits32"), BridgeExportBitDepth.Bits32),
        };

        async System.Threading.Tasks.Task SaveExportTemplate()
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var template = exportTemplate.Text ?? string.Empty;
            var (current, error) = await _session.RunForCurrentHandle(
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
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetExportPresets(handle, presets));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _settings.Reload();
            }
        }

        async System.Threading.Tasks.Task SaveDefaultExportSelection(ComboBox combo, bool release)
        {
            if (refreshingSettings || combo.SelectedItem is not ComboBoxItem item || item.Tag is not BridgeExportSelection selection)
            {
                return;
            }

            ClearSettingsError();
            var (current, error) = await _session.RunForCurrentHandle(handle => release
                ? NativeBae.SetDefaultReleaseExportSelection(handle, selection)
                : NativeBae.SetDefaultTrackExportSelection(handle, selection));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _settings.Reload();
            }
        }

        ExportPreset MakeExportPreset(string kind)
        {
            BridgeExportPresetCodec codec = kind switch
            {
                "mp3" => new BridgeExportPresetCodec.Mp3(320),
                "opus_ogg" => new BridgeExportPresetCodec.OpusOgg(192),
                "wav" => new BridgeExportPresetCodec.Wav(BridgeExportBitDepth.Source),
                "aiff" => new BridgeExportPresetCodec.Aiff(BridgeExportBitDepth.Source),
                _ => new BridgeExportPresetCodec.Flac(BridgeExportBitDepth.Source),
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
                PregapPlacement = BridgeExportPregapPlacement.AppendToPreviousExceptHtoa,
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
            var (current, error) = await _session.RunForCurrentHandle(
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
            _settings.Reload();
        }

        async System.Threading.Tasks.Task RefreshMcpStatus()
        {
            var (current, status) = await _session.RunForCurrentHandle(NativeBae.McpServerStatus);
            if (!current)
            {
                return;
            }
            mcpStatus.Text = Settings.McpStatusTextFor(status);
        }

        async System.Threading.Tasks.Task CopyMcpToken(Func<AppHandle, string?> readToken, string successKey)
        {
            ClearSettingsError();
            var (current, token) = await _session.RunForCurrentHandle(readToken);
            if (!current)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            ClipboardHelper.CopyToClipboard(token);
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
            var (tokenCurrent, token) = await _session.RunForCurrentHandle(NativeBae.GenerateMcpToken);
            if (!tokenCurrent)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            var (current, error) = await _session.RunForCurrentHandle(
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
            ClipboardHelper.CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome("settings.automation.token_rotated");
        };

        var discogsLabel = new TextBlock { Text = Loc.Chrome("settings.discogs.label") };
        content.Children.Add(libraryLabel);
        content.Children.Add(pauseBetweenSides);
        content.Children.Add(restoreOnLaunch);
        content.Children.Add(restoreOnLaunchHelp);
        content.Children.Add(exportLabel);
        content.Children.Add(saveToFolder);
        content.Children.Add(askEachTime);
        content.Children.Add(locationRow);
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
            var (current, error) = await _session.RunForCurrentHandle(
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
                _settings.Reload();
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
            var (current, code) = await _session.RunForCurrentHandle(NativeBae.GenerateRestoreCode);
            if (!current)
            {
                return;
            }
            recoveryCode.Text = code ?? Loc.Chrome("settings.recovery.unavailable");
            recoveryCode.Visibility = Visibility.Visible;
        };
        content.Children.Add(showRecoveryCode);
        content.Children.Add(recoveryCode);

        // Updates: the installed version, and — for a Velopack install — a manual
        // check that downloads in the background and applies on restart. A dev run
        // or a loose-zip copy is not an install, so only the version line shows.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.updates.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var installedVersion = _updateService.InstalledVersion is { } version
            ? UpdateFlowDisplay.VersionDisplay(version)
            : AppMetadata.ConfiguredString("BaeGitCommit");
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.updates.version", "version", installedVersion),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });

        var restartUpdateRequested = false;
        Button? updateRestartButton = null;
        Action? unsubscribeUpdates = null;
        if (_updateService.IsAvailable)
        {
            var updateStatus = new TextBlock
            {
                TextWrapping = TextWrapping.Wrap,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            };
            var checkUpdates = new Button { Content = Loc.Chrome("settings.updates.check") };
            var restartUpdate = new Button { Content = Loc.Chrome("settings.updates.restart") };
            updateRestartButton = restartUpdate;

            void RenderUpdates(UpdateFlowState state)
            {
                if (UpdateFlowDisplay.StatusFor(state) is { } mapped)
                {
                    updateStatus.Text = mapped.Args is { } args
                        ? Loc.Chrome(mapped.Key, args)
                        : Loc.Chrome(mapped.Key);
                    updateStatus.Visibility = Visibility.Visible;
                }
                else
                {
                    updateStatus.Text = string.Empty;
                    updateStatus.Visibility = Visibility.Collapsed;
                }
                checkUpdates.IsEnabled = UpdateFlowDisplay.CheckEnabled(state);
                restartUpdate.Visibility = UpdateFlowDisplay.RestartVisible(state)
                    ? Visibility.Visible
                    : Visibility.Collapsed;
            }

            checkUpdates.Click += async (_, _) => await _updateService.CheckAsync();

            // State transitions arrive on a worker thread; marshal to the UI
            // thread. Subscribed for the dialog's lifetime, so reopening settings
            // reflects a background download that finished while it was closed
            // (the phase lives on the service, not the dialog).
            void OnUpdateStateChanged(UpdateFlowState state) =>
                _dispatcher.TryEnqueue(() => RenderUpdates(state));
            _updateService.StateChanged += OnUpdateStateChanged;
            unsubscribeUpdates = () => _updateService.StateChanged -= OnUpdateStateChanged;

            RenderUpdates(_updateService.State);
            content.Children.Add(updateStatus);
            content.Children.Add(checkUpdates);
            content.Children.Add(restartUpdate);
        }

        // Lock this library: forget its encryption key on this device. Sync stops
        // and the library reopens to the unlock prompt; local files stay.
        var lockRequested = false;
        var lockButton = new Button { Content = Loc.Chrome("settings.lock_library") };
        content.Children.Add(lockButton);

        // Remove this library from this device: delete its local data directory,
        // clear the active-library pointer, and drop its encryption key, leaving
        // any cloud copy untouched. Two-step armed confirm, like disconnect: the
        // first click reads the outbox snapshot off the UI thread to decide
        // whether to call out unlanded cloud work, renders the confirmation body
        // inline, and arms; the second click requests the forget and closes the
        // dialog — the destructive work runs after ShowAsync returns (the
        // post-close dance, like lock), because a nested ContentDialog can't
        // open over this one.
        var forgetRequested = false;
        var removeArmed = false;
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.remove.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var removeFooter = new TextBlock
        {
            Text = Loc.Chrome(ForgetLibraryModel.FooterKey(s.HasCloudHome)),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        content.Children.Add(removeFooter);
        var removeButton = new Button { Content = Loc.Chrome("settings.remove.button") };
        content.Children.Add(removeButton);
        var removeConfirmText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(removeConfirmText);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.title"),
            Content = new ScrollViewer { Content = content },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };
        lockButton.Click += (_, _) =>
        {
            lockRequested = true;
            dialog.Hide();
        };
        removeButton.Click += async (_, _) =>
        {
            if (!removeArmed)
            {
                var (snapshotCurrent, snapshotResult) = await _session.RunForCurrentHandle(NativeBae.OutboxSnapshot);
                if (!snapshotCurrent)
                {
                    return;
                }
                if (snapshotResult.Error is not null)
                {
                    // The outbox read only informs the confirmation copy; a
                    // failure here doesn't block the confirm from arming, it
                    // just can't call out pending cloud work.
                    BaeDiagnostics.Logger.Warning(
                        $"Could not read the outbox snapshot for the remove confirmation: {snapshotResult.Error}");
                }
                var hasPendingCloudWork = snapshotResult.Snapshot is { } snapshot
                    && ForgetLibraryModel.HasPendingCloudWork(snapshot.UploadGroups.Length, snapshot.PendingDeletes);
                var hasCloudHome = _settings.Current?.HasCloudHome ?? s.HasCloudHome;
                removeConfirmText.Text = string.Join(
                    " ",
                    ForgetLibraryModel.ConfirmKeys(hasCloudHome, hasPendingCloudWork).Select(key => Loc.Chrome(key)));
                removeConfirmText.Visibility = Visibility.Visible;
                removeArmed = true;
                return;
            }

            forgetRequested = true;
            dialog.Hide();
        };

        // The restart button applies a staged update: hide the dialog and run the
        // apply after ShowAsync returns (a nested dialog can't open over this one,
        // same as the lock dance). Only wired when the updates section rendered it.
        if (updateRestartButton is not null)
        {
            updateRestartButton.Click += (_, _) =>
            {
                restartUpdateRequested = true;
                dialog.Hide();
            };
        }

        // Now that the dialog exists, load the device list into its placeholder.
        // The add-device button (owner-only) arms the approve flow and closes the
        // settings dialog — a nested ContentDialog can't open over it, so the
        // approve flow runs after this one returns (mirroring the lock dance).
        _ = _membersPane.LoadInto(membersHost, () =>
        {
            addDeviceRequested = true;
            dialog.Hide();
        });

        // Re-read the (generated bridge-pre-computed) settings into the live labels so a
        // config invalidation — or a connect/disconnect in this dialog — updates
        // them in place instead of requiring a reopen. The store's Reload raises
        // Changed; this renders from its fresh snapshot.
        void Refresh()
        {
            if (_settings.Current is not { } fresh)
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
            removeFooter.Text = Loc.Chrome(ForgetLibraryModel.FooterKey(fresh.HasCloudHome));
        }
        _settings.Changed += Refresh;
        // A config invalidation reloads the store while the dialog is open; the
        // registration is disposed on close.
        var configRegistration = _projections.Register(
            typeof(BridgeInvalidation.Config), () => _settings.Reload());

        // A key saved while offline lands "unvalidated"; opening settings is a
        // chance to settle it now that there may be connectivity. The core no-ops
        // unless the stored key is actually unvalidated, so call unconditionally;
        // on a result it changes the status, firing a config invalidation.
        _ = _session.RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);

        await dialog.ShowAsync();
        _settings.Changed -= Refresh;
        configRegistration.Dispose();
        unsubscribeUpdates?.Invoke();

        if (restartUpdateRequested)
        {
            // ApplyUpdatesAndRestart exits the process, so this is a second
            // app-exit path: flush telemetry through the live handle first, then
            // gate the state-saving shutdown on the same restore-on-launch
            // preference OnClosed does — the work OnClosed would otherwise do.
            _session.WithCurrentHandle(BaeDiagnostics.Flush);
            if (PersistPlaybackStore.Load())
            {
                await _session.ShutdownAndFreeCurrentHandle();
            }
            _updateService.ApplyAndRestart();
            return;
        }

        if (lockRequested)
        {
            var (lockCurrent, error) = await _session.RunForCurrentHandle(NativeBae.LockActiveLibrary);
            if (!lockCurrent)
            {
                return;
            }
            if (error is not null)
            {
                _setStatus(error);
                return;
            }

            // The key is forgotten now, so re-opening lands on the unlock prompt.
            await _session.ShutdownAndFreeCurrentHandle();

            _openLibrary(s.LibraryId);
            return;
        }

        if (forgetRequested)
        {
            var (forgetCurrent, error) = await _session.RunForCurrentHandle(NativeBae.ForgetLibrary);
            if (!forgetCurrent)
            {
                return;
            }
            if (error is not null)
            {
                _setStatus(Loc.Chrome("settings.remove.failed", "error", error));
                return;
            }

            // The local directory is gone; tear the handle down and return to
            // the welcome chooser, mirroring macOS's closeLibrary().
            await _closeToWelcome();
            return;
        }

        // Add-a-device closed settings to open the approve flow (no nested
        // dialogs). Run it, then reopen settings so the refreshed device list
        // shows the newly-approved device.
        if (addDeviceRequested)
        {
            await _approveDialog.Show();
            await Show();
        }
    }
}
