using System.Globalization;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.System;

namespace Bae.Windows;

// The settings dialog's Export section: the release-export destination popup
// and default format, the single-track filename pattern (token chips with a
// preview) and default format, and the export presets as expanders whose
// controls save on every change. Writes round-trip through config invalidation
// into the settings re-read (Render) with no optimistic mutation; preset edits
// send the whole set (set-state), never one mutated field. Mirrors macOS's
// ExportSettingsTab.
internal sealed class ExportSettingsSection
{
    public StackPanel View { get; } = new() { Spacing = 8 };

    private readonly SessionStore _session;
    private readonly SettingsStore _settings;
    private readonly Func<XamlRoot?> _dialogRoot;
    private readonly Action<string> _showError;
    private readonly Action _clearError;

    private readonly ComboBox _defaultTrack = new() { Header = Loc.Chrome("settings.export.default_track_format") };
    private readonly ComboBox _defaultRelease = new() { Header = Loc.Chrome("settings.export.default_release_format") };
    private readonly FilenameTokenEditor _patternEditor;
    private readonly TextBlock _patternPreview = PreviewLine();
    private readonly StackPanel _presetPanel = new() { Spacing = 8 };

    private bool _rendering;

    // The settings snapshot the section last rendered — the list the preset
    // controls mutate and save whole. Set before the section is interactive
    // (the dialog renders once at open).
    private Settings? _current;

    public ExportSettingsSection(
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
        _patternEditor = new FilenameTokenEditor(
            tokens => _ = SavePatternTokens(tokens));

        _defaultTrack.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultTrack, release: false);
        _defaultRelease.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultRelease, release: true);

        View.Children.Add(SectionLabel(Loc.Chrome("settings.export.release_exports")));
        View.Children.Add(_defaultRelease);
        View.Children.Add(SectionLabel(Loc.Chrome("settings.export.track_exports")));
        View.Children.Add(_defaultTrack);
        View.Children.Add(new TextBlock { Text = Loc.Chrome("settings.export.filename_format") });
        View.Children.Add(_patternEditor.View);
        View.Children.Add(_patternPreview);
        View.Children.Add(SectionLabel(Loc.Chrome("settings.export.presets")));
        View.Children.Add(_presetPanel);
        View.Children.Add(AddPresetButton());
        View.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.export.presets_footer"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
    }

    // Drive every control from the persisted settings. Called on open and on
    // every config-invalidation re-read; in-progress local drafts (a name being
    // typed) live in the rebuilt controls and re-seed from the fresh values.
    public void Render(Settings settings)
    {
        _current = settings;
        _rendering = true;
        _patternEditor.Render(settings.ExportFilenameTokens);
        _patternPreview.Text = Loc.Chrome(
            "settings.export.preview",
            "filename",
            // "Original" keeps the source format, so the sample shows a
            // representative extension; preset defaults preview themselves in
            // their own rows below.
            ExportFilenameTokenDisplay.PreviewFilename(settings.ExportFilenameTokens, "flac"));
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

    // ── Filename pattern and default formats ────────────────────────────────

    private async System.Threading.Tasks.Task SavePatternTokens(
        List<BridgeExportFilenameToken> tokens)
    {
        if (_rendering)
        {
            return;
        }
        _clearError();
        var (current, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.SetExportFilenameTokens(handle, tokens));
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

    private void PopulateSelection(ComboBox combo, Settings settings, bool release)
    {
        combo.Items.Clear();
        var selected = release
            ? settings.DefaultReleaseSavePreset
            : settings.DefaultTrackSavePreset;
        foreach (var preset in settings.ExportPresets.Where(
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
        foreach (var preset in settings.ExportPresets)
        {
            _presetPanel.Children.Add(PresetRow(settings, preset));
        }
    }

    // One preset in the list: the summary row opens the edit dialog; the
    // trailing minus removes the preset behind a confirmation. Mirrors macOS's
    // PresetRow.
    private Grid PresetRow(Settings settings, ExportPreset preset)
    {
        var row = new Grid { ColumnSpacing = 12 };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
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

        var remove = new Button
        {
            // Segoe Fluent's Remove (minus) glyph.
            Content = new FontIcon { Glyph = "\uE738", FontSize = 12 },
            Padding = new Thickness(8, 4, 8, 5),
            VerticalAlignment = VerticalAlignment.Center,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            remove, Loc.Chrome("settings.export.delete_preset"));
        remove.Click += async (_, _) => await ConfirmDeletePreset(settings, preset);
        Grid.SetColumn(remove, 1);
        row.Children.Add(remove);
        return row;
    }

    // The edit dialog. Every control inside writes through immediately (the
    // dialog renders from the same mutate-and-save closures the expanders
    // used), so Close is the only button.
    private async System.Threading.Tasks.Task ShowPresetEditor(Settings settings, ExportPreset preset)
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

    private async System.Threading.Tasks.Task ConfirmDeletePreset(Settings settings, ExportPreset preset)
    {
        if (_dialogRoot() is not { } root)
        {
            return;
        }
        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.export.delete_confirm", "name", preset.Name),
            PrimaryButtonText = Loc.Chrome("action.delete"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = root,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        settings.ExportPresets.Remove(preset);
        await SavePresets(settings.ExportPresets);
    }

    // The collapsed row: name over a settings summary, with the export menus
    // the preset appears in as trailing badges.
    private static Grid PresetHeader(ExportPreset preset)
    {
        var header = new Grid();
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

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
        header.Children.Add(titles);

        var badges = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (preset.AppliesToTrack)
        {
            badges.Children.Add(ScopeBadge(Loc.Chrome("settings.export.preset_track")));
        }
        if (preset.AppliesToRelease)
        {
            badges.Children.Add(ScopeBadge(Loc.Chrome("settings.export.preset_release")));
        }
        Grid.SetColumn(badges, 1);
        header.Children.Add(badges);
        return header;
    }

    private static Border ScopeBadge(string text) => new()
    {
        Child = new TextBlock { Text = text, FontSize = 11 },
        CornerRadius = new CornerRadius(9),
        Padding = new Thickness(9, 1, 9, 2),
        Background = new SolidColorBrush(Microsoft.UI.Colors.Gray) { Opacity = 0.25 },
        VerticalAlignment = VerticalAlignment.Center,
    };

    private static string PresetSummary(ExportPreset preset)
    {
        var parts = new List<string> { CodecLabel(preset.Codec) };
        if (LosslessBitDepth(preset.Codec) is { } bitDepth)
        {
            parts.Add(BitDepthSummaryLabel(bitDepth));
        }
        parts.Add(PregapLabel(preset.PregapPlacement));
        return string.Join(" · ", parts);
    }

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

    private StackPanel PresetEditor(Settings settings, ExportPreset preset)
    {
        // One block on a fixed rhythm, matching macOS's expanded editor.
        var editor = new StackPanel { Spacing = 11 };
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.export.preset_name"), PresetNameBox(settings, preset)));
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.export.preset_format"), PresetFormatCombo(settings, preset)));
        editor.Children.Add(PresetCodecRow(settings, preset));
        var scopes = PresetScopeBoxes(settings, preset);
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.export.preset_pregap"),
            PresetPregapCombo(settings, preset, scopes)));

        // The filename pattern is its own tighter sub-group: label, chip
        // field, add row, and the sample preview.
        var filenameGroup = new StackPanel { Spacing = 6 };
        filenameGroup.Children.Add(new TextBlock { Text = Loc.Chrome("settings.export.filename_format") });
        var patternEditor = new FilenameTokenEditor(tokens =>
        {
            preset.FilenameTokens = tokens;
            _ = SavePresets(settings.ExportPresets);
        });
        patternEditor.Render(preset.FilenameTokens);
        filenameGroup.Children.Add(patternEditor.View);
        var preview = PreviewLine();
        preview.Text = Loc.Chrome(
            "settings.export.preview",
            "filename",
            ExportFilenameTokenDisplay.PreviewFilename(preset.FilenameTokens, preset.Extension));
        filenameGroup.Children.Add(preview);
        editor.Children.Add(filenameGroup);

        editor.Children.Add(scopes.Row);
        return editor;
    }

    private TextBox PresetNameBox(Settings settings, ExportPreset preset)
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
            await SavePresets(settings.ExportPresets);
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

    private ComboBox PresetFormatCombo(Settings settings, ExportPreset preset)
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
            preset.Codec = SwitchedCodec(preset.Codec, kind);
            // The file extension rides on the bridge preset (core derives it from
            // the codec); the config round-trip refreshes it, so it isn't set here.
            if (preset.PregapPlacement == BridgeExportPregapPlacement.SingleFileWithCue
                && preset.Codec is BridgeExportPresetCodec.OpusOgg)
            {
                preset.PregapPlacement = BridgeExportPregapPlacement.AppendToPreviousExceptHtoa;
            }
            await SavePresets(settings.ExportPresets);
        };
        return format;
    }

    // The bit depth or bitrate row, per the codec family: lossless codecs
    // carry a bit depth, lossy ones a bitrate.
    private Grid PresetCodecRow(Settings settings, ExportPreset preset)
    {
        if (LosslessBitDepth(preset.Codec) is { } currentBitDepth)
        {
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
                    || item.Tag is not BridgeExportBitDepth selected
                    || selected == LosslessBitDepth(preset.Codec))
                {
                    return;
                }
                preset.Codec = preset.Codec switch
                {
                    BridgeExportPresetCodec.Flac => new BridgeExportPresetCodec.Flac(selected),
                    BridgeExportPresetCodec.Wav => new BridgeExportPresetCodec.Wav(selected),
                    BridgeExportPresetCodec.Aiff => new BridgeExportPresetCodec.Aiff(selected),
                    _ => preset.Codec,
                };
                await SavePresets(settings.ExportPresets);
            };
            return LabeledRow(Loc.Chrome("settings.export.bit_depth_label"), bitDepth);
        }

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
                BridgeExportPresetCodec.Mp3 => new BridgeExportPresetCodec.Mp3(kbps),
                BridgeExportPresetCodec.OpusOgg => new BridgeExportPresetCodec.OpusOgg(kbps),
                _ => preset.Codec,
            };
            await SavePresets(settings.ExportPresets);
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
        return LabeledRow(Loc.Chrome("settings.export.bitrate"), bitrate);
    }

    private (StackPanel Row, CheckBox Track, CheckBox Release) PresetScopeBoxes(
        Settings settings, ExportPreset preset)
    {
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
        var singleFileCue = preset.PregapPlacement == BridgeExportPregapPlacement.SingleFileWithCue;
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
            await SavePresets(settings.ExportPresets);
        }
        track.Checked += async (_, _) => await Save();
        track.Unchecked += async (_, _) => await Save();
        release.Checked += async (_, _) => await Save();
        release.Unchecked += async (_, _) => await Save();

        var boxes = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        boxes.Children.Add(track);
        boxes.Children.Add(release);
        var row = LabeledRow(Loc.Chrome("settings.export.show_in_menus"), boxes);
        return (row, track, release);
    }

    private ComboBox PresetPregapCombo(
        Settings settings,
        ExportPreset preset,
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
                || item.Tag is not BridgeExportPregapPlacement placement
                || placement == preset.PregapPlacement)
            {
                return;
            }
            preset.PregapPlacement = placement;
            if (placement == BridgeExportPregapPlacement.SingleFileWithCue)
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
            scopes.Track.IsEnabled = placement != BridgeExportPregapPlacement.SingleFileWithCue;
            scopes.Release.IsEnabled = placement != BridgeExportPregapPlacement.SingleFileWithCue;
            await SavePresets(settings.ExportPresets);
        };
        return pregap;
    }

    private DropDownButton AddPresetButton()
    {
        var flyout = new MenuFlyout();
        foreach (var kind in PresetKinds)
        {
            var item = new MenuFlyoutItem { Text = KindLabel(kind) };
            item.Click += async (_, _) => await AddPreset(kind);
            flyout.Items.Add(item);
        }
        return new DropDownButton
        {
            Content = Loc.Chrome("settings.export.add_preset"),
            Flyout = flyout,
        };
    }

    private async System.Threading.Tasks.Task AddPreset(string kind)
    {
        if (_current is not { } settings)
        {
            return;
        }
        var codec = SwitchedCodec(new BridgeExportPresetCodec.Flac(BridgeExportBitDepth.Source), kind);
        var preset = new ExportPreset
        {
            Id = Guid.NewGuid().ToString("N"),
            Name = KindLabel(kind),
            Codec = codec,
            // Extension is derived by core from the codec and filled on the config
            // round-trip; the local default ("") is only a placeholder until then.
            FilenameTokens = settings.ExportFilenameTokens.ToList(),
            PregapPlacement = BridgeExportPregapPlacement.AppendToPreviousExceptHtoa,
            AppliesToTrack = true,
            AppliesToRelease = true,
        };
        settings.ExportPresets.Add(preset);
        await SavePresets(settings.ExportPresets);
    }

    private async System.Threading.Tasks.Task SavePresets(List<ExportPreset> presets)
    {
        if (_rendering)
        {
            return;
        }
        _clearError();
        var (current, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.SetExportPresets(handle, presets));
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

    // ── Codec display and switching ──────────────────────────────────────────

    // The codec families the Format picker and the add-preset flyout offer.
    // Format names are proper nouns, shown as-is in every locale.
    private static readonly string[] PresetKinds = { "flac", "mp3", "opus_ogg", "wav", "aiff" };

    private static string KindLabel(string kind) => kind switch
    {
        "mp3" => "MP3",
        "opus_ogg" => "Opus",
        "wav" => "WAV",
        "aiff" => "AIFF",
        _ => "FLAC",
    };

    private static string CodecKind(BridgeExportPresetCodec codec) => codec switch
    {
        BridgeExportPresetCodec.Mp3 => "mp3",
        BridgeExportPresetCodec.OpusOgg => "opus_ogg",
        BridgeExportPresetCodec.Wav => "wav",
        BridgeExportPresetCodec.Aiff => "aiff",
        _ => "flac",
    };

    // Switch codec family, carrying the parameter that still applies: bit
    // depth across lossless codecs, bitrate across lossy ones (clamped into
    // MP3's supported range). A cross-family switch takes the family default.
    private static BridgeExportPresetCodec SwitchedCodec(BridgeExportPresetCodec codec, string kind)
    {
        var bitDepth = LosslessBitDepth(codec) ?? BridgeExportBitDepth.Source;
        var bitrate = LossyBitrate(codec);
        return kind switch
        {
            "mp3" => new BridgeExportPresetCodec.Mp3(Math.Clamp(bitrate ?? 320, 32, 320)),
            "opus_ogg" => new BridgeExportPresetCodec.OpusOgg(bitrate ?? 192),
            "wav" => new BridgeExportPresetCodec.Wav(bitDepth),
            "aiff" => new BridgeExportPresetCodec.Aiff(bitDepth),
            _ => new BridgeExportPresetCodec.Flac(bitDepth),
        };
    }

    // The lossless family's bit depth; null for lossy codecs.
    private static BridgeExportBitDepth? LosslessBitDepth(BridgeExportPresetCodec codec) =>
        codec switch
        {
            BridgeExportPresetCodec.Flac flac => flac.BitDepth,
            BridgeExportPresetCodec.Wav wav => wav.BitDepth,
            BridgeExportPresetCodec.Aiff aiff => aiff.BitDepth,
            _ => null,
        };

    // The lossy family's bitrate; null for lossless codecs.
    private static uint? LossyBitrate(BridgeExportPresetCodec codec) => codec switch
    {
        BridgeExportPresetCodec.Mp3 mp3 => mp3.BitrateKbps,
        BridgeExportPresetCodec.OpusOgg opus => opus.BitrateKbps,
        _ => null,
    };

    // The bitrate range core's preset validation accepts for the lossy
    // families; null for lossless, which carry no bitrate.
    private static (uint Min, uint Max)? BitrateRange(BridgeExportPresetCodec codec) => codec switch
    {
        BridgeExportPresetCodec.Mp3 => (32u, 320u),
        BridgeExportPresetCodec.OpusOgg => (32u, 512u),
        _ => null,
    };

    private static string CodecLabel(BridgeExportPresetCodec codec) => codec switch
    {
        BridgeExportPresetCodec.Mp3 mp3 =>
            Loc.Chrome("settings.export.codec_mp3", "kbps", mp3.BitrateKbps),
        BridgeExportPresetCodec.OpusOgg opus =>
            Loc.Chrome("settings.export.codec_opus", "kbps", opus.BitrateKbps),
        BridgeExportPresetCodec.Wav => "WAV",
        BridgeExportPresetCodec.Aiff => "AIFF",
        _ => "FLAC",
    };

    // The summary line's bit-depth part: "Source" alone reads as nothing in a
    // "FLAC · Source · …" join, so the source case names what it is.
    private static string BitDepthSummaryLabel(BridgeExportBitDepth bitDepth) => bitDepth switch
    {
        BridgeExportBitDepth.Source => Loc.Chrome("settings.export.bit_depth.source_summary"),
        BridgeExportBitDepth.Bits16 => Loc.Chrome("settings.export.bit_depth.bits16"),
        BridgeExportBitDepth.Bits24 => Loc.Chrome("settings.export.bit_depth.bits24"),
        _ => Loc.Chrome("settings.export.bit_depth.bits32"),
    };

    private static List<(string Label, BridgeExportBitDepth Value)> BitDepthChoices() => new()
    {
        (Loc.Chrome("settings.export.bit_depth.source"), BridgeExportBitDepth.Source),
        (Loc.Chrome("settings.export.bit_depth.bits16"), BridgeExportBitDepth.Bits16),
        (Loc.Chrome("settings.export.bit_depth.bits24"), BridgeExportBitDepth.Bits24),
        (Loc.Chrome("settings.export.bit_depth.bits32"), BridgeExportBitDepth.Bits32),
    };

    private static string PregapLabel(BridgeExportPregapPlacement placement) => placement switch
    {
        BridgeExportPregapPlacement.AppendToPreviousExceptHtoa =>
            Loc.Chrome("settings.export.pregap.append_except_htoa"),
        BridgeExportPregapPlacement.AppendToPreviousIncludingHtoa =>
            Loc.Chrome("settings.export.pregap.append_including_htoa"),
        BridgeExportPregapPlacement.Exclude => Loc.Chrome("settings.export.pregap.exclude"),
        _ => Loc.Chrome("settings.export.pregap.single_file_with_cue"),
    };

    private static List<(string Label, BridgeExportPregapPlacement Value)> PregapChoices(
        BridgeExportPresetCodec codec)
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
        if (codec is not BridgeExportPresetCodec.OpusOgg)
        {
            choices.Add((
                Loc.Chrome("settings.export.pregap.single_file_with_cue"),
                BridgeExportPregapPlacement.SingleFileWithCue
            ));
        }
        return choices;
    }
}
