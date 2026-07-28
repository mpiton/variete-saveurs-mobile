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
import android.webkit.JavascriptInterface
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

    /**
     * Back is intercepted only while the app has something of its own to do
     * with it: a bottom sheet to close, or a route to come back to. At the root
     * with nothing open it stands down, and Android runs its own predictive
     * back — the back-to-home preview, and the long-press preview Android 16
     * gives three-button navigation.
     *
     * A callback left enabled at default priority suppresses both, whatever
     * `android:enableOnBackInvokedCallback` says, and this one was enabled
     * unconditionally: `DESIGN.md §5` has promised « geste prédictif partout »
     * since it was written, and the app never delivered it.
     *
     * Starts enabled on purpose. Until the web side reports for the first time,
     * Back behaves exactly as it did before this existed — the safe direction
     * to be wrong in, since the cost is a missing animation rather than a sheet
     * that will not close.
     */
    private val backCallback = object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() = navigateBack()
    }

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
        onBackPressedDispatcher.addCallback(this, backCallback)
    }

    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        this.webView = webView
        webView.setBackgroundColor(chromeColor())
        webView.settings.textZoom = (resources.configuration.fontScale * 100).roundToInt()
        // The splash video is muted, bundled and started from script; the
        // WebView default would still gate it behind a tap (DESIGN §8).
        webView.settings.mediaPlaybackRequiresUserGesture = false
        // One method, and all it can do is choose who handles Back. The page is
        // app-local: the only content that comes from her clients is rendered
        // inside `srcdoc` iframes sandboxed without `allow-scripts`, so nothing
        // a client name could carry ever runs in this document.
        webView.addJavascriptInterface(BackBridge(), BACK_BRIDGE_NAME)
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
        if (::webView.isInitialized) {
            webView.setBackgroundColor(chrome)
            webView.settings.textZoom = (newConfig.fontScale * 100).roundToInt()
        }
    }

    private fun isNightMode(config: Configuration = resources.configuration): Boolean =
        config.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES

    private fun chromeColor(config: Configuration = resources.configuration): Int =
        if (isNightMode(config)) CHROME_COLOR_DARK else CHROME_COLOR_LIGHT

    private fun applySystemBarAppearance() {
        WindowCompat.getInsetsController(window, window.decorView).apply {
            // Both bands are red chrome, in either scheme: the top app bar
            // under the status bar, and under the navigation bar either the
            // action bar or, on the screens without one, the chrome edge the
            // scroll container paints there. So both keep light icons —
            // 12,12:1 on #6B1220, where dark ones would be 1,7:1.
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }
    }

    /**
     * Back closes an open bottom sheet before it moves in history.
     *
     * The sheets are `<dialog>` elements with an `oncancel` handler, but this
     * callback is registered as always-enabled: it consumed the back event and
     * called `goBack()` straight away, so the WebView never saw the key and the
     * handler never ran. Back left the screen instead of closing the sheet —
     * losing the line being edited, the catalogue picks, the confirmation about
     * to be answered. Confirmed on device before this fix.
     *
     * `evaluateJavascript` answers on the UI thread a few milliseconds later,
     * so the decision belongs in its callback rather than inline.
     */
    /**
     * How the web side hands over the one thing Kotlin cannot see: whether a
     * sheet is up, and whether the router has anywhere to go back to.
     */
    private inner class BackBridge {
        @JavascriptInterface
        fun setIntercepts(intercepts: Boolean) {
            // Called on the WebView's JS thread; `isEnabled` belongs to the UI one.
            runOnUiThread { backCallback.isEnabled = intercepts }
        }
    }

    private fun navigateBack() {
        if (!::webView.isInitialized) {
            finish()
            return
        }
        webView.evaluateJavascript(DISMISS_OPEN_SHEET) { dismissed ->
            if (dismissed != "true") {
                if (webView.canGoBack()) webView.goBack() else finish()
            }
        }
    }

    private companion object {
        val CHROME_COLOR_LIGHT: Int = Color.rgb(107, 18, 32)
        val CHROME_COLOR_DARK: Int = Color.rgb(74, 12, 22)
        val REPLAY_DELAYS_MS = longArrayOf(0L, 300L, 1000L, 3000L)

        /** Global the web side calls; `ui/app.rs` names it too. */
        const val BACK_BRIDGE_NAME = "AndroidBack"

        /**
         * Cancels the topmost open sheet and reports whether there was one.
         *
         * `cancel` is the event the sheet already listens to — the same one
         * Escape fires in a desktop browser — so dismissal keeps going through
         * the component's own handler, which knows not to close a sheet whose
         * job is still running. A sheet that refuses still counts as handled:
         * back must not walk away from a running export either.
         *
         * Dispatched with `bubbles: true` although `cancel` normally does not
         * bubble: it has to reach the handler whether the framework binds the
         * listener on the element or delegates it at the document root.
         */
        const val DISMISS_OPEN_SHEET = """
            (function () {
                const open = document.querySelectorAll('dialog[open]');
                const sheet = open[open.length - 1];
                if (!sheet) { return false; }
                sheet.dispatchEvent(new Event('cancel', { bubbles: true, cancelable: true }));
                return true;
            })()
        """
    }
}

/**
 * Read-only provider over one app-private directory: an Intent that carries a
 * file needs a content:// URI, and neither the share sheet nor the package
 * installer may be handed anything outside the directory it is meant to read.
 * That containment check lives here once rather than in each subclass — two
 * copies of a path-traversal guard is one copy too many.
 *
 * Kept in this file because the Dioxus build only compiles the activity
 * referenced from Dioxus.toml.
 */
abstract class PrivateFileProvider : ContentProvider() {
    /** Directory this provider is allowed to serve, and nothing above it. */
    protected abstract fun rootDir(): File

    protected abstract fun mimeFor(path: String): String

    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String = mimeFor(uri.path.orEmpty())

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
        val root = rootDir().canonicalFile
        val file = File(root, uri.path.orEmpty().removePrefix("/")).canonicalFile
        if (!file.path.startsWith(root.path + File.separator) || !file.isFile) {
            throw IllegalArgumentException("path outside ${root.name}: ${uri.path}")
        }
        return file
    }
}

/** `files/exports/` for the share sheet (ACTION_SEND). */
class ExportFileProvider : PrivateFileProvider() {
    override fun rootDir(): File = File(context!!.filesDir, "exports")

    override fun mimeFor(path: String): String = when {
        path.endsWith(".pdf") -> "application/pdf"
        path.endsWith(".png") -> "image/png"
        path.endsWith(".html") -> "text/html"
        else -> "application/octet-stream"
    }
}

/**
 * `cache/updates/` for the downloaded APK (issue 35). The cache, not
 * `filesDir`: the Auto Backup rules cover what sits beside the database, and
 * a release APK would eat the 25 MB quota her accounting depends on.
 */
class UpdateFileProvider : PrivateFileProvider() {
    override fun rootDir(): File = File(context!!.cacheDir, "updates")

    override fun mimeFor(path: String): String = "application/vnd.android.package-archive"
}
