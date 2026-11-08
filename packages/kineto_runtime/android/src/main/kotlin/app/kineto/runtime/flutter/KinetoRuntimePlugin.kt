package app.kineto.runtime.flutter

import io.flutter.embedding.engine.plugins.FlutterPlugin

class KinetoRuntimePlugin : FlutterPlugin {
    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        binding.platformViewRegistry
            .registerViewFactory("kineto/runtime_player", KinetoRuntimeViewFactory())
    }

    // No process-wide resources are owned by the plugin registration.
    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) = Unit
}
