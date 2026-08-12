using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class InitialLoadErrorViewTests
{
    [AvaloniaFact]
    public async Task RendersLocalizedFailureAndRunsRetry()
    {
        var retried = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var view = new InitialLoadErrorView(() =>
        {
            retried.SetResult();
            return Task.CompletedTask;
        });

        Assert.Equal(Loc.Chrome("library.load_failed"), view.ErrorText.Text);
        Assert.Equal(Loc.Chrome("library.retry"), view.RetryButton.Content);

        view.RetryButton.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
        await retried.Task;
    }
}
