import 'package:flutter/widgets.dart';

import 'kineto_player_stub.dart';

const String kinetoRuntimeViewType = 'kineto/runtime_player';

final class KinetoPlayerView extends StatelessWidget {
  const KinetoPlayerView({
    super.key,
    required this.player,
    this.autoplay = true,
    this.onPointerEvent,
  });

  final KinetoPlayer player;
  final bool autoplay;

  /// Not delivered on this platform yet.
  final void Function(KinetoPointerEvent event, int fired)? onPointerEvent;

  @override
  Widget build(BuildContext context) {
    final _ = (player, autoplay);
    return const Center(
      child: Text(
        'Kineto runtime playback is not implemented for this platform yet.',
        textAlign: TextAlign.center,
      ),
    );
  }
}
