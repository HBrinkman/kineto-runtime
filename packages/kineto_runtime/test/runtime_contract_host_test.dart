import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../example/integration_test/parity_runner.dart';
import '../example/integration_test/runtime_contract.dart';

/// The contract and parity fixtures through Dart FFI on the development machine.
/// example/integration_test/runtime_device_test.dart runs the same code on a device.
void main() {
  const repo = '../..';

  test('every portable parity fixture matches the other runtimes', () async {
    final executed = await runParityFixtures(
      manifestJson: File('$repo/runtimes/parity/runtime_fixtures.json').readAsStringSync(),
      loadAsset: (path) => File('$repo/$path').readAsBytes(),
    );
    expect(executed, greaterThanOrEqualTo(6));
  });

  definePlayerContract(
    () => File('$repo/engine/kineto_engine/tests/fixtures/sounds_demo.kvk').readAsBytes(),
  );
}
