package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidScanCode
import org.junit.Test

/** The component shots of `AndroidScanCode`, one for each row of its preview. */
class AndroidScanCodeShots : BoardShot() {
    @Test
    fun androidComponentScanCodeDefault() =
        component("android.component.scan-code.default") {
            AndroidScanCode(instruction = "Scan the QR code on their device, or share the code.", qr = INVITE_CODE, code = INVITE_CODE)
        }
}

/** A short invite code, which the boards draw: one with no S3 key. */
private const val INVITE_CODE = "0000 0000"
