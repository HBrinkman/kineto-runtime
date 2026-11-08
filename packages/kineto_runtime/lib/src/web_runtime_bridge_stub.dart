import 'dart:typed_data';

Future<Object?> createWasmPlayer(Uint8List bytes) async {
  final _ = bytes;
  return null;
}

void advanceWasmPlayer(Object player, double deltaSeconds) {
  final _ = (player, deltaSeconds);
}

void setWasmBool(
  Object player,
  String stateMachineId,
  String inputName,
  bool value,
) {
  final _ = (player, stateMachineId, inputName, value);
}

void setWasmNumber(
  Object player,
  String stateMachineId,
  String inputName,
  double value,
) {
  final _ = (player, stateMachineId, inputName, value);
}

int wasmPointerEvent(Object player, String event, double x, double y) {
  final _ = (player, event, x, y);
  throw UnsupportedError('The wasm bridge exists only on web.');
}

void setWasmTrigger(Object player, String stateMachineId, String inputName) {
  final _ = (player, stateMachineId, inputName);
}

String? getWasmActiveState(Object player, String stateMachineId) {
  final _ = (player, stateMachineId);
  return null;
}

String? getWasmActiveTransition(Object player, String stateMachineId) {
  final _ = (player, stateMachineId);
  return null;
}

double getWasmTransitionProgress(Object player, String stateMachineId) {
  final _ = (player, stateMachineId);
  return 0;
}

String? hitTestWasmDocumentPoint(Object player, double x, double y) {
  final _ = (player, x, y);
  return null;
}

void disposeWasmPlayer(Object player) {
  final _ = player;
}

void renderWasmToCanvas(Object player, String canvasId) {
  final _ = (player, canvasId);
}
