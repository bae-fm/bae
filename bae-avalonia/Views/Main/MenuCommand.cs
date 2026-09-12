using System;
using System.Windows.Input;

namespace Bae.Desktop;

/// <summary>
/// The command behind one menu item and the key binding that carries its
/// shortcut — both hold this same instance, so the item and the shortcut run
/// the identical action and share one notion of whether it is available.
///
/// <c>canRun</c> is asked each time the answer matters: a menu item
/// disables itself on it, and a key binding leaves the keystroke unhandled when
/// it says no, so the key goes on to whatever else would take it. It is
/// re-asked only when <see cref="RaiseCanExecuteChanged"/> says the answer may
/// have moved.
/// </summary>
internal sealed class MenuCommand : ICommand
{
    private readonly Action _run;
    private readonly Func<bool>? _canRun;

    public MenuCommand(Action run, Func<bool>? canRun = null)
    {
        _run = run;
        _canRun = canRun;
    }

    public event EventHandler? CanExecuteChanged;

    public bool CanExecute(object? parameter) => _canRun?.Invoke() ?? true;

    public void Execute(object? parameter) => _run();

    public void RaiseCanExecuteChanged() => CanExecuteChanged?.Invoke(this, EventArgs.Empty);
}
