using System.Collections.Generic;
using Avalonia.Headless.XUnit;
using Avalonia.Threading;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class LatestUiValueTests
{
    [AvaloniaFact]
    public void PendingDeliveryUsesTheNewestProgressValue()
    {
        var received = new List<int>();
        var delivery = new LatestUiValue<int>(received.Add);

        delivery.Offer(1);
        delivery.Offer(2);
        delivery.Offer(3);
        Dispatcher.UIThread.RunJobs();

        Assert.Equal(new[] { 3 }, received);
    }
}
