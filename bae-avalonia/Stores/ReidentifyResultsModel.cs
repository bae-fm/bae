using System.Collections.Generic;
using System.Linq;

namespace Bae.Desktop;

// Arbitrates the re-identify dialog's results list between its two producers:
// the auto-identify pipeline (live — it replays on every candidate
// invalidation, including badge-only updates) and a manual search the user
// ran. A successful manual search takes the list over; pipeline refreshes
// keep updating the status line and signal badges but leave the results
// alone until a pipeline interaction (signal toggle, re-run) hands it back.
// Pipeline renders are also change-gated: replaying an identical match set
// must not rebuild the list, because rebuilding drops the user's selection.
public sealed class ReidentifyResultsModel
{
    private List<string> _shownPipelineKeys = new();

    public bool ManualResultsShown { get; private set; }

    // Whether this pipeline refresh replaces the rendered results list.
    // False while manual results own the list, and false when the match
    // set is unchanged from the last render.
    public bool ApplyPipelineMatches(IReadOnlyList<string> matchKeys)
    {
        if (ManualResultsShown || _shownPipelineKeys.SequenceEqual(matchKeys))
        {
            return false;
        }
        _shownPipelineKeys = matchKeys.ToList();
        return true;
    }

    public void ApplyManualResults() => ManualResultsShown = true;

    // A pipeline interaction (signal toggle / re-run) asks the pipeline for
    // results again: the next pipeline refresh re-renders unconditionally.
    public void ResumePipeline()
    {
        ManualResultsShown = false;
        _shownPipelineKeys = new List<string>();
    }
}
