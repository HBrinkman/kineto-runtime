import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:kineto_runtime/kineto_runtime.dart';

void main() {
  test('should throw load error when bytes are not a kvk document', () {
    expect(
      () => KinetoPlayer.loadBytes(Uint8List.fromList([0, 1, 2])),
      throwsA(isA<KinetoRuntimeException>()),
    );
  });

  test('should load sample kvk and advance through Rust FFI when dylib exists',
      () {
    final sample = File('example/assets/sample.kvk');
    expect(sample.existsSync(), isTrue);

    final player = KinetoPlayer.loadBytes(sample.readAsBytesSync());
    addTearDown(player.dispose);

    player.advance(0.016);
    expect(player.nativeHandleAddress, isNonZero);
  });

  test('should load sample kvk through async bytes API', () async {
    final sample = File('example/assets/sample.kvk');
    expect(sample.existsSync(), isTrue);

    final player = await KinetoPlayer.loadBytesAsync(sample.readAsBytesSync());
    addTearDown(player.dispose);

    player.advance(0.016);
    expect(player.nativeHandleAddress, isNonZero);
  });

  test('should expose diagnostics for a loaded player', () {
    final sample = File('example/assets/sample.kvk');
    expect(sample.existsSync(), isTrue);

    final player = KinetoPlayer.loadBytes(sample.readAsBytesSync());
    addTearDown(player.dispose);

    final diagnostics =
        player.diagnostics(assetName: 'example/assets/sample.kvk');

    expect(diagnostics.assetName, 'example/assets/sample.kvk');
    expect(diagnostics.isDisposed, isFalse);
    expect(diagnostics.nativeHandleAddress, isNonZero);
    expect(diagnostics.platform, isNotEmpty);
    expect(diagnostics.renderer, isNotEmpty);
  });
}
