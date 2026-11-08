import 'dart:async';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

Future<Object?> createWasmPlayer(Uint8List bytes) async {
  final bridge = await _waitForBridge();
  if (bridge == null) return null;
  final promise = bridge.callMethod<JSPromise<JSAny?>>(
    'createPlayer'.toJS,
    bytes.toJS,
  );
  return (await promise.toDart) as Object?;
}

void advanceWasmPlayer(Object player, double deltaSeconds) {
  _jsPlayer(player).callMethod<JSAny?>('advance'.toJS, deltaSeconds.toJS);
}

void setWasmBool(
  Object player,
  String stateMachineId,
  String inputName,
  bool value,
) {
  _jsPlayer(player).callMethod<JSAny?>(
    'setBool'.toJS,
    stateMachineId.toJS,
    inputName.toJS,
    value.toJS,
  );
}

void setWasmNumber(
  Object player,
  String stateMachineId,
  String inputName,
  double value,
) {
  _jsPlayer(player).callMethod<JSAny?>(
    'setNumber'.toJS,
    stateMachineId.toJS,
    inputName.toJS,
    value.toJS,
  );
}

int wasmPointerEvent(Object player, String event, double x, double y) =>
    _jsPlayer(player)
        .callMethod<JSNumber>(
          'pointerEvent'.toJS,
          event.toJS,
          x.toJS,
          y.toJS,
        )
        .toDartInt;

void setWasmTrigger(Object player, String stateMachineId, String inputName) {
  _jsPlayer(player).callMethod<JSAny?>(
    'setTrigger'.toJS,
    stateMachineId.toJS,
    inputName.toJS,
  );
}

String? getWasmActiveState(Object player, String stateMachineId) =>
    _optionalString(
      _jsPlayer(player).callMethod<JSAny?>(
        'activeState'.toJS,
        stateMachineId.toJS,
      ),
    );

String? getWasmActiveTransition(Object player, String stateMachineId) =>
    _optionalString(
      _jsPlayer(player).callMethod<JSAny?>(
        'activeTransition'.toJS,
        stateMachineId.toJS,
      ),
    );

double getWasmTransitionProgress(Object player, String stateMachineId) {
  final result = _jsPlayer(player).callMethod<JSNumber>(
    'transitionProgress'.toJS,
    stateMachineId.toJS,
  );
  return result.toDartDouble;
}

String? hitTestWasmDocumentPoint(Object player, double x, double y) =>
    _optionalString(
      _jsPlayer(player).callMethod<JSAny?>(
        'hitTestDocumentPoint'.toJS,
        x.toJS,
        y.toJS,
      ),
    );

void disposeWasmPlayer(Object player) {
  _jsPlayer(player).callMethod<JSAny?>('free'.toJS);
}

void renderWasmToCanvas(Object player, String canvasId) {
  _jsPlayer(player).callMethod<JSAny?>('renderToCanvas'.toJS, canvasId.toJS);
}

JSObject _jsPlayer(Object player) => player as JSObject;

String? _optionalString(JSAny? value) {
  if (value.isUndefinedOrNull) return null;
  return (value as JSString).toDart;
}

Future<JSObject?> _waitForBridge() async {
  for (var attempt = 0; attempt < 50; attempt += 1) {
    final bridge =
        globalContext.getProperty<JSObject?>('__kinetoRuntimeBridge'.toJS);
    if (bridge != null) return bridge;
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  return null;
}
