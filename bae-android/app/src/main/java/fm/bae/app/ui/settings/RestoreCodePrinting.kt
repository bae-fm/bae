package fm.bae.app.ui.settings

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.Typeface
import android.os.Bundle
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.print.PageRange
import android.print.PrintAttributes
import android.print.PrintDocumentAdapter
import android.print.PrintDocumentInfo
import android.print.PrintManager
import android.print.pdf.PrintedPdfDocument
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import fm.bae.app.R
import fm.bae.app.ui.components.encodeQRCode
import java.io.FileOutputStream
import java.io.IOException
import java.text.DateFormat
import java.util.Date

private const val SIDE_MARGIN_MILS = 889
private const val TOP_MARGIN_MILS = 778

/** The words on a printed restore code, in the app's language. */
internal class RestoreCodePage(
    val title: String,
    val location: String,
    val printed: String,
    val code: String,
    val keep: String,
) {
    /** What the QR code holds: the code run together, which decodes as typed does. */
    val scanned: String get() = code.filterNot { it.isWhitespace() }

    companion object {
        fun of(
            context: Context,
            code: String,
            location: String,
            date: Date = Date(),
        ): RestoreCodePage =
            RestoreCodePage(
                title = context.getString(R.string.restore_code_print_title),
                location = location,
                printed =
                    context.getString(
                        R.string.restore_code_printed,
                        DateFormat.getDateInstance(DateFormat.LONG).format(date),
                    ),
                code = code,
                keep = context.getString(R.string.restore_code_print_keep),
            )
    }
}

/**
 * Prints the restore code through the system's print dialog: one page of
 * [RestoreCodePage], with no library name and no S3 key.
 */
internal fun printRestoreCode(
    context: Context,
    page: RestoreCodePage,
) {
    val manager = context.getSystemService(PrintManager::class.java)
    // The board's margins: 64 points at the sides, 56 above and below, in mils.
    val margins =
        PrintAttributes.Margins(SIDE_MARGIN_MILS, TOP_MARGIN_MILS, SIDE_MARGIN_MILS, TOP_MARGIN_MILS)
    val attributes = PrintAttributes.Builder().setMinMargins(margins).build()
    manager.print(page.title, RestoreCodePrintAdapter(context, page), attributes)
}

private class RestoreCodePrintAdapter(
    private val context: Context,
    private val page: RestoreCodePage,
) : PrintDocumentAdapter() {
    private var attributes: PrintAttributes? = null

    override fun onLayout(
        old: PrintAttributes?,
        new: PrintAttributes,
        cancel: CancellationSignal?,
        callback: LayoutResultCallback,
        extras: Bundle?,
    ) {
        if (cancel?.isCanceled == true) {
            callback.onLayoutCancelled()
            return
        }
        attributes = new
        val info =
            PrintDocumentInfo
                .Builder("${page.title}.pdf")
                .setContentType(PrintDocumentInfo.CONTENT_TYPE_DOCUMENT)
                .setPageCount(1)
                .build()
        callback.onLayoutFinished(info, new != old)
    }

    override fun onWrite(
        pages: Array<out PageRange>?,
        destination: ParcelFileDescriptor,
        cancel: CancellationSignal?,
        callback: WriteResultCallback,
    ) {
        val document = PrintedPdfDocument(context, checkNotNull(attributes))
        try {
            val sheet = document.startPage(0)
            draw(sheet.canvas, sheet.info.contentRect)
            document.finishPage(sheet)
            FileOutputStream(destination.fileDescriptor).use { document.writeTo(it) }
            callback.onWriteFinished(arrayOf(PageRange.ALL_PAGES))
        } catch (e: IOException) {
            callback.onWriteFailed(e.message)
        } finally {
            document.close()
        }
    }

    /** Lays the page out top to bottom inside [area], as the board draws it. */
    private fun draw(
        canvas: Canvas,
        area: Rect,
    ) {
        val column = Column(canvas, area)
        column.write(page.title, Ink(TITLE_SIZE, Typeface.create(Typeface.DEFAULT, Typeface.BOLD), SOFT_BLACK))
        column.gap()
        column.write("${page.location}\n${page.printed}", Ink(BODY_SIZE, Typeface.DEFAULT, GREY))
        column.gap()
        encodeQRCode(page.scanned)?.let { qr ->
            column.centered(qr, QR_SIDE)
            column.gap()
        }
        column.write(page.code, Ink(CODE_SIZE, Typeface.MONOSPACE, SOFT_BLACK))
        column.gap()
        column.write(page.keep, Ink(BODY_SIZE, Typeface.DEFAULT, GREY))
    }

    /** The size, face and colour text is written in. */
    private class Ink(
        val size: Float,
        val face: Typeface,
        val color: Int,
    )

    /** What has been written down [area] of [canvas] so far. */
    private class Column(
        private val canvas: Canvas,
        private val area: Rect,
    ) {
        private var top = area.top.toFloat()

        fun gap() {
            top += GAP
        }

        fun write(
            text: String,
            ink: Ink,
        ) {
            val paint =
                TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
                    color = ink.color
                    textSize = ink.size
                    typeface = ink.face
                }
            val layout =
                StaticLayout.Builder
                    .obtain(text, 0, text.length, paint, area.width())
                    .setAlignment(Layout.Alignment.ALIGN_NORMAL)
                    .setLineSpacing(LINE_EXTRA, 1f)
                    .build()
            canvas.save()
            canvas.translate(area.left.toFloat(), top)
            layout.draw(canvas)
            canvas.restore()
            top += layout.height
        }

        fun centered(
            bitmap: Bitmap,
            side: Int,
        ) {
            val left = (area.left + (area.width() - side) / 2f).toInt()
            canvas.drawBitmap(bitmap, null, Rect(left, top.toInt(), left + side, top.toInt() + side), null)
            top += side
        }
    }

    private companion object {
        const val TITLE_SIZE = 22f
        const val BODY_SIZE = 13f
        const val CODE_SIZE = 12f
        const val GAP = 20f
        const val LINE_EXTRA = 4f
        const val QR_SIDE = 200
        const val SOFT_BLACK = 0xFF111111.toInt()
        const val GREY = 0xFF444444.toInt()
    }
}
