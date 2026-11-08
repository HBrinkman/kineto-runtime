import 'dart:ffi' as ffi;
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'ffi/bindings.dart';
import 'ffi/library_loader.dart';
import 'kineto_player_base.dart';
import 'native_view_io.dart';

export 'kineto_player_base.dart';

final class KinetoPlayer implements ffi.Finalizable {
  KinetoPlayer._(this._bindings, this._handle) {
    _bindings.finalizer.attach(this, _handle.cast(), detach: this);
  }

  final KinetoEngineBindings _bindings;
  ffi.Pointer<ffi.Void> _handle;

  static KinetoEngineBindings? _sharedBindings;

  static KinetoEngineBindings get _bindingsForProcess =>
      _sharedBindings ??= KinetoEngineBindings(loadKinetoEngineLibrary());

  static Future<KinetoPlayer> load(File file) async =>
      loadBytes(await file.readAsBytes());

  static Future<KinetoPlayer> loadAsset(String assetKey) async {
    final data = await rootBundle.load(assetKey);
    return loadBytes(
      data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
    );
  }

  static KinetoPlayer loadBytes(
    Uint8List bytes, {
    KinetoEngineBindings? bindings,
  }) {
    final activeBindings = bindings ?? _bindingsForProcess;
    final bytesPtr = calloc<ffi.Uint8>(bytes.length);
    final statusPtr = calloc<ffi.Int32>();
    try {
      bytesPtr.asTypedList(bytes.length).setAll(0, bytes);
      final handle = activeBindings.load(bytesPtr, bytes.length, statusPtr);
      final status = KinetoStatus.fromCode(statusPtr.value);
      if (handle == ffi.nullptr || status != KinetoStatus.ok) {
        throw KinetoRuntimeException(status, 'Unable to load .kvk data');
      }
      return KinetoPlayer._(activeBindings, handle);
    } finally {
      calloc.free(bytesPtr);
      calloc.free(statusPtr);
    }
  }

  static Future<KinetoPlayer> loadBytesAsync(Uint8List bytes) async =>
      loadBytes(bytes);

  bool get isDisposed => _handle == ffi.nullptr;

  int get nativeHandleAddress => _checkedHandle().address;

  void advance(double deltaSeconds) {
    final status = KinetoStatus.fromCode(
      _bindings.advance(_checkedHandle(), deltaSeconds),
    );
    _check(status);
  }

  void setBool(String stateMachineId, String inputName, bool value) {
    _withNativeStrings(stateMachineId, inputName, (machine, input) {
      _check(
        KinetoStatus.fromCode(
          _bindings.setBool(_checkedHandle(), machine, input, value),
        ),
      );
    });
  }

  void setNumber(String stateMachineId, String inputName, double value) {
    _withNativeStrings(stateMachineId, inputName, (machine, input) {
      _check(
        KinetoStatus.fromCode(
          _bindings.setNumber(_checkedHandle(), machine, input, value),
        ),
      );
    });
  }

  /// Delivers a pointer event at a document point (issue #74). Whatever
  /// listens there drives its input; the caller names a place, not an input.
  ///
  /// Returns how many listeners fired.
  int pointerEvent(KinetoPointerEvent event, double x, double y) {
    final fired = calloc<ffi.Uint32>();
    try {
      _check(
        KinetoStatus.fromCode(
          _bindings.pointerEvent(_checkedHandle(), event.index, x, y, fired),
        ),
      );
      return fired.value;
    } finally {
      calloc.free(fired);
    }
  }

  /// A drag rather than a tap: fires no listeners, drives what follows the
  /// pointer.
  void pointerMoved(double x, double y) {
    _check(
      KinetoStatus.fromCode(_bindings.pointerMoved(_checkedHandle(), x, y)),
    );
  }

  /// A point on a view of [viewSize], in document space. The engine undoes the
  /// fit and camera it renders with, so a host never repeats that arithmetic.
  ///
  /// The fit only depends on proportions, so logical pixels are as good as
  /// physical ones provided [position] and [viewSize] use the same unit.
  Offset documentPoint(Offset position, Size viewSize) {
    final outX = calloc<ffi.Double>();
    final outY = calloc<ffi.Double>();
    try {
      _check(
        KinetoStatus.fromCode(
          _bindings.documentPoint(
            _checkedHandle(),
            viewSize.width,
            viewSize.height,
            position.dx,
            position.dy,
            outX,
            outY,
          ),
        ),
      );
      return Offset(outX.value, outY.value);
    } finally {
      calloc.free(outX);
      calloc.free(outY);
    }
  }

  /// One evaluated scalar of the current frame, or null when it carries none.
  /// For cross-runtime parity checks; hosts consume a frame by rendering it.
  double? overrideScalar(String nodeId, String key) {
    final out = calloc<ffi.Double>();
    try {
      double? value;
      _withNativeStrings(nodeId, key, (node, name) {
        final status = _bindings.overrideScalar(
          _checkedHandle(),
          node,
          name,
          out,
        );
        if (status == KinetoStatus.ok.code) value = out.value;
      });
      return value;
    } finally {
      calloc.free(out);
    }
  }

  /// The document's background as straight `[r, g, b, a]`, 0-255. All zero when
  /// it has none: the surface stays transparent.
  List<int> backgroundColor() {
    final out = calloc<ffi.Uint8>(4);
    try {
      _check(
        KinetoStatus.fromCode(_bindings.backgroundColor(_checkedHandle(), out)),
      );
      return List.of(out.asTypedList(4));
    } finally {
      calloc.free(out);
    }
  }

  /// Starts the active clip. The timeline, and its audio, only move while
  /// playing.
  void play() =>
      _check(KinetoStatus.fromCode(_bindings.play(_checkedHandle())));

  /// Pauses playback, leaving the playhead where it is.
  void pause() =>
      _check(KinetoStatus.fromCode(_bindings.pause(_checkedHandle())));

  /// Makes [animationId] the active clip, rewound to its start.
  void selectAnimation(String animationId) {
    final id = animationId.toNativeUtf8().cast<ffi.Char>();
    try {
      _check(
        KinetoStatus.fromCode(_bindings.selectAnimation(_checkedHandle(), id)),
      );
    } finally {
      calloc.free(id);
    }
  }

  List<KinetoMachineInfo> stateMachines() => [
        for (final row in _records(_bindings.stateMachines, 2))
          KinetoMachineInfo(id: row[0], name: row[1]),
      ];

  /// One machine's inputs. Empty when nothing carries [machineId].
  List<KinetoInputInfo> inputs(String machineId) {
    final machine = machineId.toNativeUtf8().cast<ffi.Char>();
    try {
      final joined = _string(_bindings.machineInputs(_checkedHandle(), machine));
      return [
        for (final row in parseKinetoRecords(joined, 5))
          // A kind this build does not know is skipped rather than guessed at:
          // driving an input by the wrong rule is worse than not offering it.
          if (_inputKind(row[2]) case final kind?)
            KinetoInputInfo(
              id: row[0],
              name: row[1],
              kind: kind,
              defaultBool: row[3] == '1',
              defaultNumber: double.tryParse(row[4]) ?? 0,
            ),
      ];
    } finally {
      calloc.free(machine);
    }
  }

  List<KinetoClipInfo> clips() => [
        for (final row in _records(_bindings.animations, 3))
          KinetoClipInfo(
            id: row[0],
            name: row[1],
            duration: double.tryParse(row[2]) ?? 0,
          ),
      ];

  /// Every audio file this document needs.
  List<KinetoAudioAssetInfo> audioAssets() => [
        for (final row in _records(_bindings.audioAssets, 3))
          KinetoAudioAssetInfo(
            id: row[0],
            fileName: row[1],
            duration: double.tryParse(row[2]) ?? 0,
          ),
      ];

  /// The timeline audio that should be sounding. Poll it after [advance].
  List<KinetoActiveAudioClip> activeAudioClips() => [
        for (final row in _records(_bindings.activeAudioClips, 5))
          if ((double.tryParse(row[3]), double.tryParse(row[4]))
              case (final position?, final volume?))
            KinetoActiveAudioClip(
              trackId: row[0],
              assetId: row[1],
              fileName: row[2],
              sourcePosition: position,
              volume: volume,
            ),
      ];

  /// Every sound the document names, so a firing can be mapped to the app's
  /// own audio.
  List<KinetoAudioEvent> audioEvents() => [
        for (final row in _records(_bindings.audioEvents, 3))
          KinetoAudioEvent(
            id: row[0],
            name: row[1],
            sourceName: row[2].isEmpty ? null : row[2],
          ),
      ];

  /// The sounds fired since the last call, in order. Draining and never
  /// de-duplicated: the same sound twice is two firings.
  ///
  /// The runtime plays nothing. A host plays what it ships for each id, after
  /// [advance] and after [pointerEvent].
  List<String> takeFiredAudioEvents() => _string(
        _bindings.takeFiredAudioEvents(_checkedHandle(), ffi.nullptr),
      ).split('\n').where((id) => id.isNotEmpty).toList();

  /// Adds an owner for a platform view, which outlives anything Dart can
  /// observe: the view keeps drawing for a while after the widget is gone. The
  /// native view gives the reference up when it is torn down.
  int retainForView() {
    final handle = _checkedHandle();
    _check(KinetoStatus.fromCode(_bindings.retain(handle)));
    return handle.address;
  }

  void setTrigger(String stateMachineId, String inputName) {
    _withNativeStrings(stateMachineId, inputName, (machine, input) {
      _check(
        KinetoStatus.fromCode(
          _bindings.setTrigger(_checkedHandle(), machine, input),
        ),
      );
    });
  }

  String? getActiveState(String stateMachineId) =>
      _copyString(stateMachineId, _bindings.getActiveState);

  String? getActiveTransition(String stateMachineId) =>
      _copyString(stateMachineId, _bindings.getActiveTransition);

  double getTransitionProgress(String stateMachineId) {
    final machine = stateMachineId.toNativeUtf8().cast<ffi.Char>();
    final statusPtr = calloc<ffi.Int32>();
    try {
      final value =
          _bindings.getTransitionProgress(_checkedHandle(), machine, statusPtr);
      final status = KinetoStatus.fromCode(statusPtr.value);
      _check(status);
      return value;
    } finally {
      calloc.free(machine);
      calloc.free(statusPtr);
    }
  }

  RuntimeTapTarget? hitTestDocumentPoint(Offset position) {
    final id = _copyHitTestString(position.dx, position.dy);
    return id == null ? null : RuntimeTapTarget(nodeId: id);
  }

  RuntimeTapTarget? hitTestViewportPoint(Offset position, Size viewportSize) =>
      hitTestDocumentPoint(documentPoint(position, viewportSize));

  KinetoRuntimeDiagnostics diagnostics({
    String? assetName,
    String? lastError,
  }) {
    final disposed = isDisposed;
    return KinetoRuntimeDiagnostics(
      platform: Platform.operatingSystem,
      renderer: Platform.isIOS
          ? 'iOS UiKitView + Metal surface'
          : Platform.isAndroid
              ? 'Android platform view + wgpu surface'
              : 'No renderer on this platform; the player evaluates only',
      isDisposed: disposed,
      assetName: assetName,
      nativeHandleAddress: disposed ? null : _checkedHandle().address,
      lastError: lastError ?? (disposed ? null : _runtimeError()),
    );
  }

  Widget asWidget({Key? key, bool autoplay = true}) =>
      KinetoPlayerView(key: key, player: this, autoplay: autoplay);

  void dispose() {
    final handle = _handle;
    if (handle == ffi.nullptr) return;
    _bindings.finalizer.detach(this);
    _bindings.destroy(handle);
    _handle = ffi.nullptr;
  }

  ffi.Pointer<ffi.Void> _checkedHandle() {
    if (_handle == ffi.nullptr) {
      throw const KinetoRuntimeException(
        KinetoStatus.nullPointer,
        'KinetoPlayer has been disposed',
      );
    }
    return _handle;
  }

  void _check(KinetoStatus status) {
    if (status == KinetoStatus.ok) return;
    throw KinetoRuntimeException(status, _lastErrorMessage());
  }

  /// What the engine last reported, from the player or from surface creation.
  /// Null while there is nothing to report.
  String? _runtimeError() {
    for (final message in [
      _lastErrorMessage(),
      _string(_bindings.surfaceLastError()),
    ]) {
      if (message.isNotEmpty) return message;
    }
    return null;
  }

  KinetoInputKind? _inputKind(String raw) {
    final index = int.tryParse(raw);
    if (index == null || index < 0) return null;
    if (index >= KinetoInputKind.values.length) return null;
    return KinetoInputKind.values[index];
  }

  List<List<String>> _records(KinetoRecordsDart read, int fields) =>
      parseKinetoRecords(_string(read(_checkedHandle())), fields);

  String _string(ffi.Pointer<ffi.Char> pointer) =>
      pointer == ffi.nullptr ? '' : pointer.cast<Utf8>().toDartString();

  String _lastErrorMessage() {
    final messagePtr = _bindings.lastError(_checkedHandle());
    if (messagePtr == ffi.nullptr) return '';
    return messagePtr.cast<Utf8>().toDartString();
  }

  String? _copyString(
    String stateMachineId,
    KinetoCopyStringDart copy,
  ) {
    final machine = stateMachineId.toNativeUtf8().cast<ffi.Char>();
    try {
      final required = copy(_checkedHandle(), machine, ffi.nullptr, 0);
      if (required <= 1) return null;
      final buffer = calloc<ffi.Char>(required);
      try {
        final copied = copy(_checkedHandle(), machine, buffer, required);
        if (copied != required) return null;
        return buffer.cast<Utf8>().toDartString();
      } finally {
        calloc.free(buffer);
      }
    } finally {
      calloc.free(machine);
    }
  }

  String? _copyHitTestString(double x, double y) {
    final required = _bindings.hitTestDocumentPoint(
      _checkedHandle(),
      x,
      y,
      ffi.nullptr,
      0,
    );
    if (required <= 1) return null;
    final buffer = calloc<ffi.Char>(required);
    try {
      final copied = _bindings.hitTestDocumentPoint(
        _checkedHandle(),
        x,
        y,
        buffer,
        required,
      );
      if (copied != required) return null;
      return buffer.cast<Utf8>().toDartString();
    } finally {
      calloc.free(buffer);
    }
  }

  void _withNativeStrings(
    String first,
    String second,
    void Function(ffi.Pointer<ffi.Char>, ffi.Pointer<ffi.Char>) body,
  ) {
    final firstPtr = first.toNativeUtf8().cast<ffi.Char>();
    final secondPtr = second.toNativeUtf8().cast<ffi.Char>();
    try {
      body(firstPtr, secondPtr);
    } finally {
      calloc.free(firstPtr);
      calloc.free(secondPtr);
    }
  }
}
