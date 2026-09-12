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
// window chrome, sized to the story-3 shell. Placement persistence lives in
// MainWindow.WindowBounds.cs.
internal sealed partial class MainWindow : Window
{
    private readonly SessionStore _session;
    private readonly AppService _app;
    private readonly MainShellView _shell;
    private readonly Func<string, Task> _switchLibrary;

    public MainWindow(
        SessionStore session,
        IMediaControl mediaControl,
        UpdateService updates,
        AppearanceStore appearance,
        Func<Task> closeLibrary,
        Func<string, Task> switchLibrary,
        Func<Task> applyUpdateAndRestart)
    {
        _switchLibrary = switchLibrary;
        _session = session;
        _app = new AppService(session, Dispatcher.UIThread, mediaControl);
        _session.UiEvent += _app.UiEventRouter.Route;

        Title = "bae";
        // The placement for a first launch, before anything has been saved; a saved
        // rect replaces it, and tracking then follows the window from here on.
        Width = 1350;
        Height = 850;
        RestoreWindowBounds();
        TrackWindowBounds();
        this[!BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");

        // The in-window overlays over the shell: the modal host for the album-detail
        // and import dialogs, and the lightbox for the galleries. Both present above
        // the shell (the lightbox topmost).
        var modalHost = new ModalHost();
        var lightbox = new LightboxOverlay();
        var dialogs = new ReleaseActionDialogs(_app, modalHost, lightbox);
        // The import dialog's "view in library" jump reveals an album in the shell;
        // the shell is built just below, so the callback reads the field at call time.
        var importDialogs = new ImportDialogs(modalHost, lightbox, _app.Images, albumId => _shell.OpenAlbum(albumId));
        var storageDialog = new StorageDialog(_app, modalHost);
        // One command set behind every playback control: the menu bar's items and
        // their shortcuts, and the now-playing bar's transport.
        var playback = PlaybackCommands.ForApp(_app);
        // The library manager (switch / rename / add) and the settings window's
        // Lock + Remove sections both drive the coordinator's window swap, so the
        // switch and close callbacks are threaded to each.
        var librariesDialog = new LibrariesDialog(_app, modalHost, switchLibrary);
        // The settings window is a real window (like macOS's Settings scene) with
        // its own modal host for its sub-dialogs. Its updates section drives the
        // process-wide update service, and applying a staged update exits the app,
        // so the coordinator owns that path too.
        var settingsWindow = new SettingsWindow(
            _app, appearance, updates, closeLibrary, switchLibrary, applyUpdateAndRestart);
        _shell = new MainShellView(_app, playback, dialogs, importDialogs);

        // The menu bar over the shell, carrying the library commands and the
        // playback transport. Its shortcuts, and the library-switch digits, are
        // the window's key bindings, so every shortcut in the app is declared the
        // same way.
        var menuBar = new MainMenuBar(
            playback,
            openLibraries: () => _ = librariesDialog.Show(),
            openStorage: () => _ = storageDialog.Show(),
            openSettings: settingsWindow.Show,
            closeLibrary: () => _ = closeLibrary());
        foreach (var binding in menuBar.WindowKeyBindings)
        {
            KeyBindings.Add(binding);
        }
        // Ctrl+1..9 switches to the nth library on this device, from the number
        // row or the numpad.
        for (var digit = 1; digit <= 9; digit++)
        {
            KeyBindings.Add(SwitchLibraryBinding(Key.D1 + (digit - 1), digit));
            KeyBindings.Add(SwitchLibraryBinding(Key.NumPad1 + (digit - 1), digit));
        }

        var shellWithMenu = new DockPanel();
        DockPanel.SetDock(menuBar.View, Dock.Top);
        shellWithMenu.Children.Add(menuBar.View);
        shellWithMenu.Children.Add(_shell);

        var root = new Panel();
        root.Children.Add(shellWithMenu);
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
        Closed += (_, _) =>
        {
            _session.UiEvent -= _app.UiEventRouter.Route;
            _shell.Dispose();
            _app.Dispose();
        };
    }

    private KeyBinding SwitchLibraryBinding(Key key, int digit) => new()
    {
        Gesture = new KeyGesture(key, KeyModifiers.Control),
        Command = new MenuCommand(() => _ = SwitchToLibrary(digit)),
    };

    // Switch to the nth library this device holds, skipping the ones that failed
    // to load. A digit past the end names no library and does nothing.
    private async Task SwitchToLibrary(int digit)
    {
        var libraries = LibraryDiscovery.Load(_ => { })
            .Where(library => library.Error is null)
            .Select(library => (library.Id, library.IsActive))
            .ToList();
        if (LibrarySwitchModel.TargetLibraryId(libraries, digit) is { } target)
        {
            await _switchLibrary(target);
        }
    }

    // Run what an activation asks for. ImportFolder is the whole intent set, so an
    // intent arriving here unhandled is a defect in the parse, not something a
    // user did — it throws rather than being absorbed.
    internal Task HandleActivationIntent(ActivationIntent intent) => intent switch
    {
        ActivationIntent.ImportFolder importFolder => ImportFolder(importFolder.Path),
        _ => throw new ArgumentOutOfRangeException(nameof(intent), intent, "Unknown activation intent"),
    };

    private async void OnWindowDrop(object? sender, DragEventArgs e)
    {
        var files = e.Data.GetFiles();
        if (files is null)
        {
            return;
        }
        // Import is folder-scoped, as on macOS: take the first dropped folder and
        // ignore loose files.
        foreach (var item in files)
        {
            if (item is IStorageFolder && item.TryGetLocalPath() is { } path)
            {
                await ImportFolder(path);
                return;
            }
        }
    }

    // Scan a folder into the import watched set, then land on the import section
    // showing its candidates. The one action behind every way a folder reaches the
    // app: dropped on the window, dropped on the executable, opened through the
    // folder verb, or named by a bae://import link. The section switch waits on the
    // scan because the handle it ran against may have been swapped out underneath
    // it — those candidates then belong to a library that is no longer open.
    private async Task ImportFolder(string path)
    {
        var (current, _) = await _app.ImportStore.ScanFolder(path);
        if (current)
        {
            _shell.ShowImport();
        }
    }
}
