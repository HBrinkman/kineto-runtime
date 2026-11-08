import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'kineto_player_base.dart';
import 'native_view_web.dart';
import 'web_runtime_bridge.dart' as web_bridge;

export 'kineto_player_base.dart';

final class KinetoPlayer {
  KinetoPlayer._(this._bytes, {this.assetName, Object? wasmPlayer})
      : _wasmPlayer = wasmPlayer;

  Uint8List? _bytes;
  Object? _wasmPlayer;
  double _currentTime = 0;

  final String? assetName;

  static Future<KinetoPlayer> loadAsset(String assetKey) async {
    final data = await rootBundle.load(assetKey);
    final bytes =
        data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes);
    return KinetoPlayer._(
      bytes,
      assetName: assetKey,
      wasmPlayer: await web_bridge.createWasmPlayer(bytes),
    );
  }

  static KinetoPlayer loadBytes(Uint8List bytes) =>
      KinetoPlayer._(Uint8List.fromList(bytes));

  static Future<KinetoPlayer> loadBytesAsync(Uint8List bytes) async {
    final copiedBytes = Uint8List.fromList(bytes);
    return KinetoPlayer._(
      copiedBytes,
      wasmPlayer: await web_bridge.createWasmPlayer(copiedBytes),
    );
  }

  bool get isDisposed => _bytes == null;

  int get nativeHandleAddress {
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Web runtime uses wasm handles; JS glue is not packaged yet.',
    );
  }

  void advance(double deltaSeconds) {
    _checkOpen();
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer != null) {
      web_bridge.advanceWasmPlayer(wasmPlayer, deltaSeconds);
    }
    _currentTime += deltaSeconds < 0 ? 0 : deltaSeconds;
  }

  void setBool(String stateMachineId, String inputName, bool value) {
    _checkOpen();
    final wasmPlayer = _checkedWasmPlayer();
    web_bridge.setWasmBool(
      wasmPlayer,
      stateMachineId,
      inputName,
      value,
    );
  }

  void setNumber(String stateMachineId, String inputName, double value) {
    _checkOpen();
    final wasmPlayer = _checkedWasmPlayer();
    web_bridge.setWasmNumber(
      wasmPlayer,
      stateMachineId,
      inputName,
      value,
    );
  }

  int pointerEvent(KinetoPointerEvent event, double x, double y) {
    _checkOpen();
    return web_bridge.wasmPointerEvent(_checkedWasmPlayer(), event.name, x, y);
  }

  void setTrigger(String stateMachineId, String inputName) {
    _checkOpen();
    final wasmPlayer = _checkedWasmPlayer();
    web_bridge.setWasmTrigger(
      wasmPlayer,
      stateMachineId,
      inputName,
    );
  }

  String? getActiveState(String stateMachineId) {
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer == null) return null;
    return web_bridge.getWasmActiveState(wasmPlayer, stateMachineId);
  }

  String? getActiveTransition(String stateMachineId) {
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer == null) return null;
    return web_bridge.getWasmActiveTransition(wasmPlayer, stateMachineId);
  }

  double getTransitionProgress(String stateMachineId) {
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer == null) return 0;
    return web_bridge.getWasmTransitionProgress(wasmPlayer, stateMachineId);
  }

  RuntimeTapTarget? hitTestDocumentPoint(Offset position) {
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer == null) return null;
    final nodeId = web_bridge.hitTestWasmDocumentPoint(
      wasmPlayer,
      position.dx,
      position.dy,
    );
    return nodeId == null ? null : RuntimeTapTarget(nodeId: nodeId);
  }

  RuntimeTapTarget? hitTestViewportPoint(Offset position, Size viewportSize) {
    final _ = (position, viewportSize);
    return null;
  }

  void renderToCanvas(String canvasId) {
    _checkOpen();
    final wasmPlayer = _checkedWasmPlayer();
    web_bridge.renderWasmToCanvas(wasmPlayer, canvasId);
  }

  KinetoRuntimeDiagnostics diagnostics({
    String? assetName,
    String? lastError,
  }) =>
      KinetoRuntimeDiagnostics(
        platform: 'web',
        renderer: _wasmPlayer == null
            ? 'Web wasm assets packaged; bridge not ready'
            : 'Web wasm-bindgen runtime active; WebGPU renderer pending',
        isDisposed: isDisposed,
        assetName: assetName ?? this.assetName,
        nativeHandleAddress: null,
        lastError: lastError,
      );

  double get debugCurrentTime => _currentTime;

  int? get debugByteLength => _bytes?.length;

  Widget asWidget({Key? key, bool autoplay = true}) =>
      KinetoPlayerView(key: key, player: this, autoplay: autoplay);

  void dispose() {
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer != null) {
      web_bridge.disposeWasmPlayer(wasmPlayer);
    }
    _wasmPlayer = null;
    _bytes = null;
  }

  Never _unsupported() => throw const KinetoRuntimeException(
        KinetoStatus.unsupported,
        'Not available in the Flutter web runtime yet.',
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

  void _checkOpen() {
    if (isDisposed) {
      throw const KinetoRuntimeException(
        KinetoStatus.nullPointer,
        'KinetoPlayer has been disposed',
      );
    }
  }

  Object _checkedWasmPlayer() {
    final wasmPlayer = _wasmPlayer;
    if (wasmPlayer != null) return wasmPlayer;
    throw const KinetoRuntimeException(
      KinetoStatus.unsupported,
      'Web runtime bridge is not ready.',
    );
  }
}
