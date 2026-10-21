package fm.bae.app.data

import fm.bae.app.DisplayError

/**
 * A live subscription's latest answer: the value it last delivered, or that
 * its last read failed. A failure replaces the value, so nothing shows a
 * value that is no longer known to be current; the next value replaces the
 * failure.
 */
sealed interface LiveValue<out Value> {
    data class Current<Value>(
        val value: Value,
    ) : LiveValue<Value>

    data class Failed(
        val error: DisplayError,
    ) : LiveValue<Nothing>
}
