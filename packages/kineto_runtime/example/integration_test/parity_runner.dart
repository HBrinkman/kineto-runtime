import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:kineto_runtime/kineto_runtime.dart';

/// Runs runtimes/parity/runtime_fixtures.json with the expectations
/// runtimes/parity/run_wasm_parity.mjs and the Android ParityFixturesTest use.
/// Shared so the host test and the on-device test cannot drift apart.
Future<int> runParityFixtures({
  required String manifestJson,
  required Future<Uint8List> Function(String repoPath) loadAsset,
}) async {
  final manifest = jsonDecode(manifestJson) as Map<String, Object?>;
  var executed = 0;
  for (final fixture in (manifest['fixtures']! as List).cast<Map<String, Object?>>()) {
    // A fixture with a platform belongs to that platform's own runner.
    if (fixture.containsKey('platform')) continue;
    await _runFixture(fixture, loadAsset);
    executed++;
  }
  return executed;
}

Future<void> _runFixture(
  Map<String, Object?> fixture,
  Future<Uint8List> Function(String repoPath) loadAsset,
) async {
  final id = fixture['id'];
  KinetoPlayer? player;
  try {
    for (final step in (fixture['steps']! as List).cast<Map<String, Object?>>()) {
      if (step['op'] == 'load') {
        player = KinetoPlayer.loadBytes(await loadAsset(fixture['asset']! as String));
        continue;
      }
      _execute(id, fixture, player!, step);
    }
  } finally {
    player?.dispose();
  }
}

void _execute(
  Object? id,
  Map<String, Object?> fixture,
  KinetoPlayer player,
  Map<String, Object?> step,
) {
  double number(String key) => (step[key]! as num).toDouble();
  String text(String key) => step[key]! as String;

  switch (step['op']) {
    case 'advance':
      player.advance(number('deltaSeconds'));
    case 'pointerEvent':
      final fired = player.pointerEvent(
        KinetoPointerEvent.values.byName(text('event')),
        number('x'),
        number('y'),
      );
      if (step.containsKey('expectFired')) {
        expect(fired, step['expectFired'], reason: '$id: listeners fired');
      }
    case 'setNumber':
      player.setNumber(text('stateMachineId'), text('inputName'), number('value'));
    case 'setTrigger':
      player.setTrigger(text('stateMachineId'), text('inputName'));
    case 'expectActiveState':
      expect(
        player.getActiveState(text('stateMachineId')),
        step['stateId'],
        reason: '$id: active state of ${step['stateMachineId']}',
      );
    case 'expectOverrideScalar':
      final actual = player.overrideScalar(text('nodeId'), text('key'));
      expect(actual, isNotNull, reason: '$id: no ${step['nodeId']}.${step['key']}');
      expect(
        actual,
        closeTo(number('value'), 1e-6),
        reason: '$id: ${step['nodeId']}.${step['key']}',
      );
    case 'expectBackground':
      expect(
        player.backgroundColor(),
        step['rgba'],
        reason: '$id: background',
      );
    case 'hitTestDocumentPoint':
      final expected = (fixture['expect']! as Map<String, Object?>)['nodeId'];
      expect(
        player.hitTestDocumentPoint(Offset(number('x'), number('y')))?.nodeId,
        expected,
        reason: '$id: hit test',
      );
    default:
      fail('$id: this runner does not know op ${step['op']}');
  }
}
