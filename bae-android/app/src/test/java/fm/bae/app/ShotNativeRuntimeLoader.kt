package fm.bae.app

import org.robolectric.nativeruntime.DefaultNativeRuntimeLoader
import org.robolectric.util.TempDirectory
import org.w3c.dom.Element
import java.nio.file.Files
import javax.xml.parsers.DocumentBuilderFactory
import javax.xml.transform.TransformerFactory
import javax.xml.transform.dom.DOMSource
import javax.xml.transform.stream.StreamResult

/** Adds the weights bae uses that Robolectric's static Roboto family lacks. */
class ShotNativeRuntimeLoader : DefaultNativeRuntimeLoader() {
    override fun maybeCopyExtraResources(dir: TempDirectory) {
        val fonts = dir.basePath.resolve("fonts")
        // This Roboto is the variable font from AOSP's android-15.0.0_r1 tag in external/roboto-fonts.
        val font =
            checkNotNull(javaClass.getResourceAsStream("/shot-fonts/Roboto-Regular.ttf")) {
                "The Android shot fonts lack variable Roboto"
            }
        font.use { Files.copy(it, fonts.resolve("Roboto-Variable.ttf")) }

        // Robolectric calls this hook after extracting its fonts and before Android loads the system font map.
        val config = fonts.resolve("fonts.xml").toFile()
        val document = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(config)
        val families = document.getElementsByTagName("family")
        val sansSerif =
            (0 until families.length)
                .map { families.item(it) as Element }
                .single { it.getAttribute("name") == "sans-serif" }

        for (weight in listOf("600", "800")) {
            val face =
                document.createElement("font").apply {
                    setAttribute("weight", weight)
                    setAttribute("style", "normal")
                    textContent = "Roboto-Variable.ttf"
                }
            val axis =
                document.createElement("axis").apply {
                    setAttribute("tag", "wght")
                    setAttribute("stylevalue", weight)
                }
            face.appendChild(axis)
            sansSerif.appendChild(face)
        }
        TransformerFactory.newInstance().newTransformer().transform(DOMSource(document), StreamResult(config))
    }
}
