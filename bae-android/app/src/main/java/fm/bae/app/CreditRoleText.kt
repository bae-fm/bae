package fm.bae.app

import android.content.Context
import android.icu.text.ListFormatter
import uniffi.bae_bridge.BridgeCreditRole

// A credit's role in the person's language. Mirrors macOS's `BridgeCreditRole+Text`.

/**
 * The role in the person's language: its word, then its qualifiers in parentheses joined by the list
 * formatter ("Engineer (assistant)").
 */
fun Context.creditRoleText(role: BridgeCreditRole): String {
    val word = role.role.text(this)
    if (role.qualifiers.isEmpty()) return word
    val qualifiers = ListFormatter.getInstance(currentLocale()).format(role.qualifiers.map { it.text(this) })
    return coreString("core.credit.qualified", mapOf("role" to word, "qualifiers" to qualifiers))
}
