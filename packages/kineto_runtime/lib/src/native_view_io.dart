import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'kineto_player_native.dart';

const String kinetoRuntimeViewType = 'kineto/runtime_player';

/// Plays a [KinetoPlayer] on a native surface and delivers touch to it.
///
/// Touch is read here, on the Flutter side, so Android and iOS share one path
/// and the platform view never has to take part in gesture arbitration.
final class KinetoPlayerView extends StatefulWidget {
  const KinetoPlayerView({
    super.key,
    required this.player,
    this.autoplay = true,
    this.onPointerEvent,
  });

  final KinetoPlayer player;
  final bool autoplay;

  /// With the number of listeners a touch fired. Pump audio from here: a firing
  /// outside a frame reaches a host only if something asks.
  final void Function(KinetoPointerEvent event, int fired)? onPointerEvent;

  @override
  State<KinetoPlayerView> createState() => _KinetoPlayerViewState();
}

final class _KinetoPlayerViewState extends State<KinetoPlayerView> {
  /// The reference this view's native side owns, taken once per native view.
  ///
  /// There is no safe moment to give it up from Dart: a platform view is torn
  /// down some time after its widget is gone, and it draws until then. So it
  /// travels with the creation params and the native view gives it up. If the
  /// widget dies before the view is ever created, the reference leaks, which
  /// costs one player. Freeing it here could race the creation in flight.
  late Map<String, Object?> _creationParams = _paramsFor(widget);

  static bool get _hasNativeView => Platform.isAndroid || Platform.isIOS;

  int? _activePointer;

  Map<String, Object?> _paramsFor(KinetoPlayerView view) => <String, Object?>{
        'handle': _hasNativeView ? view.player.retainForView() : 0,
        'autoplay': view.autoplay,
      };

  @override
  void didUpdateWidget(KinetoPlayerView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.player, widget.player) ||
        oldWidget.autoplay != widget.autoplay) {
      // A new key below makes a new native view, which needs its own reference.
      _creationParams = _paramsFor(widget);
      _activePointer = null;
    }
  }

  void _deliver(KinetoPointerEvent kind, PointerEvent event) {
    final player = widget.player;
    if (player.isDisposed) return;
    final size = context.size;
    if (size == null || size.isEmpty) return;
    final point = player.documentPoint(event.localPosition, size);
    final fired = player.pointerEvent(kind, point.dx, point.dy);
    widget.onPointerEvent?.call(kind, fired);
  }

  void _down(PointerDownEvent event) {
    if (_activePointer != null) return;
    _activePointer = event.pointer;
    _deliver(KinetoPointerEvent.down, event);
  }

  void _move(PointerMoveEvent event) {
    if (event.pointer != _activePointer) return;
    final player = widget.player;
    final size = context.size;
    if (player.isDisposed || size == null || size.isEmpty) return;
    final point = player.documentPoint(event.localPosition, size);
    player.pointerMoved(point.dx, point.dy);
  }

  void _release(PointerEvent event) {
    if (event.pointer != _activePointer) return;
    _activePointer = null;
    // A cancelled touch is an up to the machine: leaving a pointer-down listener
    // latched would strand the state.
    _deliver(KinetoPointerEvent.up, event);
  }

  @override
  Widget build(BuildContext context) {
    final Widget? view;
    final key = ValueKey<Object>(_creationParams);
    if (Platform.isAndroid) {
      view = AndroidView(
        key: key,
        viewType: kinetoRuntimeViewType,
        creationParams: _creationParams,
        creationParamsCodec: const StandardMessageCodec(),
        gestureRecognizers: const <Factory<OneSequenceGestureRecognizer>>{},
      );
    } else if (Platform.isIOS) {
      view = UiKitView(
        key: key,
        viewType: kinetoRuntimeViewType,
        creationParams: _creationParams,
        creationParamsCodec: const StandardMessageCodec(),
        gestureRecognizers: const <Factory<OneSequenceGestureRecognizer>>{},
      );
    } else {
      view = null;
    }
    if (view == null) {
      return const Center(
        child: Text(
          'Kineto playback needs Android, iOS or web. '
          'This platform can only evaluate.',
          textAlign: TextAlign.center,
        ),
      );
    }
    return Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: _down,
      onPointerMove: _move,
      onPointerUp: _release,
      onPointerCancel: _release,
      child: view,
    );
  }
}
