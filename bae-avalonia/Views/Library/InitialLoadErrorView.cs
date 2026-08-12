using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;

namespace Bae.Desktop;

internal sealed class InitialLoadErrorView : StackPanel
{
    internal TextBlock ErrorText { get; }
    internal Button RetryButton { get; }

    public InitialLoadErrorView(Func<Task> retry)
    {
        HorizontalAlignment = HorizontalAlignment.Center;
        VerticalAlignment = VerticalAlignment.Center;
        Spacing = 8;

        ErrorText = new TextBlock
        {
            Text = Loc.Chrome("library.load_failed"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontSize = 18,
        };
        ErrorText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        RetryButton = new Button
        {
            Content = Loc.Chrome("library.retry"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        RetryButton.Click += async (_, _) =>
        {
            RetryButton.IsEnabled = false;
            try
            {
                await retry();
            }
            finally
            {
                RetryButton.IsEnabled = true;
            }
        };
        Children.Add(ErrorText);
        Children.Add(RetryButton);
    }
}
