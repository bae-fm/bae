using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Bae.Desktop;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        AvaloniaXamlLoader.Load(this);
        this.FindControl<TextBlock>("Subtitle")!.Text = Loc.Chrome("welcome.subtitle");
    }
}
