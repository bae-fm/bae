package fm.bae.app

import android.content.Context
import android.icu.text.ListFormatter
import uniffi.bae_bridge.BridgeCreditJoin
import uniffi.bae_bridge.BridgeCreditLine
import uniffi.bae_bridge.BridgeCreditedName
import uniffi.bae_bridge.bridgeCreditJoinKey

// A credit line in the person's language. Mirrors macOS's `BridgeCreditLine+Text`.

/**
 * The credit line in the person's language. Each run of names with no join between them is joined
 * by the list formatter; a recognized join is worded from its catalog message, and any other join
 * is shown as its source wrote it. [joinKey] is core's catalog key for a recognized join; a unit test,
 * which has no native library, passes its own.
 */
fun Context.creditLineText(
    line: BridgeCreditLine,
    joinKey: (BridgeCreditJoin) -> String? = ::bridgeCreditJoinKey,
): String {
    val formatter = ListFormatter.getInstance(currentLocale())
    val runs = mutableListOf<Pair<List<String>, BridgeCreditJoin?>>()
    var run = mutableListOf<String>()
    for (credited in line.names) {
        run.add(credited.name)
        val join = credited.join
        if (join != null) {
            runs.add(run to join)
            run = mutableListOf()
        }
    }
    if (run.isNotEmpty() || runs.isEmpty()) runs.add(run to null)
    var text = formatter.format(runs[0].first)
    for (index in 1 until runs.size) {
        val after = formatter.format(runs[index].first)
        text = joinedCredits(text, runs[index - 1].second, after, joinKey)
    }
    return text
}

private fun Context.joinedCredits(
    before: String,
    join: BridgeCreditJoin?,
    after: String,
    joinKey: (BridgeCreditJoin) -> String?,
): String =
    when (join) {
        null -> {
            before + after
        }

        is BridgeCreditJoin.Unrecognized -> {
            before + join.text + after
        }

        else -> {
            coreString(
                checkNotNull(joinKey(join)) { "a recognized credit join has a catalog message" },
                mapOf("before" to before, "after" to after),
            )
        }
    }

/** A line of one name, for a preview or a fixture. */
fun creditLineNamed(name: String): BridgeCreditLine = BridgeCreditLine(listOf(BridgeCreditedName(name, null)))
