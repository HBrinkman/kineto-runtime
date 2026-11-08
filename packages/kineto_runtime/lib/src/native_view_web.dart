import 'dart:async';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:flutter/widgets.dart';
import 'package:web/web.dart' as web;

import 'kineto_player_web.dart';

const String kinetoRuntimeViewType = 'kineto/runtime_player';

final class KinetoPlayerView extends StatefulWidget {
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
  State<KinetoPlayerView> createState() => _KinetoPlayerViewState();
}

final class _KinetoPlayerViewState extends State<KinetoPlayerView> {
  late String _canvasId;
  Timer? _timer;
  DateTime? _lastTick;
  String? _error;

  @override
  void initState() {
    super.initState();
    _canvasId = 'kineto-runtime-canvas-${identityHashCode(widget.player)}';
  }

  @override
  void didUpdateWidget(KinetoPlayerView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.player != widget.player) {
      _canvasId = 'kineto-runtime-canvas-${identityHashCode(widget.player)}';
      _lastTick = null;
      _error = null;
      _stopLoop();
    }
    if (oldWidget.autoplay != widget.autoplay) {
      _updateLoop();
    }
  }

  @override
  void dispose() {
    _stopLoop();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final diagnostics = widget.player.diagnostics();
    return DecoratedBox(
      decoration: const BoxDecoration(color: Color(0xFF101419)),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _CanvasHost(
                  player: widget.player,
                  canvasId: _canvasId,
                  onReady: _updateLoop,
                  onError: _setError,
                ),
                const SizedBox(height: 12),
                Text(
                  _error ?? diagnostics.renderer,
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    color: _error == null
                        ? const Color(0xFFE8EDF2)
                        : const Color(0xFFFFB4AB),
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  'Bytes: ${widget.player.debugByteLength ?? 0} | '
                  'Time: ${widget.player.debugCurrentTime.toStringAsFixed(3)}s',
                  textAlign: TextAlign.center,
                  style: const TextStyle(color: Color(0xFF9BA8B5)),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _updateLoop() {
    if (!mounted) return;
    if (!widget.autoplay) {
      _stopLoop();
      return;
    }
    if (_timer != null) return;
    _lastTick = DateTime.now();
    _timer = Timer.periodic(const Duration(milliseconds: 16), (_) {
      final now = DateTime.now();
      final previous = _lastTick ?? now;
      _lastTick = now;
      final delta = now.difference(previous).inMicroseconds / 1000000.0;
      try {
        widget.player.advance(delta.clamp(0.0, 0.05).toDouble());
        widget.player.renderToCanvas(_canvasId);
        _writeSmokePlaybackState(widget.player.debugCurrentTime);
        if (mounted) setState(() => _error = null);
      } on Object catch (error) {
        _stopLoop();
        if (mounted) setState(() => _error = error.toString());
      }
    });
  }

  void _stopLoop() {
    _timer?.cancel();
    _timer = null;
    _lastTick = null;
  }

  void _setError(Object error) {
    if (!mounted) return;
    setState(() => _error = error.toString());
  }
}

void _writeSmokePlaybackState(double currentTime) {
  final previousTicks = globalContext.getProperty<JSNumber?>(
    '__kinetoRuntimeAutoplayTicks'.toJS,
  );
  final ticks = (previousTicks?.toDartDouble ?? 0) + 1;
  globalContext.setProperty('__kinetoRuntimeAutoplayTicks'.toJS, ticks.toJS);
  globalContext.setProperty(
      '__kinetoRuntimeCurrentTime'.toJS, currentTime.toJS);
}

final class _CanvasHost extends StatelessWidget {
  const _CanvasHost({
    required this.player,
    required this.canvasId,
    required this.onReady,
    required this.onError,
  });

  final KinetoPlayer player;
  final String canvasId;
  final VoidCallback onReady;
  final ValueChanged<Object> onError;

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: 16 / 9,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: const Color(0xFF18202A),
          border: Border.all(color: const Color(0xFF2D3A46)),
        ),
        child: HtmlElementView.fromTagName(
          tagName: 'canvas',
          onElementCreated: (element) {
            final canvas = element as web.HTMLCanvasElement;
            canvas.id = canvasId;
            canvas.width = 640;
            canvas.height = 360;
            canvas.style.width = '100%';
            canvas.style.height = '100%';
            canvas.style.display = 'block';
            _renderWhenAttached(player, canvasId, onReady, onError);
          },
        ),
      ),
    );
  }
}

Future<void> _renderWhenAttached(
  KinetoPlayer player,
  String canvasId,
  VoidCallback onReady,
  ValueChanged<Object> onError,
) async {
  Object? lastError;
  for (var attempt = 0; attempt < 20; attempt += 1) {
    await Future<void>.delayed(const Duration(milliseconds: 50));
    try {
      player.renderToCanvas(canvasId);
      onReady();
      return;
    } on Object catch (error) {
      lastError = error;
      // The element is created before attachment. Retry until the canvas is in
      // the document and wasm can resolve it by id.
    }
  }
  if (lastError != null) onError(lastError);
}
