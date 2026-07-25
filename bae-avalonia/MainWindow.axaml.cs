using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Markup.Xaml.MarkupExtensions;
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

    public MainWindow(SessionStore session, Func<Task> closeLibrary)
    {
        _ = closeLibrary; // wired to the shell's close-library affordance in the parity port
        _session = session;
        _app = new AppService(session, Dispatcher.UIThread);
        _session.UiEvent += _app.UiEventRouter.Route;

        Title = "bae";
        Width = 1350;
        Height = 850;
        this[!BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");

        // The in-window modal host over the shell: the album-detail action dialogs
        // (change cover, and the rest as the family grows) present here.
        var modalHost = new ModalHost();
        var dialogs = new ReleaseActionDialogs(_app, modalHost);
        var root = new Panel();
        root.Children.Add(new MainShellView(_app, dialogs));
        root.Children.Add(modalHost);
        Content = root;

        // Subscribe to core's UI events once the window is up (the handle is
        // already open; the subscription fences stale deliveries by generation).
        Opened += (_, _) => _session.Subscribe();
    }
}
