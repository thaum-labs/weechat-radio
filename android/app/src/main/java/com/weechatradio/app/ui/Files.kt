package com.weechatradio.app.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import java.io.ByteArrayOutputStream
import java.io.File
import kotlin.concurrent.thread

// Files in and out of the app: pick one to send, save one that came in.

/** A file picked to send: its name and bytes. */
class Picked(val name: String, val data: ByteArray)

private val PICTURE = Regex("""\.(jpe?g|png|webp|gif|heic|bmp)$""", RegexOption.IGNORE_CASE)

fun isPicture(name: String) = PICTURE.containsMatchIn(name)

/** Read a picked file off the main thread; [done] runs on the main thread (null if it could not be read). */
private fun readPicked(context: Context, uri: Uri, done: (Picked?) -> Unit) {
    val main = android.os.Handler(context.mainLooper)
    thread(name = "read-file", isDaemon = true) {
        val picked = runCatching {
            val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            } ?: uri.lastPathSegment?.substringAfterLast('/') ?: "file"
            val data = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@runCatching null
            Picked(name, data)
        }.getOrNull()
        main.post { done(picked) }
    }
}

/** "Pick a file" for a screen: [onPicked] gets the file once it is read. */
@Composable
fun rememberFilePicker(onPicked: (Picked) -> Unit): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) readPicked(context, uri) { p ->
            if (p == null) Toast.makeText(context, "Could not read that file", Toast.LENGTH_SHORT).show() else onPicked(p)
        }
    }
    return { launcher.launch("*/*") }
}

/**
 * A picture made small enough for the air: JPEG, made smaller and softer
 * until it fits [maxBytes]. Null if it is not a picture or cannot get that small.
 */
fun shrinkPicture(data: ByteArray, maxBytes: Int): ByteArray? {
    val full = BitmapFactory.decodeByteArray(data, 0, data.size) ?: return null
    var side = 640
    while (side >= 96) {
        val scale = side.toFloat() / maxOf(full.width, full.height)
        val bmp = if (scale < 1f) Bitmap.createScaledBitmap(full, (full.width * scale).toInt().coerceAtLeast(1), (full.height * scale).toInt().coerceAtLeast(1), true) else full
        for (q in intArrayOf(70, 50, 35, 25)) {
            val out = ByteArrayOutputStream()
            bmp.compress(Bitmap.CompressFormat.JPEG, q, out)
            if (out.size() <= maxBytes) return out.toByteArray()
        }
        side = side * 3 / 4
    }
    return null
}

/** "Save a copy" of a file the station received: asks where, then copies it there. */
@Composable
fun rememberFileSaver(): (path: String) -> Unit {
    val context = LocalContext.current
    var pending by remember { mutableStateOf<String?>(null) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val from = pending
        pending = null
        if (uri == null || from == null) return@rememberLauncherForActivityResult
        val ok = runCatching {
            context.contentResolver.openOutputStream(uri)?.use { out -> File(from).inputStream().use { it.copyTo(out) } } != null
        }.getOrDefault(false)
        Toast.makeText(context, if (ok) "Saved" else "Could not save it", Toast.LENGTH_SHORT).show()
    }
    return { path ->
        if (!File(path).exists()) {
            Toast.makeText(context, "The file is no longer on the phone", Toast.LENGTH_SHORT).show()
        } else {
            pending = path
            launcher.launch(File(path).name)
        }
    }
}
