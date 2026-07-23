using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using uniffi.bae_bridge;
using Windows.System;

namespace Bae.Windows;

// Per-track actions from the detail dialog: exporting a single track and the
// flash-on-realize highlight when the list scrolls to it. Split out of
// AlbumDetailDialog.cs unchanged.
internal sealed partial class AlbumDetailDialog
{
    // The per-track "Save As…": choose a track-applicable preset, seed the
    // filename from the configured template, then save to the picked path. Errors
    // surface in the album-detail status line, which the modal doesn't occlude
    // here (the pickers are OS dialogs).
    private async System.Threading.Tasks.Task ExportTrack(Track track, TextBlock statusLine)
    {
        statusLine.Visibility = Visibility.Collapsed;
        var picker = new global::Windows.Storage.Pickers.FileSavePicker();
        WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
        var (settingsCurrent, settings) = await _session.RunForCurrentHandle(NativeBae.GetSettings);
        if (!settingsCurrent)
        {
            return;
        }
        var trackPresets = settings.SavePresets
            .Where(preset => preset.AppliesToTrack)
            .ToList();
        // Config validation guarantees at least one track-applicable preset and a
        // valid default; guard anyway rather than crash if that's ever violated.
        if (trackPresets.Count == 0)
        {
            statusLine.Text = Loc.Chrome("track.export.prepare_failed");
            statusLine.Visibility = Visibility.Visible;
            return;
        }
        var formatPicker = new ComboBox
        {
            Header = Loc.Chrome("settings.formats.default_track_format"),
            MinWidth = 260,
        };
        var defaultIndex = 0;
        for (var index = 0; index < trackPresets.Count; index++)
        {
            if (trackPresets[index].Id == settings.DefaultTrackSavePreset)
            {
                defaultIndex = index;
            }
            formatPicker.Items.Add(new ComboBoxItem
            {
                Content = trackPresets[index].TrackPickerLabel,
            });
        }
        formatPicker.SelectedIndex = defaultIndex;
        var formatDialog = new ContentDialog
        {
            Title = Loc.Chrome("save.title"),
            Content = formatPicker,
            PrimaryButtonText = Loc.Chrome("action.save"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };
        var formatResult = await formatDialog.ShowAsync();
        if (formatResult != ContentDialogResult.Primary)
        {
            return;
        }
        if (formatPicker.SelectedIndex < 0)
        {
            statusLine.Text = Loc.Chrome("track.export.prepare_failed");
            statusLine.Visibility = Visibility.Visible;
            return;
        }
        var selectedPreset = trackPresets[formatPicker.SelectedIndex];
        picker.FileTypeChoices.Add(
            selectedPreset.TrackPickerLabel,
            new List<string> { selectedPreset.FileExtension });
        // Seed the suggested name from the chosen preset's filename pattern,
        // which the core renders and sanitizes from this track's metadata. The
        // format dialog runs before the picker, so one call suffices. A null
        // return — or a throw — means that render failed (the core logged the
        // cause); surface it and abort rather than saving under a guessed name.
        string? stem;
        try
        {
            var (nameCurrent, suggestedName) = await _session.RunForCurrentHandle(
                handle => NativeBae.SaveTrackSuggestedName(
                    handle, track.TrackId, selectedPreset.Id));
            if (!nameCurrent)
            {
                return;
            }
            stem = suggestedName;
        }
        catch (Exception ex)
        {
            BaeDiagnostics.Logger.Error("save suggested-name lookup threw", ex);
            stem = null;
        }
        if (stem is null)
        {
            statusLine.Text = Loc.Chrome("track.export.prepare_failed");
            statusLine.Visibility = Visibility.Visible;
            return;
        }
        picker.SuggestedFileName = stem;
        var file = await picker.PickSaveFileAsync();
        if (file is null)
        {
            return;
        }

        var path = file.Path;
        var (saveCurrent, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.SaveTrack(handle, track.TrackId, path, selectedPreset.Id));
        if (!saveCurrent)
        {
            return;
        }
        if (error is not null)
        {
            statusLine.Text = error;
            statusLine.Visibility = Visibility.Visible;
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
}
