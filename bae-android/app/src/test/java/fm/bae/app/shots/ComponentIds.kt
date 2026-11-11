package fm.bae.app.shots

import java.io.File

/**
 * The ids of component shots (`android.component.<thing>.<state>`) are the design system's: `<thing>` is
 * the name of a component under `design/system/components` after `Android`, and `<state>` is one of the
 * named rows of its `preview.html` (`N('Opens a page', …)` is `opens-a-page`), each in lower-case words
 * joined by `-`. A component whose preview names no row has the one state `default`.
 */
internal object ComponentIds {
    private const val PREFIX = "android.component."

    /** Refuses a component shot id that names no component, or a state its preview doesn't have. */
    fun check(id: String) {
        if (!id.startsWith(PREFIX)) return
        val (thing, state) = id.removePrefix(PREFIX).split('.', limit = 2).let { it[0] to it.getOrNull(1) }
        require(state != null) { "component shot '$id' names no state" }
        val name = "Android" + thing.split('-').joinToString("") { it.replaceFirstChar(Char::uppercase) }
        val preview = File(componentsDirectory(), "$name/preview.html")
        require(preview.isFile) { "component shot '$id': the design system has no component $name" }
        val states = states(preview.readText()).ifEmpty { listOf("default") }
        require(state in states) { "component shot '$id': $name has the states ${states.joinToString()}" }
    }

    /** The ids of the named rows of a preview page: each `N('…', …)` or `W('…', …)` call. */
    internal fun states(html: String): List<String> =
        Regex("""\b[NW]\('((?:[^'\\]|\\.)*)'""")
            .findAll(html)
            .map { id(it.groupValues[1].replace("\\'", "'")) }
            .toList()

    private fun id(name: String) = Regex("[A-Za-z0-9]+").findAll(name).joinToString("-") { it.value.lowercase() }

    private fun componentsDirectory(): File {
        var directory: File? = File(System.getProperty("user.dir")).absoluteFile
        while (directory != null) {
            val candidate = File(directory, "design/system/components")
            if (candidate.isDirectory) return candidate
            directory = directory.parentFile
        }
        error("design/system/components not found above ${System.getProperty("user.dir")}")
    }
}
