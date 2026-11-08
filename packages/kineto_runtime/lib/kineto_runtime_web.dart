import 'package:flutter_web_plugins/flutter_web_plugins.dart';
import 'package:web/web.dart' as web;

class KinetoRuntimeWebPlugin {
  static void registerWith(Registrar registrar) {
    final _ = registrar;
    _ensureBridgeScript();
  }
}

void _ensureBridgeScript() {
  if (web.document.querySelector('[data-kineto-runtime-bridge]') != null) {
    return;
  }

  final script = web.document.createElement('script') as web.HTMLScriptElement;
  script.type = 'module';
  script.src = 'assets/packages/kineto_runtime/web/kineto_runtime_bridge.js';
  script.setAttribute('data-kineto-runtime-bridge', 'true');
  web.document.head?.appendChild(script);
}
