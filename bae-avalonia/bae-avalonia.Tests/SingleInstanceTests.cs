using Xunit;

namespace Bae.Desktop.Tests;

// Opening bae while it runs hands the launch to the running copy; a launch
// whose handoff fails takes over when that copy is gone, and the running
// copy's listener survives a failure handling one launch.
public sealed class SingleInstanceTests
{
    [Fact]
    public void ALaunchHandsItsArgumentsToTheRunningCopy()
    {
        var edition = Edition();
        var forwarded = new BlockingQueue();
        using var running = SingleInstance.Acquire(edition, [], forwarded.Add);
        Assert.NotNull(running);

        var second = OnOtherThread(() => SingleInstance.Acquire(edition, ["bae://open"], _ => { }));

        Assert.Null(second);
        Assert.Equal(["bae://open"], forwarded.Take());
    }

    [Fact]
    public void AFailedHandoffTakesOverWhenTheRunningCopyIsGone()
    {
        var edition = Edition();
        // A copy that took the instance and ended without giving it back.
        OnOtherThread(() => new Mutex(initiallyOwned: true, SingleInstance.Name(edition), out _));

        var launch = OnOtherThread(() => SingleInstance.Acquire(edition, [], _ => { }));

        Assert.NotNull(launch);
        launch.Dispose();
    }

    [Fact]
    public void TheListenerKeepsListeningAfterAFailedActivation()
    {
        var edition = Edition();
        var forwarded = new BlockingQueue();
        var activations = 0;
        using var running = SingleInstance.Acquire(edition, [], args =>
        {
            if (Interlocked.Increment(ref activations) == 1)
            {
                throw new InvalidOperationException("the window is gone");
            }
            forwarded.Add(args);
        });

        OnOtherThread(() => SingleInstance.Acquire(edition, ["first"], _ => { }));
        OnOtherThread(() => SingleInstance.Acquire(edition, ["second"], _ => { }));

        Assert.Equal(["second"], forwarded.Take());
    }

    // Short: the pipe is a socket under the temp dir on Unix, whose path macOS
    // caps at 104 characters.
    private static string Edition() => $"t{Guid.NewGuid().ToString("N")[..8]}";

    // A second launch runs on a thread of its own: a mutex is recursive on the
    // thread that holds it, so the same thread would always win it again.
    private static T OnOtherThread<T>(Func<T> launch)
    {
        T result = default!;
        var thread = new Thread(() => result = launch());
        thread.Start();
        thread.Join();
        return result;
    }

    private sealed class BlockingQueue
    {
        private readonly System.Collections.Concurrent.BlockingCollection<IReadOnlyList<string>> _items = new();

        public void Add(IReadOnlyList<string> args) => _items.Add(args);

        public IReadOnlyList<string> Take()
        {
            Assert.True(_items.TryTake(out var args, TimeSpan.FromSeconds(10)), "no launch was handed over");
            return args!;
        }
    }
}
