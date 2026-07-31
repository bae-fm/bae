using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The per-release actions reachable from album detail: export a release, delete
// it, edit its metadata, re-identify it, browse its gallery, and change its
// cover. Each opens its own dialog (a nested ContentDialog can't open over album
// detail, so album detail closes first and calls into here). Errors that would be
// occluded by a modal surface inside the dialog; the rest go to the status line.
internal sealed partial class ReleaseActionDialogs
{
    private readonly SessionStore _session;
    private readonly ImageStore _images;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly Action<string> _setStatus;
    private readonly ProjectionRegistry _projections;
    private readonly LightboxOverlay _lightbox;

    public ReleaseActionDialogs(
        SessionStore session,
        ImageStore images,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        Action<string> setStatus,
        ProjectionRegistry projections,
        LightboxOverlay lightbox)
    {
        _session = session;
        _images = images;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _setStatus = setStatus;
        _projections = projections;
        _lightbox = lightbox;
    }

    // Verbatim export: no format choice, just a destination folder. The picked
    // folder is remembered in OutputFolderStore as the last-used output folder
    // (the Windows sibling of macOS's lastExportFolder).
    public async System.Threading.Tasks.Task ShowExportRelease(string releaseId)
    {
        var targetDir = await PickOutputFolder();
        if (targetDir is null)
        {
            return;
        }
        var (exportCurrent, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.ExportRelease(handle, releaseId, targetDir));
        if (!exportCurrent)
        {
            return;
        }
        if (error is not null)
        {
            _setStatus(error);
        }
    }

    // Release-level save: pick a release-applicable preset, then a destination
    // folder, then enqueue the save (the preset is captured whole at enqueue).
    public async System.Threading.Tasks.Task ShowSaveReleaseAs(string releaseId)
    {
        var (settingsCurrent, settings) = await _session.RunForCurrentHandle(NativeBae.GetSettings);
        if (!settingsCurrent)
        {
            return;
        }
        var releasePresets = settings.SavePresets
            .Where(preset => preset.AppliesToRelease)
            .ToList();
        // Config validation guarantees a release-applicable preset and a valid
        // default; guard anyway rather than crash if that's ever violated.
        if (releasePresets.Count == 0)
        {
            _setStatus(Loc.Chrome("track.export.prepare_failed"));
            return;
        }

        var formatPicker = new ComboBox
        {
            Header = Loc.Chrome("settings.formats.default_release_format"),
            MinWidth = 260,
        };
        var defaultIndex = 0;
        for (var index = 0; index < releasePresets.Count; index++)
        {
            if (releasePresets[index].Id == settings.DefaultReleaseSavePreset)
            {
                defaultIndex = index;
            }
            formatPicker.Items.Add(new ComboBoxItem
            {
                Content = releasePresets[index].Name,
                Tag = releasePresets[index].Id,
            });
        }
        formatPicker.SelectedIndex = defaultIndex;

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("save.title"),
            Content = formatPicker,
            PrimaryButtonText = Loc.Chrome("action.save"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary)
        {
            return;
        }
        if (formatPicker.SelectedItem is not ComboBoxItem selectedItem
            || selectedItem.Tag is not string presetId)
        {
            _setStatus(Loc.Chrome("track.export.prepare_failed"));
            return;
        }

        var targetDir = await PickOutputFolder();
        if (targetDir is null)
        {
            return;
        }
        var (saveCurrent, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.SaveRelease(handle, releaseId, targetDir, presetId));
        if (!saveCurrent)
        {
            return;
        }
        if (error is not null)
        {
            _setStatus(error);
        }
    }

    // Prompt for a destination folder, remembering the pick as the last-used
    // output folder. Returns null when the user cancels.
    private async System.Threading.Tasks.Task<string?> PickOutputFolder()
    {
        var picker = new global::Windows.Storage.Pickers.FolderPicker();
        picker.FileTypeFilter.Add("*");
        WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
        var folder = await picker.PickSingleFolderAsync();
        if (folder is null)
        {
            return null;
        }
        OutputFolderStore.Save(folder.Path);
        return folder.Path;
    }

    public async System.Threading.Tasks.Task ConfirmDeleteRelease(string releaseId)
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
            XamlRoot = _xamlRoot(),
        };
        if (await confirm.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }

        // On success an invalidation refreshes the grid.
        var (current, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.DeleteRelease(handle, releaseId));
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            await DialogPrimitives.ShowError(_xamlRoot(), Loc.Chrome("album.delete.failed"), error);
        }
    }

}
