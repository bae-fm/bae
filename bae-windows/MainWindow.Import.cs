using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

// MainWindow: the toolbar clicks, window folder-drop, folder import, and the
// queue-button drag/drop. Split out of MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    private async void OnCloseLibraryClick(object sender, RoutedEventArgs e)
    {
        await CloseLibrary();
    }

    private void OnShuffleLibraryClick(object sender, RoutedEventArgs e)
    {
        WithCurrentHandle(NativeBae.PlayLibraryShuffled);
    }

    private async void OnLibrariesClick(object sender, RoutedEventArgs e)
    {
        await _librariesDialog.Show();
    }

    private async void OnImportClick(object sender, RoutedEventArgs e)
    {
        await _importDialog.Show();
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
    // candidates stream into the import store, and the dialog (bound to that list)
    // shows them. Scanning runs off the UI thread; errors surface in the banner.
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

        await ImportFolder(folderPath);
    }

    // Scan a folder and open the import dialog on its candidates — candidates
    // stream into the import store and the dialog (bound to that list) shows
    // them, on a scan error too, matching macOS, which navigates to import
    // regardless of the scan result. Shared by the window drop target and a
    // folder activation intent (the folder verb or bae://import); the caller
    // has already confirmed a library is open.
    private async System.Threading.Tasks.Task ImportFolder(string folderPath)
    {
        var (current, error) = await _import.ScanFolder(folderPath);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            ShowImportBanner(error);
        }

        // Skip if one is already open (only one ContentDialog can open at a time).
        if (!_importDialog.IsOpen)
        {
            await _importDialog.Show();
        }
    }

    private void ShowImportBanner(string message)
    {
        _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("import.error_title"), message);
    }

    private void OnQueueClick(object sender, RoutedEventArgs e)
    {
        _queuePane.Toggle();
    }

    // Start a drag from an album card: carry the album ids as the newline-joined
    // payload the queue pane decodes — the whole multi-selection (visible order)
    // when the pressed card is part of it, else just that card. Cancelled when no
    // library is open. Never mutates the selection. Per-card (the grid ListView's
    // own drag would carry the row), so the album is the card's DataContext.
    private void OnAlbumCardDragStarting(UIElement sender, DragStartingEventArgs e)
    {
        if (CurrentHandleOrNull() == null || (sender as FrameworkElement)?.DataContext is not Album album)
        {
            e.Cancel = true;
            return;
        }
        var ids = _albumSelection.OrderedTargets(album.Id, AlbumPosition);
        e.Data.SetText(QueueDragPayload.Encode(ids));
        e.Data.RequestedOperation = DataPackageOperation.Copy;
    }

    // The queue button is also an append drop target: a card dropped on it adds
    // the album's tracks to the end of the manual lane, and the +N badge animates
    // from core's queue-items-added event.
    private void OnQueueButtonDragOver(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.Text))
        {
            return;
        }
        e.AcceptedOperation = DataPackageOperation.Copy;
        if (e.DragUIOverride is not null)
        {
            e.DragUIOverride.Caption = Loc.Chrome("menu.add_to_queue");
        }
        e.Handled = true;
    }

    private async void OnQueueButtonDrop(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.Text))
        {
            return;
        }
        e.Handled = true;
        await _queuePane.HandleButtonAppendDrop(e);
    }
}
