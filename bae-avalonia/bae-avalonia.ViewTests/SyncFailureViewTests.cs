using System.Threading.Tasks;
using Avalonia.Headless.XUnit;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class SyncFailureViewTests
{
    /// The row exists so the fault is on screen, not one control-click away: both
    /// lines render, and the retry for the configured provider comes with them.
    [AvaloniaFact]
    public void AFailureShowsItsCategoryLineAndItsFault()
    {
        var view = new SyncFailureView(() => Task.CompletedTask);

        view.Render(
            "Something went wrong.",
            "sync cycle: pull Store commits: database: retained Merge replay "
                + "has an unresolved foreign-key dependency", true);

        Assert.True(view.IsVisible);
        Assert.Equal("Something went wrong.", view.LineText.Text);
        Assert.True(view.DetailText.IsVisible);
        Assert.Contains("retained Merge replay", view.DetailText.Text);
        Assert.Equal(Loc.Chrome("sync.reconnect"), view.ReconnectButton.Content);
    }

    /// A keyed failure has no chain behind it. One line, and no empty second one
    /// implying a fault nobody named.
    [AvaloniaFact]
    public void AFailureWithoutAFaultShowsOneLine()
    {
        var view = new SyncFailureView(() => Task.CompletedTask);

        view.Render("Something went wrong.", null, true);

        Assert.True(view.IsVisible);
        Assert.False(view.DetailText.IsVisible);
    }

    [AvaloniaFact]
    public void AnAppUpdateCannotBeRetriedByReconnecting()
    {
        var view = new SyncFailureView(() => Task.CompletedTask);
        view.Render("Update the app to continue syncing.", "schema 17 required", false);
        Assert.True(view.IsVisible);
        Assert.False(view.ReconnectButton.IsVisible);
        view.Render("Could not reach the cloud.", "connection reset", true);
        Assert.True(view.ReconnectButton.IsVisible);
    }

    /// Healthy sync is not a failure with empty text — the row is gone.
    [AvaloniaFact]
    public void HealthySyncHidesTheRow()
    {
        var view = new SyncFailureView(() => Task.CompletedTask);
        view.Render("Something went wrong.", "connection reset", true);

        view.Render(null, null, false);

        Assert.False(view.IsVisible);
    }
}
