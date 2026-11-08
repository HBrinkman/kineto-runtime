import 'dart:typed_data';

import 'package:flutter/widgets.dart';

import 'kineto_player_base.dart';
import 'native_view_stub.dart';

export 'kineto_player_base.dart';

final class KinetoPlayer {
  KinetoPlayer._unsupported();

  static Future<KinetoPlayer> loadAsset(String assetKey) async {
    final _ = assetKey;
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  static KinetoPlayer loadBytes(Uint8List bytes) {
    final _ = bytes;
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  static Future<KinetoPlayer> loadBytesAsync(Uint8List bytes) async =>
      loadBytes(bytes);

  bool get isDisposed => true;

  int get nativeHandleAddress {
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  void advance(double deltaSeconds) {
    final _ = deltaSeconds;
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  void setBool(String stateMachineId, String inputName, bool value) {
    final _ = (stateMachineId, inputName, value);
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  void setNumber(String stateMachineId, String inputName, double value) {
    final _ = (stateMachineId, inputName, value);
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  int pointerEvent(KinetoPointerEvent event, double x, double y) {
    final _ = (event, x, y);
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  void setTrigger(String stateMachineId, String inputName) {
    final _ = (stateMachineId, inputName);
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Kineto runtime playback is not implemented for this platform yet.',
    );
  }

  String? getActiveState(String stateMachineId) {
    final _ = stateMachineId;
    return null;
  }

  String? getActiveTransition(String stateMachineId) {
    final _ = stateMachineId;
    return null;
  }

  double getTransitionProgress(String stateMachineId) {
    final _ = stateMachineId;
    return 0;
  }

  RuntimeTapTarget? hitTestDocumentPoint(Offset position) {
    final _ = position;
    return null;
  }

  RuntimeTapTarget? hitTestViewportPoint(Offset position, Size viewportSize) {
    final _ = (position, viewportSize);
    return null;
  }

  KinetoRuntimeDiagnostics diagnostics({
    String? assetName,
    String? lastError,
  }) =>
      KinetoRuntimeDiagnostics(
        platform: 'unsupported',
        renderer: 'Renderer unavailable until platform backend is implemented',
        isDisposed: true,
        assetName: assetName,
        lastError: lastError,
      );

  Widget asWidget({Key? key, bool autoplay = true}) =>
      KinetoPlayerView(key: key, player: this, autoplay: autoplay);

  void dispose() {}

  Never _unsupported() => throw const KinetoRuntimeException(
        KinetoStatus.unsupported,
        'Kineto runtime playback is not implemented for this platform yet.',
      );

  int retainForView() => _unsupported();

  void pointerMoved(double x, double y) => _unsupported();

  void play() => _unsupported();

  void pause() => _unsupported();

  void selectAnimation(String animationId) => _unsupported();

  List<KinetoMachineInfo> stateMachines() => _unsupported();

  List<KinetoInputInfo> inputs(String machineId) => _unsupported();

  List<KinetoClipInfo> clips() => _unsupported();

  List<KinetoAudioAssetInfo> audioAssets() => _unsupported();

  List<KinetoActiveAudioClip> activeAudioClips() => _unsupported();

  List<KinetoAudioEvent> audioEvents() => _unsupported();

  List<String> takeFiredAudioEvents() => _unsupported();

  Offset documentPoint(Offset position, Size viewSize) => _unsupported();

  double? overrideScalar(String nodeId, String key) => _unsupported();

  List<int> backgroundColor() => _unsupported();
}
