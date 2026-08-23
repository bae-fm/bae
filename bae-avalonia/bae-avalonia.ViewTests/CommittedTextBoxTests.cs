using System;
using System.Collections.Generic;
using System.Threading;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// A field whose value is a row in the database. One write is one commit and a
/// commit redraws whatever reads it, so what these check is when the box
/// decides a person means "this is the value" — and, just as much, when it
/// decides they do not.
/// </summary>
public sealed class CommittedTextBoxTests
{
    [AvaloniaFact]
    public void TypingOnItsOwnCommitsNothing()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.Text = "2";
        box.Text = "20";
        box.Text = "201";

        Assert.Empty(written);
    }

    [AvaloniaFact]
    public void LeavingTheFieldCommitsWhatIsInIt()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.Text = "2011";
        box.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Equal(new[] { "2011" }, written);
    }

    [AvaloniaFact]
    public void ReturnCommitsWithoutLeavingTheField()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.Text = "2011";
        box.RaiseEvent(new KeyEventArgs
        {
            RoutedEvent = InputElement.KeyDownEvent,
            Key = Key.Enter,
        });

        Assert.Equal(new[] { "2011" }, written);
    }

    [AvaloniaFact]
    public void PausingCommitsWhatWasTyped()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.Text = "2011";
        WaitOutTheCommitDelay();

        Assert.Equal(new[] { "2011" }, written);
    }

    // The three moments are one commit between them, not three: whichever
    // arrives first sends the value, and the rest find nothing new to send.
    [AvaloniaFact]
    public void APauseAndThenLeavingCommitsOnce()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.Text = "2011";
        WaitOutTheCommitDelay();
        box.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Equal(new[] { "2011" }, written);
    }

    // Leaving a field nobody touched writes nothing: a redraw per focus change
    // would be a write per focus change.
    [AvaloniaFact]
    public void LeavingAnUntouchedFieldCommitsNothing()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Empty(written);
    }

    // Typing a value back to what it already was is not an edit either.
    [AvaloniaFact]
    public void TypingTheStoredValueBackCommitsNothing()
    {
        var written = new List<string>();
        var box = Committed("1996", written);

        box.Text = "199";
        box.Text = "1996";
        box.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Empty(written);
    }

    private static TextBox Committed(string value, List<string> written) =>
        new TextBox().Commits(value, written.Add);

    // The pause is a DispatcherTimer, and a timer only fires from inside the
    // dispatcher's loop — draining the job queue is not enough. So the test
    // runs the loop for a little longer than the delay and lets it end itself.
    private static void WaitOutTheCommitDelay()
    {
        using var stop = new CancellationTokenSource(
            CommittedTextBox.CommitDelay + TimeSpan.FromMilliseconds(400));
        Dispatcher.UIThread.MainLoop(stop.Token);
    }
}
