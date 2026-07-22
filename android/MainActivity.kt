package dev.dioxus.main

import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.roundToInt

typealias BuildConfig = fr.variete_saveurs.devis_factures.BuildConfig

class MainActivity : WryActivity() {
    private lateinit var webView: WebView

    override fun onCreate(savedInstanceState: Bundle?) {
        window.setBackgroundDrawable(ColorDrawable(CHROME_COLOR))
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        super.onCreate(savedInstanceState)

        WindowCompat.getInsetsController(window, window.decorView).apply {
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = true
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isNavigationBarContrastEnforced = false
        }
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = navigateBack()
        })
    }

    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        this.webView = webView
        webView.setBackgroundColor(CHROME_COLOR)
        webView.settings.textZoom = (resources.configuration.fontScale * 100).roundToInt()

        ViewCompat.setOnApplyWindowInsetsListener(webView) { _, insets ->
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
            insets
        }
        ViewCompat.requestApplyInsets(webView)
    }

    private fun navigateBack() {
        if (::webView.isInitialized && webView.canGoBack()) {
            webView.goBack()
        } else {
            finish()
        }
    }

    private companion object {
        val CHROME_COLOR: Int = Color.rgb(15, 63, 58)
    }
}
