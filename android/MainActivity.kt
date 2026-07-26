package dev.dioxus.main

import android.content.ContentProvider
import android.content.ContentValues
import android.content.res.Configuration
import android.database.Cursor
import android.database.MatrixCursor
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File
import kotlin.math.roundToInt

typealias BuildConfig = fr.variete_saveurs.devis_factures.BuildConfig

class MainActivity : WryActivity() {
    private lateinit var webView: WebView
    private var latestInsets: WindowInsetsCompat? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        window.setBackgroundDrawable(ColorDrawable(chromeColor()))
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        super.onCreate(savedInstanceState)

        applySystemBarAppearance()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isNavigationBarContrastEnforced = false
        }
        // The decor view reliably receives system bar insets on every device;
        // a listener set on the WebView itself may never fire, leaving the
        // app bar under the status bar. Cache the latest insets here and push
        // them to the WebView whenever both sides are ready.
        ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { _, insets ->
            latestInsets = insets
            pushInsetsToWebView()
            insets
        }
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = navigateBack()
        })
    }

    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        this.webView = webView
        webView.setBackgroundColor(chromeColor())
        webView.settings.textZoom = (resources.configuration.fontScale * 100).roundToInt()
        // The splash video is muted, bundled and started from script; the
        // WebView default would still gate it behind a tap (DESIGN §8).
        webView.settings.mediaPlaybackRequiresUserGesture = false
        // evaluateJavascript is a silent no-op until a page is loaded, and the
        // first insets dispatch usually lands before that: replay the cached
        // insets a few times after attach. Later real dispatches (rotation,
        // IME, keyguard) keep them up to date.
        REPLAY_DELAYS_MS.forEach { delay ->
            webView.postDelayed({ pushInsetsToWebView() }, delay)
        }
    }

    private fun pushInsetsToWebView() {
        if (!::webView.isInitialized) return
        val insets = latestInsets ?: return
        val systemBars = insets.getInsets(
            WindowInsetsCompat.Type.systemBars() or
                WindowInsetsCompat.Type.displayCutout(),
        )
        val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
        val imeVisible = insets.isVisible(WindowInsetsCompat.Type.ime())
        webView.evaluateJavascript(
            """
            (() => {
                const scale = window.devicePixelRatio || 1;
                const root = document.documentElement;
                const style = root.style;
                style.setProperty('--system-inset-top', (${systemBars.top} / scale) + 'px');
                style.setProperty('--system-inset-right', (${systemBars.right} / scale) + 'px');
                style.setProperty('--system-inset-bottom', (${systemBars.bottom} / scale) + 'px');
                style.setProperty('--system-inset-left', (${systemBars.left} / scale) + 'px');
                style.setProperty('--ime-inset-bottom', (${ime.bottom} / scale) + 'px');
                root.classList.toggle('ime-visible', $imeVisible);
                requestAnimationFrame(() => requestAnimationFrame(() => {
                    if ($imeVisible && document.activeElement) {
                        document.activeElement.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'auto' });
                    }
                }));
            })();
            """.trimIndent(),
            null,
        )
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // uiMode is in configChanges, so a theme switch keeps this activity
        // alive instead of recreating it. The native chrome around the WebView
        // does not follow the new configuration by itself, so repaint it here.
        val chrome = chromeColor(newConfig)
        window.setBackgroundDrawable(ColorDrawable(chrome))
        applySystemBarAppearance(newConfig)
        if (::webView.isInitialized) {
            webView.setBackgroundColor(chrome)
            webView.settings.textZoom = (newConfig.fontScale * 100).roundToInt()
        }
    }

    private fun isNightMode(config: Configuration = resources.configuration): Boolean =
        config.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES

    private fun chromeColor(config: Configuration = resources.configuration): Int =
        if (isNightMode(config)) CHROME_COLOR_DARK else CHROME_COLOR_LIGHT

    private fun applySystemBarAppearance(config: Configuration = resources.configuration) {
        WindowCompat.getInsetsController(window, window.decorView).apply {
            // The status bar always sits on the red chrome.
            isAppearanceLightStatusBars = false
            // The navigation bar sits on the content background on most
            // screens; the form and the preview end on the red action bar,
            // where dark icons are wrong — but only in the light scheme.
            isAppearanceLightNavigationBars = !isNightMode(config)
        }
    }

    private fun navigateBack() {
        if (::webView.isInitialized && webView.canGoBack()) {
            webView.goBack()
        } else {
            finish()
        }
    }

    private companion object {
        val CHROME_COLOR_LIGHT: Int = Color.rgb(107, 18, 32)
        val CHROME_COLOR_DARK: Int = Color.rgb(74, 12, 22)
        val REPLAY_DELAYS_MS = longArrayOf(0L, 300L, 1000L, 3000L)
    }
}

/**
 * Read-only provider exposing the app-private `files/exports/` directory to
 * the share sheet (ACTION_SEND needs a content:// URI). Kept in this file
 * because the Dioxus build only compiles the activity referenced from
 * Dioxus.toml.
 */
class ExportFileProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String = when {
        uri.path.orEmpty().endsWith(".pdf") -> "application/pdf"
        uri.path.orEmpty().endsWith(".png") -> "image/png"
        uri.path.orEmpty().endsWith(".html") -> "text/html"
        else -> "application/octet-stream"
    }

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val file = fileFor(uri)
        val cursor = MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE))
        cursor.addRow(arrayOf(file.name, file.length()))
        return cursor
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        require(mode == "r" || mode == "rt") { "read-only provider" }
        return ParcelFileDescriptor.open(fileFor(uri), ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?): Int = 0
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?): Int = 0

    private fun fileFor(uri: Uri): File {
        val exportsDir = File(context!!.filesDir, "exports").canonicalFile
        val file = File(exportsDir, uri.path.orEmpty().removePrefix("/")).canonicalFile
        if (!file.path.startsWith(exportsDir.path + File.separator) || !file.isFile) {
            throw IllegalArgumentException("path outside exports: ${uri.path}")
        }
        return file
    }
}
