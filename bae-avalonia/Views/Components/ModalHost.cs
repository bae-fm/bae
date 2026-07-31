using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;

namespace Bae.Desktop;

// The in-window modal host: a scrim over the window content with one dialog card
// centered on it. A window (Welcome, Main) owns one host as its top layer; a
// dialog is a UserControl presented in it. Only real OS windows stay real windows
// (Welcome, Main, Settings); every other dialog shows here.
internal sealed class ModalHost : Panel
{
    private readonly Border _scrim;
    private readonly ContentControl _card;
    private TaskCompletionSource<bool>? _closed;

    public ModalHost()
    {
        IsVisible = false;

        _scrim = new Border { HorizontalAlignment = HorizontalAlignment.Stretch, VerticalAlignment = VerticalAlignment.Stretch };
        _scrim[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeScrimBrush");

        _card = new ContentControl
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            // A cap wide enough for the storage sheet's table; narrower dialogs set
            // their own MinWidth and stay content-sized well under it.
            MaxWidth = 820,
            Margin = new Thickness(24),
        };

        Children.Add(_scrim);
        Children.Add(_card);
    }

    // Present a dialog control and complete when it closes. The dialog calls the
    // supplied close callback (with an outcome) to dismiss itself.
    public Task Show(Func<Action, Control> build)
    {
        var closed = new TaskCompletionSource<bool>();
        _closed = closed;
        _card.Content = WrapCard(build(() => Close()));
        IsVisible = true;
        return closed.Task;
    }

    public void Close()
    {
        IsVisible = false;
        _card.Content = null;
        _closed?.TrySetResult(true);
        _closed = null;
    }

    // The dialog card chrome: a themed rounded surface with a hairline, holding
    // the dialog's own content.
    private static Control WrapCard(Control content)
    {
        var card = new Border
        {
            CornerRadius = new CornerRadius(12),
            Padding = new Thickness(24),
            BorderThickness = new Thickness(1),
            Child = content,
        };
        card[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");
        card[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        return card;
    }
}
