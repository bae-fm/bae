using System.Globalization;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.System;

namespace Bae.Windows;

// The settings dialog's Formats section: the format list (inline Save As…
// scope toggles, edit dialog, add/remove) and the default format per save
// target. Writes round-trip through config invalidation into the settings
// re-read (Render) with no optimistic mutation; preset edits send the whole
// set (set-state), never one mutated field. Mirrors macOS's
// FormatsSettingsTab.
internal sealed class FormatsSettingsSection
{
    public StackPanel View { get; } = new() { Spacing = 8 };

    private readonly SessionStore _session;
    private readonly SettingsStore _settings;
    private readonly Func<XamlRoot?> _dialogRoot;
    private readonly Action<string> _showError;
    private readonly Action _clearError;

    private readonly ComboBox _defaultTrack = new() { Header = Loc.Chrome("settings.formats.default_track_format") };
    private readonly ComboBox _defaultRelease = new() { Header = Loc.Chrome("settings.formats.default_release_format") };
    private readonly StackPanel _presetPanel = new() { Spacing = 8 };

    private bool _rendering;

    // The settings snapshot the section last rendered — the list the preset
    // controls mutate and save whole. Set before the section is interactive
    // (the dialog renders once at open).
    private Settings? _current;

    public FormatsSettingsSection(
        SessionStore session,
        SettingsStore settings,
        Func<XamlRoot?> dialogRoot,
        Action<string> showError,
        Action clearError)
    {
        _session = session;
        _settings = settings;
        _dialogRoot = dialogRoot;
        _showError = showError;
        _clearError = clearError;

        _defaultTrack.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultTrack, release: false);
        _defaultRelease.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultRelease, release: true);

        View.Children.Add(SectionLabel(Loc.Chrome("settings.formats.formats")));
        View.Children.Add(_presetPanel);
        View.Children.Add(AddPresetButton());
        View.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.formats.presets_footer"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        View.Children.Add(SectionLabel(Loc.Chrome("settings.formats.defaults")));
        View.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.formats.defaults_footer"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        View.Children.Add(_defaultRelease);
        View.Children.Add(_defaultTrack);
    }

    // Drive every control from the persisted settings. Called on open and on
    // every config-invalidation re-read; in-progress local drafts (a name being
    // typed) live in the rebuilt controls and re-seed from the fresh values.
    public void Render(Settings settings)
    {
        _current = settings;
        _rendering = true;
        PopulateSelection(_defaultTrack, settings, release: false);
        PopulateSelection(_defaultRelease, settings, release: true);
        RenderPresets(settings);
        _rendering = false;
    }

    private static TextBlock SectionLabel(string text) => new()
    {
        Text = text,
        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        Margin = new Thickness(0, 8, 0, 0),
    };

    private static TextBlock PreviewLine() => new()
    {
        TextWrapping = TextWrapping.Wrap,
        Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
    };

    // ── Default formats ─────────────────────────────────────────────────────

    private void PopulateSelection(ComboBox combo, Settings settings, bool release)
    {
        combo.Items.Clear();
        var selected = release
            ? settings.DefaultReleaseSavePreset
            : settings.DefaultTrackSavePreset;
        foreach (var preset in settings.SavePresets.Where(
            p => release ? p.AppliesToRelease : p.AppliesToTrack))
        {
            combo.Items.Add(new ComboBoxItem
            {
                Content = preset.Name,
                Tag = preset.Id,
                IsSelected = preset.Id == selected,
            });
        }
    }

    private async System.Threading.Tasks.Task SaveDefaultSelection(ComboBox combo, bool release)
    {
        if (_rendering
            || combo.SelectedItem is not ComboBoxItem item
            || item.Tag is not string presetId)
        {
            return;
        }
        _clearError();
        var (current, error) = await _session.RunForCurrentHandle(handle => release
            ? NativeBae.SetDefaultReleaseSavePreset(handle, presetId)
            : NativeBae.SetDefaultTrackSavePreset(handle, presetId));
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _showError(error);
            _settings.Reload();
        }
    }

    // ── Presets ──────────────────────────────────────────────────────────────

    private void RenderPresets(Settings settings)
    {
        _presetPanel.Children.Clear();
        foreach (var preset in settings.SavePresets)
        {
            _presetPanel.Children.Add(PresetRow(settings, preset));
        }
    }

    // One format in the list: the summary opens the edit dialog, the inline
    // Track/Release toggles set which Save As\u2026 scopes it appears under, and the
    // trailing minus removes it behind a confirmation. Mirrors macOS's
    // PresetRow.
    private Grid PresetRow(Settings settings, SavePreset preset)
    {
        var row = new Grid { ColumnSpacing = 12 };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        var open = new Button
        {
            Content = PresetHeader(preset),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4, 6, 4, 6),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        };
        open.Click += async (_, _) => await ShowPresetEditor(settings, preset);
        row.Children.Add(open);

        var scopes = PresetScopeToggles(settings, preset);
        Grid.SetColumn(scopes, 1);
        row.Children.Add(scopes);

        var remove = new Button
        {
            // Segoe Fluent's Remove (minus) glyph.
            Content = new FontIcon { Glyph = "\uE738", FontSize = 12 },
            Padding = new Thickness(8, 4, 8, 5),
            VerticalAlignment = VerticalAlignment.Center,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            remove, Loc.Chrome("settings.formats.delete_preset"));
        remove.Click += async (_, _) => await ConfirmDeletePreset(settings, preset);
        Grid.SetColumn(remove, 2);
        row.Children.Add(remove);
        return row;
    }

    // The inline Track/Release scope toggles for a list row, writing the whole
    // updated preset through SavePresets. A single-file+CUE image is a
    // whole-release export, so the pregap choice fixes its scope: Track reads
    // off, Release reads on, and neither is editable \u2014 the same gating the
    // editor's scope checkboxes apply.
    private StackPanel PresetScopeToggles(Settings settings, SavePreset preset)
    {
        var singleFileCue = preset.PregapPlacement == BridgeSavePregapPlacement.SingleFileWithCue;
        var track = new ToggleButton
        {
            Content = Loc.Chrome("settings.formats.preset_track"),
            IsChecked = !singleFileCue && preset.AppliesToTrack,
            IsEnabled = !singleFileCue,
        };
        var release = new ToggleButton
        {
            Content = Loc.Chrome("settings.formats.preset_release"),
            IsChecked = singleFileCue || preset.AppliesToRelease,
            IsEnabled = !singleFileCue,
        };
        async System.Threading.Tasks.Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.AppliesToTrack = track.IsChecked == true && !singleFileCue;
            preset.AppliesToRelease = release.IsChecked == true || singleFileCue;
            await SavePresets(settings.SavePresets);
        }
        track.Checked += async (_, _) => await Save();
        track.Unchecked += async (_, _) => await Save();
        release.Checked += async (_, _) => await Save();
        release.Unchecked += async (_, _) => await Save();

        var panel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        panel.Children.Add(track);
        panel.Children.Add(release);
        return panel;
    }

    // The edit dialog. Every control inside writes through immediately (the
    // dialog renders from the same mutate-and-save closures the expanders
    // used), so Close is the only button.
    private async System.Threading.Tasks.Task ShowPresetEditor(Settings settings, SavePreset preset)
    {
        if (_dialogRoot() is not { } root)
        {
            return;
        }
        var editor = PresetEditor(settings, preset);
        editor.MinWidth = 420;
        var dialog = new ContentDialog
        {
            Title = preset.Name,
            Content = new ScrollViewer { Content = editor },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = root,
        };
        await dialog.ShowAsync();
    }

    private async System.Threading.Tasks.Task ConfirmDeletePreset(Settings settings, SavePreset preset)
    {
        if (_dialogRoot() is not { } root)
        {
            return;
        }
        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.formats.delete_confirm", "name", preset.Name),
            PrimaryButtonText = Loc.Chrome("action.delete"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = root,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        settings.SavePresets.Remove(preset);
        await SavePresets(settings.SavePresets);
    }

    // The list row's label: name over a codec summary. The Track and Release
    // scopes it appears under are separate inline toggles in the row, not part
    // of this label.
    private static StackPanel PresetHeader(SavePreset preset)
    {
        var titles = new StackPanel { Spacing = 2 };
        titles.Children.Add(new TextBlock
        {
            Text = preset.Name,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        titles.Children.Add(new TextBlock
        {
            Text = PresetSummary(preset),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            FontSize = 12,
        });
        return titles;
    }

    private static string PresetSummary(SavePreset preset) => CodecLabel(preset.Codec);

    // A justified editor row: the label leading, the control trailing.
    private static Grid LabeledRow(string label, FrameworkElement control)
    {
        var row = new Grid();
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.Children.Add(new TextBlock
        {
            Text = label,
            VerticalAlignment = VerticalAlignment.Center,
        });
        Grid.SetColumn(control, 1);
        row.Children.Add(control);
        return row;
    }

    private StackPanel PresetEditor(Settings settings, SavePreset preset)
    {
        // One block on a fixed rhythm, matching macOS's editor: the primary
        // settings up top, bit depth and pregap tucked under Advanced.
        var editor = new StackPanel { Spacing = 11 };
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.formats.preset_name"), PresetNameBox(settings, preset)));
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.formats.preset_format"), PresetFormatCombo(settings, preset)));
        // Bitrate is a lossy preset's primary quality setting, so it stays in
        // the main form; lossless presets carry no bitrate.
        if (LossyBitrate(preset.Codec) is not null)
        {
            editor.Children.Add(PresetBitrateRow(settings, preset));
        }

        // The filename pattern is its own tighter sub-group: label, chip
        // field, add row, and the sample preview.
        var filenameGroup = new StackPanel { Spacing = 6 };
        filenameGroup.Children.Add(new TextBlock { Text = Loc.Chrome("settings.formats.filename_format") });
        var patternEditor = new FilenameTokenEditor(tokens =>
        {
            preset.FilenameTokens = tokens;
            _ = SavePresets(settings.SavePresets);
        });
        patternEditor.Render(preset.FilenameTokens);
        filenameGroup.Children.Add(patternEditor.View);
        var preview = PreviewLine();
        preview.Text = Loc.Chrome(
            "settings.formats.preview",
            "filename",
            SaveFilenameTokenDisplay.PreviewFilename(preset.FilenameTokens, preset.Extension));
        filenameGroup.Children.Add(preview);
        editor.Children.Add(filenameGroup);

        editor.Children.Add(PresetEmbedCoverBox(settings, preset));
        var scopes = PresetScopeBoxes(settings, preset);
        editor.Children.Add(scopes.Row);

        // Advanced: bit depth (lossless only) and pregap placement.
        var advanced = new StackPanel { Spacing = 11 };
        if (LosslessBitDepth(preset.Codec) is not null)
        {
            advanced.Children.Add(PresetBitDepthRow(settings, preset));
        }
        advanced.Children.Add(LabeledRow(
            Loc.Chrome("settings.formats.preset_pregap"),
            PresetPregapCombo(settings, preset, scopes)));
        editor.Children.Add(new Expander
        {
            Header = Loc.Chrome("settings.formats.advanced"),
            Content = advanced,
            IsExpanded = false,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        });
        return editor;
    }

    private CheckBox PresetEmbedCoverBox(Settings settings, SavePreset preset)
    {
        var embed = new CheckBox
        {
            Content = Loc.Chrome("settings.formats.embed_cover"),
            IsChecked = preset.EmbedCover,
        };
        async System.Threading.Tasks.Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.EmbedCover = embed.IsChecked == true;
            await SavePresets(settings.SavePresets);
        }
        embed.Checked += async (_, _) => await Save();
        embed.Unchecked += async (_, _) => await Save();
        return embed;
    }

    private TextBox PresetNameBox(Settings settings, SavePreset preset)
    {
        var name = new TextBox
        {
            Text = preset.Name,
            Width = 200,
        };
        async System.Threading.Tasks.Task Commit()
        {
            if (name.Text is not string text || text == preset.Name)
            {
                return;
            }
            // A blank name never saves (core rejects it); snap the box back
            // to the stored name instead of round-tripping an error.
            if (string.IsNullOrWhiteSpace(text))
            {
                name.Text = preset.Name;
                return;
            }
            preset.Name = text;
            await SavePresets(settings.SavePresets);
        }
        name.LostFocus += async (_, _) => await Commit();
        name.KeyDown += async (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                await Commit();
            }
        };
        return name;
    }

    private ComboBox PresetFormatCombo(Settings settings, SavePreset preset)
    {
        var format = new ComboBox();
        foreach (var kind in PresetKinds)
        {
            format.Items.Add(new ComboBoxItem
            {
                Content = KindLabel(kind),
                Tag = kind,
                IsSelected = kind == CodecKind(preset.Codec),
            });
        }
        format.SelectionChanged += async (_, _) =>
        {
            if (_rendering
                || format.SelectedItem is not ComboBoxItem item
                || item.Tag is not string kind
                || kind == CodecKind(preset.Codec))
            {
                return;
            }
            // A default name follows the format; a name the user typed stays put.
            if (preset.Name == KindLabel(CodecKind(preset.Codec)))
            {
                preset.Name = KindLabel(kind);
            }
            preset.Codec = SwitchedCodec(preset.Codec, kind);
            // The file extension rides on the bridge preset (core derives it from
            // the codec); the config round-trip refreshes it, so it isn't set here.
            if (preset.PregapPlacement == BridgeSavePregapPlacement.SingleFileWithCue
                && preset.Codec is BridgeSaveCodec.OpusOgg or BridgeSaveCodec.Aac)
            {
                preset.PregapPlacement = BridgeSavePregapPlacement.AppendToPreviousExceptHtoa;
            }
            await SavePresets(settings.SavePresets);
        };
        return format;
    }

    // The bit-depth row for lossless presets. It lives in the editor's
    // Advanced group — a secondary setting most presets keep at the source
    // depth. Only built for a lossless codec, which always carries a bit depth.
    private Grid PresetBitDepthRow(Settings settings, SavePreset preset)
    {
        var currentBitDepth = LosslessBitDepth(preset.Codec) ?? BridgeSaveBitDepth.Source;
        var bitDepth = new ComboBox();
        foreach (var (label, value) in BitDepthChoices())
        {
            bitDepth.Items.Add(new ComboBoxItem
            {
                Content = label,
                Tag = value,
                IsSelected = currentBitDepth == value,
            });
        }
        bitDepth.SelectionChanged += async (_, _) =>
        {
            if (_rendering
                || bitDepth.SelectedItem is not ComboBoxItem item
                || item.Tag is not BridgeSaveBitDepth selected
                || selected == LosslessBitDepth(preset.Codec))
            {
                return;
            }
            preset.Codec = preset.Codec switch
            {
                BridgeSaveCodec.Flac => new BridgeSaveCodec.Flac(selected),
                BridgeSaveCodec.Wav => new BridgeSaveCodec.Wav(selected),
                BridgeSaveCodec.Aiff => new BridgeSaveCodec.Aiff(selected),
                _ => preset.Codec,
            };
            await SavePresets(settings.SavePresets);
        };
        return LabeledRow(Loc.Chrome("settings.formats.bit_depth_label"), bitDepth);
    }

    // The bitrate row for lossy presets. Only built for a lossy codec, which
    // always carries a bitrate.
    private Grid PresetBitrateRow(Settings settings, SavePreset preset)
    {
        var bitrate = new TextBox
        {
            Text = (LossyBitrate(preset.Codec) ?? 0).ToString(CultureInfo.InvariantCulture),
            Width = 96,
        };
        async System.Threading.Tasks.Task Commit()
        {
            // Only a parseable, in-range bitrate saves (mirroring core's
            // preset validation); anything else snaps the box back to the
            // stored value instead of round-tripping an error.
            if (!uint.TryParse(bitrate.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var kbps)
                || BitrateRange(preset.Codec) is not { } range
                || kbps < range.Min
                || kbps > range.Max)
            {
                bitrate.Text = (LossyBitrate(preset.Codec) ?? 0).ToString(CultureInfo.InvariantCulture);
                return;
            }
            if (kbps == LossyBitrate(preset.Codec))
            {
                return;
            }
            preset.Codec = preset.Codec switch
            {
                BridgeSaveCodec.Mp3 => new BridgeSaveCodec.Mp3(kbps),
                BridgeSaveCodec.OpusOgg => new BridgeSaveCodec.OpusOgg(kbps),
                _ => preset.Codec,
            };
            await SavePresets(settings.SavePresets);
        }
        bitrate.LostFocus += async (_, _) => await Commit();
        bitrate.KeyDown += async (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                await Commit();
            }
        };
        return LabeledRow(Loc.Chrome("settings.formats.bitrate"), bitrate);
    }

    private (Grid Row, CheckBox Track, CheckBox Release) PresetScopeBoxes(
        Settings settings, SavePreset preset)
    {
        var track = new CheckBox
        {
            Content = Loc.Chrome("settings.formats.preset_track"),
            IsChecked = preset.AppliesToTrack,
        };
        var release = new CheckBox
        {
            Content = Loc.Chrome("settings.formats.preset_release"),
            IsChecked = preset.AppliesToRelease,
        };
        var singleFileCue = preset.PregapPlacement == BridgeSavePregapPlacement.SingleFileWithCue;
        track.IsEnabled = !singleFileCue;
        release.IsEnabled = !singleFileCue;
        async System.Threading.Tasks.Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.AppliesToTrack = track.IsChecked == true;
            preset.AppliesToRelease = release.IsChecked == true;
            await SavePresets(settings.SavePresets);
        }
        track.Checked += async (_, _) => await Save();
        track.Unchecked += async (_, _) => await Save();
        release.Checked += async (_, _) => await Save();
        release.Unchecked += async (_, _) => await Save();

        var boxes = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        boxes.Children.Add(track);
        boxes.Children.Add(release);
        var row = LabeledRow(Loc.Chrome("settings.formats.show_in_menus"), boxes);
        return (row, track, release);
    }

    private ComboBox PresetPregapCombo(
        Settings settings,
        SavePreset preset,
        (StackPanel Row, CheckBox Track, CheckBox Release) scopes)
    {
        var pregap = new ComboBox();
        foreach (var (label, value) in PregapChoices(preset.Codec))
        {
            pregap.Items.Add(new ComboBoxItem
            {
                Content = label,
                Tag = value,
                IsSelected = preset.PregapPlacement == value,
            });
        }
        pregap.SelectionChanged += async (_, _) =>
        {
            if (_rendering
                || pregap.SelectedItem is not ComboBoxItem item
                || item.Tag is not BridgeSavePregapPlacement placement
                || placement == preset.PregapPlacement)
            {
                return;
            }
            preset.PregapPlacement = placement;
            if (placement == BridgeSavePregapPlacement.SingleFileWithCue)
            {
                // A single-file image is inherently a whole-release export.
                // Snapping the checkboxes fires their change handlers; the
                // rendering guard keeps that from saving a second time.
                preset.AppliesToTrack = false;
                preset.AppliesToRelease = true;
                _rendering = true;
                scopes.Track.IsChecked = false;
                scopes.Release.IsChecked = true;
                _rendering = false;
            }
            scopes.Track.IsEnabled = placement != BridgeSavePregapPlacement.SingleFileWithCue;
            scopes.Release.IsEnabled = placement != BridgeSavePregapPlacement.SingleFileWithCue;
            await SavePresets(settings.SavePresets);
        };
        return pregap;
    }

    private Button AddPresetButton()
    {
        var content = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
        };
        // Segoe Fluent's Add (plus) glyph, leading a visible label.
        content.Children.Add(new FontIcon { Glyph = "\uE710", FontSize = 12 });
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.formats.add_preset"),
            VerticalAlignment = VerticalAlignment.Center,
        });
        var add = new Button { Content = content };
        add.Click += async (_, _) => await AddPreset();
        return add;
    }

    // The filename pattern a newly added preset starts with — the zero-padded
    // track number then the title, matching core's default. The user edits it in
    // the preset dialog; there is no global pattern anymore.
    private static List<BridgeSaveFilenameToken> DefaultPresetFilenameTokens() =>
        new() { BridgeSaveFilenameToken.TrackNumber, BridgeSaveFilenameToken.Title };

    // Add a preset with the FLAC default and open its editor: the plus button
    // creates the preset, then the user picks the format and settings in the
    // dialog. The editor opens only after the save lands, so it reads a preset
    // config already holds.
    private async System.Threading.Tasks.Task AddPreset()
    {
        if (_current is not { } settings)
        {
            return;
        }
        var preset = new SavePreset
        {
            Id = Guid.NewGuid().ToString("N"),
            Name = KindLabel("flac"),
            Codec = new BridgeSaveCodec.Flac(BridgeSaveBitDepth.Source),
            // Extension is derived by core from the codec and filled on the config
            // round-trip; the local default ("") is only a placeholder until then.
            FilenameTokens = DefaultPresetFilenameTokens(),
            PregapPlacement = BridgeSavePregapPlacement.AppendToPreviousExceptHtoa,
            AppliesToTrack = true,
            AppliesToRelease = true,
            EmbedCover = true,
        };
        settings.SavePresets.Add(preset);
        if (await SavePresets(settings.SavePresets))
        {
            await ShowPresetEditor(settings, preset);
        }
    }

    // Returns whether the presets persisted, so a caller that opens follow-up
    // UI on the saved preset (add-preset opening its editor) acts only after
    // the write lands.
    private async System.Threading.Tasks.Task<bool> SavePresets(List<SavePreset> presets)
    {
        if (_rendering)
        {
            return false;
        }
        _clearError();
        var (current, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.SetSavePresets(handle, presets));
        if (!current)
        {
            return false;
        }
        if (error is not null)
        {
            _showError(error);
            _settings.Reload();
            return false;
        }
        return true;
    }

    // ── Codec display and switching ──────────────────────────────────────────

    // The codec families the Format picker and the add-preset flyout offer.
    // Format names are proper nouns, shown as-is in every locale.
    private static readonly string[] PresetKinds = { "flac", "mp3", "aac", "opus_ogg", "wav", "aiff" };

    private static string KindLabel(string kind) => kind switch
    {
        "mp3" => "MP3",
        "aac" => "AAC",
        "opus_ogg" => "Opus",
        "wav" => "WAV",
        "aiff" => "AIFF",
        _ => "FLAC",
    };

    private static string CodecKind(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 => "mp3",
        BridgeSaveCodec.Aac => "aac",
        BridgeSaveCodec.OpusOgg => "opus_ogg",
        BridgeSaveCodec.Wav => "wav",
        BridgeSaveCodec.Aiff => "aiff",
        _ => "flac",
    };

    // Switch codec family, carrying the parameter that still applies: bit
    // depth across lossless codecs, bitrate across lossy ones (clamped into
    // the target family's supported range). A cross-family switch takes the
    // family default.
    private static BridgeSaveCodec SwitchedCodec(BridgeSaveCodec codec, string kind)
    {
        var bitDepth = LosslessBitDepth(codec) ?? BridgeSaveBitDepth.Source;
        var bitrate = LossyBitrate(codec);
        return kind switch
        {
            "mp3" => new BridgeSaveCodec.Mp3(Math.Clamp(bitrate ?? 320, 32, 320)),
            "aac" => new BridgeSaveCodec.Aac(Math.Clamp(bitrate ?? 256, 32, 512)),
            "opus_ogg" => new BridgeSaveCodec.OpusOgg(bitrate ?? 192),
            "wav" => new BridgeSaveCodec.Wav(bitDepth),
            "aiff" => new BridgeSaveCodec.Aiff(bitDepth),
            _ => new BridgeSaveCodec.Flac(bitDepth),
        };
    }

    // The lossless family's bit depth; null for lossy codecs.
    private static BridgeSaveBitDepth? LosslessBitDepth(BridgeSaveCodec codec) =>
        codec switch
        {
            BridgeSaveCodec.Flac flac => flac.BitDepth,
            BridgeSaveCodec.Wav wav => wav.BitDepth,
            BridgeSaveCodec.Aiff aiff => aiff.BitDepth,
            _ => null,
        };

    // The lossy family's bitrate; null for lossless codecs.
    private static uint? LossyBitrate(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 mp3 => mp3.BitrateKbps,
        BridgeSaveCodec.Aac aac => aac.BitrateKbps,
        BridgeSaveCodec.OpusOgg opus => opus.BitrateKbps,
        _ => null,
    };

    // The bitrate range core's preset validation accepts for the lossy
    // families; null for lossless, which carry no bitrate.
    private static (uint Min, uint Max)? BitrateRange(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 => (32u, 320u),
        BridgeSaveCodec.Aac => (32u, 512u),
        BridgeSaveCodec.OpusOgg => (32u, 512u),
        _ => null,
    };

    private static string CodecLabel(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 mp3 =>
            Loc.Chrome("settings.formats.codec_mp3", "kbps", mp3.BitrateKbps),
        BridgeSaveCodec.Aac aac =>
            Loc.Chrome("settings.formats.codec_aac", "kbps", aac.BitrateKbps),
        BridgeSaveCodec.OpusOgg opus =>
            Loc.Chrome("settings.formats.codec_opus", "kbps", opus.BitrateKbps),
        BridgeSaveCodec.Wav => "WAV",
        BridgeSaveCodec.Aiff => "AIFF",
        _ => "FLAC",
    };

    private static List<(string Label, BridgeSaveBitDepth Value)> BitDepthChoices() => new()
    {
        (Loc.Chrome("settings.formats.bit_depth.source"), BridgeSaveBitDepth.Source),
        (Loc.Chrome("settings.formats.bit_depth.bits16"), BridgeSaveBitDepth.Bits16),
        (Loc.Chrome("settings.formats.bit_depth.bits24"), BridgeSaveBitDepth.Bits24),
        (Loc.Chrome("settings.formats.bit_depth.bits32"), BridgeSaveBitDepth.Bits32),
    };

    private static List<(string Label, BridgeSavePregapPlacement Value)> PregapChoices(
        BridgeSaveCodec codec)
    {
        var choices = new List<(string Label, BridgeSavePregapPlacement Value)>
        {
            (
                Loc.Chrome("settings.formats.pregap.append_except_htoa"),
                BridgeSavePregapPlacement.AppendToPreviousExceptHtoa
            ),
            (
                Loc.Chrome("settings.formats.pregap.append_including_htoa"),
                BridgeSavePregapPlacement.AppendToPreviousIncludingHtoa
            ),
            (Loc.Chrome("settings.formats.pregap.exclude"), BridgeSavePregapPlacement.Exclude),
        };
        if (codec is not (BridgeSaveCodec.OpusOgg or BridgeSaveCodec.Aac))
        {
            choices.Add((
                Loc.Chrome("settings.formats.pregap.single_file_with_cue"),
                BridgeSavePregapPlacement.SingleFileWithCue
            ));
        }
        return choices;
    }
}
