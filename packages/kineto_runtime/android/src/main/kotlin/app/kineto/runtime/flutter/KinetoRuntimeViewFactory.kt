package app.kineto.runtime.flutter

import android.content.Context
import android.view.View
import app.kineto.runtime.KinetoPlayer
import app.kineto.runtime.KinetoPlayerView
import io.flutter.plugin.common.StandardMessageCodec
import io.flutter.plugin.platform.PlatformView
import io.flutter.plugin.platform.PlatformViewFactory

internal class KinetoRuntimeViewFactory : PlatformViewFactory(StandardMessageCodec.INSTANCE) {
    override fun create(context: Context, viewId: Int, args: Any?): PlatformView {
        val params = args as? Map<*, *> ?: emptyMap<Any, Any>()
        return KinetoRuntimePlatformView(
            context = context,
            handle = (params["handle"] as? Number)?.toLong() ?: 0L,
            autoplay = params["autoplay"] as? Boolean ?: true,
        )
    }
}

/**
 * The runtime's own [KinetoPlayerView], so Flutter and native Android hosts play
 * through one code path.
 *
 * [handle] is a reference Dart retained for this view (`native_view_io.dart`). The
 * view owns it from here: Dart can dispose its player while this keeps drawing.
 */
private class KinetoRuntimePlatformView(
    context: Context,
    handle: Long,
    autoplay: Boolean,
) : PlatformView {
    private val player = if (handle == 0L) null else KinetoPlayer.adopt(handle)
    private val view = KinetoPlayerView(context).also {
        it.autoplay = autoplay
        it.player = player
    }

    override fun getView(): View = view

    override fun dispose() {
        view.player = null
        player?.close()
    }
}
