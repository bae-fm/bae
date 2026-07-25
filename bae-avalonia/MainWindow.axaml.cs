using System;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Platform.Storage;
using Avalonia.Threading;

namespace Bae.Desktop;

// The library window: the shell over one open library. Built only when a library
// actually opens (App.OpenLibrary), it composes an AppService around the already-
// open session, routes core's UI events into it, and hosts the shell. Native
// window chrome, sized to the story-3 shell. The library grid, queue, and live
// transport arrive with the parity port; this is the empty-library shell.
internal sealed class MainWindow : Window
{
    private readonly SessionStore _session;
    private readonly AppService _app;
    private readonly MainShellView _shell;
    private readonly Func<Task> _closeLibrary;
    private readonly Func<string, Task> _switchLibrary;

    public MainWindow(SessionStore session, Func<Task> closeLibrary, Func<string, Task> switchLibrary)
    {
        _closeLibrary = closeLibrary;
        _switchLibrary = switchLibrary;
        _session = session;
        _app = new AppService(session, Dispatcher.UIThread);
        _session.UiEvent += _app.UiEventRouter.Route;

        Title = "bae";
        Width = 1350;
        Height = 850;
        this[!BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");

        // The in-window overlays over the shell: the modal host for the album-detail
        // and import dialogs, and the lightbox for the galleries. Both present above
        // the shell (the lightbox topmost).
        var modalHost = new ModalHost();
        var lightbox = new LightboxOverlay();
        var dialogs = new ReleaseActionDialogs(_app, modalHost, lightbox);
        // The import dialog's "view in library" jump reveals an album in the shell;
        // the shell is built just below, so the callback reads the field at call time.
        var importDialogs = new ImportDialogs(_app, modalHost, lightbox, albumId => _shell.OpenAlbum(albumId));
        var storageDialog = new StorageDialog(_app, modalHost);
        // The library manager (switch / rename / add) and the settings window's
        // Lock + Remove sections both drive the coordinator's window swap, so the
        // switch and close callbacks are threaded to each.
        var librariesDialog = new LibrariesDialog(_app, modalHost, switchLibrary);
        // The settings window is a real window (like macOS's Settings scene) with
        // its own modal host for its sub-dialogs; the gear opens it.
        var settingsWindow = new SettingsWindow(_app, closeLibrary, switchLibrary);
        _shell = new MainShellView(
            _app, dialogs, importDialogs, storageDialog, settingsWindow, librariesDialog, closeLibrary);
        var root = new Panel();
        root.Children.Add(_shell);
        root.Children.Add(modalHost);
        root.Children.Add(lightbox);
        Content = root;

        // A folder dropped on the window scans it into the import watched set — the
        // same action a folder dropped on the exe or a bae://import intent runs.
        DragDrop.SetAllowDrop(root, true);
        root.AddHandler(DragDrop.DragOverEvent, (_, e) =>
        {
            e.DragEffects = e.Data.Contains(DataFormats.Files) ? DragDropEffects.Copy : e.DragEffects;
        });
        root.AddHandler(DragDrop.DropEvent, OnWindowDrop);

        // Subscribe to core's UI events once the window is up (the handle is
        // already open; the subscription fences stale deliveries by generation).
        // The session outlives this window across a swap, so detach this window's
        // router when it closes — otherwise a swapped-out AppService keeps handling
        // events on the shared session.
        Opened += (_, _) => _session.Subscribe();
        Closed += (_, _) => _session.UiEvent -= _app.UiEventRouter.Route;

        // Ctrl+1..9 switches to the nth library on this device; Ctrl+Shift+W closes
        // the library back to the welcome chooser.
        KeyDown += OnKeyDown;
    }

    private async void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (!e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            return;
        }

        if (e.KeyModifiers.HasFlag(KeyModifiers.Shift) && e.Key == Key.W)
        {
            e.Handled = true;
            await _closeLibrary();
            return;
        }

        if (SwitchDigit(e.Key) is not { } digit)
        {
            return;
        }
        var libraries = LibraryDiscovery.Load(_ => { })
            .Where(library => library.Error is null)
            .Select(library => (library.Id, library.IsActive))
            .ToList();
        if (LibrarySwitchModel.TargetLibraryId(libraries, digit) is { } target)
        {
            e.Handled = true;
            await _switchLibrary(target);
        }
    }

    // Map the number-row and numpad digit keys 1..9 onto a switch digit; other keys
    // are not switch shortcuts.
    private static int? SwitchDigit(Key key) => key switch
    {
        >= Key.D1 and <= Key.D9 => key - Key.D1 + 1,
        >= Key.NumPad1 and <= Key.NumPad9 => key - Key.NumPad1 + 1,
        _ => null,
    };

    private async void OnWindowDrop(object? sender, DragEventArgs e)
    {
        if (_session.CurrentHandleOrNull() is null)
        {
            return;
        }
        var files = e.Data.GetFiles();
        if (files is null)
        {
            return;
        }
        // Match macOS / the WinUI window drop: the first dropped item must be a
        // folder. Scan it, then land on the import section showing its candidates —
        // the same dispatch a folder-verb / bae://import activation runs.
        foreach (var item in files)
        {
            if (item is IStorageFolder && item.TryGetLocalPath() is { } path)
            {
                var (current, _) = await _app.ImportStore.ScanFolder(path);
                if (current)
                {
                    _shell.ShowImport();
                }
                return;
            }
        }
    }
}
