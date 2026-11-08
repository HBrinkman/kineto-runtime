export 'kineto_player_stub.dart'
    if (dart.library.io) 'kineto_player_native.dart'
    if (dart.library.js_interop) 'kineto_player_web.dart';
