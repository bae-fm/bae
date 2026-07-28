using System.Reflection;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ImportSectionViewTests
{
    private const string CandidateKey = "/Music/Incoming/Collection/Release 01";
    private static bool _avaloniaStarted;

    [Fact]
    public void ReadyCheckboxTapDoesNotActivateItsCandidateRow()
    {
        var view = BuildView(PreviewData.ImportQueue);
        var row = CandidateRow(view);
        var checkbox = row
            .GetLogicalDescendants()
            .OfType<Button>()
            .Single(button => button.Content is Border { Width: 18, Height: 18 });

        RaiseTap(checkbox);

        Assert.Null(SelectedKey(view));
    }

    [Fact]
    public void FolderDecisionClearsTheSelectedCandidateBeforeDispatch()
    {
        var view = BuildView(PreviewData.ImportResolvedQueue);
        var row = CandidateRow(view);
        RaiseTap(row);
        Assert.Equal(CandidateKey, SelectedKey(view));
        var decision = DecisionButton(row);

        decision.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Null(SelectedKey(view));
    }

    [Fact]
    public void FolderDecisionTapDoesNotReactivateItsCandidateRow()
    {
        var view = BuildView(PreviewData.ImportResolvedQueue);
        var row = CandidateRow(view);
        var decision = DecisionButton(row);

        RaiseTap(decision);

        Assert.Null(SelectedKey(view));
    }

    private static ImportSectionView BuildView(BridgeTriageQueue queue)
    {
        if (!_avaloniaStarted)
        {
            AppBuilder
                .Configure<App>()
                .UseHeadless(new AvaloniaHeadlessPlatformOptions())
                .SetupWithoutStarting();
            _avaloniaStarted = true;
        }
        var import = new ImportService
        {
            CandidateForKey = _ => (true, null),
            SetFolderReleaseDecision = (_, _) =>
                Task.FromResult((true, (string?)null)),
        };
        var playback = new PlaybackService
        {
            PreviewStop = () => true,
        };
        var app = AppService.Stubbed(
            new SessionStore(Dispatcher.UIThread),
            Dispatcher.UIThread,
            EmptyLibrary(),
            import,
            playback);
        var view = new ImportSectionView(
            app,
            new ImportDialogs(
                new ModalHost(),
                new LightboxOverlay(),
                _ => Task.CompletedTask));
        app.ImportStore.SeedPreview(
            queue,
            PreviewData.ImportWatchedFolders,
            BridgeTriageTab.Ready);
        return view;
    }

    private static LibraryService EmptyLibrary() => new()
    {
        AlbumCount = () => (true, 0L),
        AlbumPage = (_, _, _) => (true, (new List<Album>(), (string?)null)),
        ComposerCount = () => (true, 0L),
        ComposerPage = (_, _, _) => (true, (new List<ComposerSummary>(), (string?)null)),
        ArtistCount = () => (true, 0L),
        ArtistPage = (_, _, _) => (true, (new List<ArtistSummary>(), (string?)null)),
    };

    private static Control CandidateRow(ImportSectionView view) =>
        view
            .GetLogicalDescendants()
            .OfType<Control>()
            .Single(control => Equals(control.Tag, CandidateKey));

    private static Button DecisionButton(Control row) =>
        row
            .GetLogicalDescendants()
            .OfType<Button>()
            .Single(button => Equals(
                button.Content,
                Loc.Chrome("import.release.one")));

    private static string? SelectedKey(ImportSectionView view) =>
        (string?)typeof(ImportSectionView)
            .GetField("_selectedKey", BindingFlags.Instance | BindingFlags.NonPublic)!
            .GetValue(view);

    private static void RaiseTap(Control control) =>
        control.RaiseEvent(new TappedEventArgs(Gestures.TappedEvent, null!));
}
