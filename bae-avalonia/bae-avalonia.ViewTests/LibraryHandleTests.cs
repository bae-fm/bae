using System;
using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class LibraryHandleTests
{
    [Fact]
    public void TeardownFreesTheHandleWhenGracefulShutdownFails()
    {
        var calls = new List<string>();

        Assert.Throws<InvalidOperationException>(() =>
            LibraryHandle.CompleteTeardown(
                () =>
                {
                    calls.Add("shutdown");
                    throw new InvalidOperationException("shutdown failed");
                },
                () => calls.Add("free")));

        Assert.Equal(new[] { "shutdown", "free" }, calls);
    }
}
