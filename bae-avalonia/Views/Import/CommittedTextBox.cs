using System;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;

namespace Bae.Desktop;

/// <summary>
/// A text box whose value lives somewhere else — a row in the database — and
/// which decides when to send what was typed there.
///
/// One write is one commit, and a commit redraws whatever reads it. That is
/// right per settled value and wrong per keystroke, so the box keeps what is
/// being typed and sends it on the three moments a person means "this is the
/// value": leaving the field, pressing Return, and pausing.
/// </summary>
internal static class CommittedTextBox
{
    /// <summary>How long a pause counts as "done typing".</summary>
    internal static readonly TimeSpan CommitDelay = TimeSpan.FromMilliseconds(400);

    /// <summary>Send <paramref name="box"/>'s text to <paramref name="onCommit"/>
    /// on focus loss, on Return, and after a pause. A value that is already
    /// what was handed in is not an edit and sends nothing.</summary>
    internal static TextBox Commits(this TextBox box, string value, Action<string> onCommit)
    {
        box.Text = value;
        var stored = value;
        DispatcherTimer? pending = null;

        void Commit()
        {
            pending?.Stop();
            pending = null;
            var typed = box.Text ?? string.Empty;
            if (typed == stored)
            {
                return;
            }
            stored = typed;
            onCommit(typed);
        }

        box.TextChanged += (_, _) =>
        {
            pending?.Stop();
            pending = new DispatcherTimer { Interval = CommitDelay };
            pending.Tick += (_, _) => Commit();
            pending.Start();
        };
        box.LostFocus += (_, _) => Commit();
        box.KeyDown += (_, args) =>
        {
            if (args.Key == Key.Enter)
            {
                Commit();
            }
        };
        return box;
    }
}
