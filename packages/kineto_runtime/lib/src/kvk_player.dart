import 'dart:typed_data';

import 'kineto_player.dart';

/// Backwards-compatible entrypoint for older callers.
///
/// M7 makes the Rust runtime canonical. New code should use [KinetoPlayer].
@Deprecated('Use KinetoPlayer.loadBytes instead.')
class KvkPlayer {
  KvkPlayer._();

  static KinetoPlayer load(Uint8List bytes) => KinetoPlayer.loadBytes(bytes);
}
