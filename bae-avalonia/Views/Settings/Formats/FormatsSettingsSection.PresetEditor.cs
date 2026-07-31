using System;
using System.Collections.Generic;
using System.Globalization;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The save-preset editor form: the per-field controls (name, format, bit depth,
// bitrate, pregap, embed-cover), the Add button, and the persist path. Split out
// of FormatsSettingsSection.cs unchanged.
internal sealed partial class FormatsSettingsSection
{
    private StackPanel PresetEditor(Settings settings, SavePreset preset)
    {
        // One block on a fixed rhythm, matching macOS's editor: the primary
        // settings up top, bit depth and pregap tucked under Advanced.
        var editor = new StackPanel { Spacing = 11 };
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.formats.preset_name"), PresetNameBox(settings, preset)));
        editor.Children.Add(LabeledRow(
            Loc.Chrome("settings.formats.preset_format"), PresetFormatCombo(settings, preset)));
        // Bitrate is a lossy preset's primary quality setting, so it stays in the
        // main form; lossless presets carry no bitrate.
        if (LossyBitrate(preset.Codec) is not null)
        {
            editor.Children.Add(PresetBitrateRow(settings, preset));
        }

        // The filename pattern is its own tighter sub-group: label, chip field,
        // add row, and the sample preview.
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
        embed.IsCheckedChanged += async (_, _) =>
        {
            if (_rendering)
            {
                return;
            }
            preset.EmbedCover = embed.IsChecked == true;
            await SavePresets(settings.SavePresets);
        };
        return embed;
    }

    private TextBox PresetNameBox(Settings settings, SavePreset preset)
    {
        var name = new TextBox { Text = preset.Name, Width = 200 };
        async System.Threading.Tasks.Task Commit()
        {
            if (name.Text is not string text || text == preset.Name)
            {
                return;
            }
            // A blank name never saves (core rejects it); snap the box back to the
            // stored name instead of round-tripping an error.
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
            if (args.Key == Key.Enter)
            {
                args.Handled = true;
                await Commit();
            }
        };
        return name;
    }

    private ComboBox PresetFormatCombo(Settings settings, SavePreset preset)
    {
        var format = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        var kinds = new List<(string Label, string Value)>();
        foreach (var kind in PresetKinds)
        {
            kinds.Add((KindLabel(kind), kind));
        }
        FillTagCombo(format, kinds, CodecKind(preset.Codec));
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

    // The bit-depth row for lossless presets. It lives in the editor's Advanced
    // group — a secondary setting most presets keep at the source depth. Only
    // built for a lossless codec, which always carries a bit depth.
    private Grid PresetBitDepthRow(Settings settings, SavePreset preset)
    {
        var currentBitDepth = LosslessBitDepth(preset.Codec) ?? BridgeSaveBitDepth.Source;
        var bitDepth = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        FillTagCombo(bitDepth, BitDepthChoices(), currentBitDepth);
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
            // Only a parseable, in-range bitrate saves (mirroring core's preset
            // validation); anything else snaps the box back to the stored value
            // instead of round-tripping an error.
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
                BridgeSaveCodec.Aac => new BridgeSaveCodec.Aac(kbps),
                BridgeSaveCodec.OpusOgg => new BridgeSaveCodec.OpusOgg(kbps),
                _ => preset.Codec,
            };
            await SavePresets(settings.SavePresets);
        }
        bitrate.LostFocus += async (_, _) => await Commit();
        bitrate.KeyDown += async (_, args) =>
        {
            if (args.Key == Key.Enter)
            {
                args.Handled = true;
                await Commit();
            }
        };
        return LabeledRow(Loc.Chrome("settings.formats.bitrate"), bitrate);
    }

    private ComboBox PresetPregapCombo(
        Settings settings,
        SavePreset preset,
        (Grid Row, CheckBox Track, CheckBox Release) scopes)
    {
        var pregap = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        FillTagCombo(pregap, PregapChoices(preset.Codec), preset.PregapPlacement);
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
                // A single-file image is inherently a whole-release export. Snapping
                // the checkboxes fires their change handlers; the rendering guard
                // keeps that from saving a second time.
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
            Children =
            {
                new TextBlock { Text = "+", VerticalAlignment = VerticalAlignment.Center },
                new TextBlock { Text = Loc.Chrome("settings.formats.add_preset"), VerticalAlignment = VerticalAlignment.Center },
            },
        };
        var add = new Button { Content = content };
        add.Click += async (_, _) => await AddPreset();
        return add;
    }

    // The filename pattern a newly added preset starts with — the zero-padded track
    // number then the title, matching core's default. The user edits it in the
    // preset dialog; there is no global pattern anymore.
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

    // Returns whether the presets persisted, so a caller that opens follow-up UI on
    // the saved preset (add-preset opening its editor) acts only after the write
    // lands.
    private async System.Threading.Tasks.Task<bool> SavePresets(List<SavePreset> presets)
    {
        if (_rendering)
        {
            return false;
        }
        _clearError();
        var (current, error) = await _app.Downloads.SetSavePresets(presets);
        if (!current)
        {
            return false;
        }
        if (error is not null)
        {
            _showError(error);
            _app.SettingsStore.Reload();
            return false;
        }
        return true;
    }
}
