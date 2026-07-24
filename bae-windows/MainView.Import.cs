using System;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;

namespace Bae.Windows;

// MainView: the whole-window folder drop target. A folder dropped anywhere over
// the window is scanned and imported — the same dispatch a folder-verb / bae://
// activation intent runs (HandleActivationIntent). The view reads the folder
// path off the drop (window input) and hands it to ImportDialog; the scan-and-show
// work lives there.
public sealed partial class MainView : UserControl
{
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

    // Read the first dropped folder's path and hand it to the import dialog, which
    // scans it and opens on its candidates. Mirrors the macOS window drop. Reading
    // the drop payload runs off the UI thread; a read error surfaces in the banner.
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

        await _importSection.ImportFolder(folderPath);
    }

    private void ShowImportBanner(string message)
    {
        _appService.ShowError(Loc.Chrome("import.error_title"), message);
    }
}
