import 'dart:ffi' as ffi;
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kineto_runtime/kineto_runtime.dart';

import 'parity_runner.dart';
import 'runtime_contract.dart';

Future<Uint8List> _asset(String key) async {
  final data = await rootBundle.load(key);
  return data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes);
}

/// The fixtures are bundled under their file name: see the `assets/` copies, kept
/// byte-identical by test/tool/android_runtime_contract_test.dart.
Future<Uint8List> _fixture(String repoPath) {
  final name = repoPath.split('/').last;
  const top = {'sample.kvk', 'pointer_binding.kvk'};
  return _asset(top.contains(name) ? 'assets/$name' : 'assets/parity/$name');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('every portable parity fixture matches the other runtimes', (_) async {
    final manifest = await rootBundle.loadString('assets/parity/runtime_fixtures.json');
    final executed = await runParityFixtures(manifestJson: manifest, loadAsset: _fixture);
    expect(executed, greaterThanOrEqualTo(6));
  });

  definePlayerContract(() => _asset('assets/parity/sounds_demo.kvk'));

  testWidgets('Dart shares the thread the platform view draws on', (_) async {
    // The native view advances and renders on the main thread, and a handle is
    // not safe across threads. That holds only while Flutter merges its UI thread
    // into the platform thread, so pin it.
    final libc = ffi.DynamicLibrary.process();
    if (Platform.isAndroid) {
      final gettid =
          libc.lookupFunction<ffi.Int32 Function(), int Function()>('gettid');
      expect(gettid(), pid);
    } else {
      final isMain = libc.lookupFunction<ffi.Int32 Function(), int Function()>(
        'pthread_main_np',
      );
      expect(isMain(), 1);
    }
  });

  testWidgets('disposing a player under a live view does not touch freed memory',
      (tester) async {
    for (var round = 0; round < 20; round++) {
      final player = await KinetoPlayer.loadAsset('assets/sample.kvk');
      await tester.pumpWidget(MaterialApp(home: player.asWidget()));
      await tester.pump(const Duration(milliseconds: 400));
      player.dispose();
      // Reuse the freed blocks, so a dangling handle reads garbage and faults.
      final junk = [
        for (var i = 0; i < 40; i++) await KinetoPlayer.loadAsset('assets/sample.kvk'),
      ];
      await Future<void>.delayed(const Duration(milliseconds: 300));
      for (final other in junk) {
        other.dispose();
      }
      await tester.pumpWidget(const SizedBox());
      await tester.pump(const Duration(milliseconds: 100));
    }
  });

  testWidgets('a tap in the view reaches the listener under it', (tester) async {
    final player = await KinetoPlayer.loadAsset('assets/parity/sounds_demo.kvk');
    addTearDown(player.dispose);
    final fired = <(KinetoPointerEvent, int)>[];
    await tester.pumpWidget(
      MaterialApp(
        home: KinetoPlayerView(
          player: player,
          onPointerEvent: (event, count) => fired.add((event, count)),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 300));

    await tester.tapAt(tester.getCenter(find.byType(KinetoPlayerView)));

    expect(fired, hasLength(2));
    expect(fired.first, (KinetoPointerEvent.down, 1));
    expect(fired.last.$1, KinetoPointerEvent.up);
    expect(player.takeFiredAudioEvents(), contains('event-click'));

    await tester.tapAt(const Offset(2, 2));
    expect(fired[fired.length - 2], (KinetoPointerEvent.down, 0));
  });

  testWidgets('a cancelled drag releases the pointer', (tester) async {
    final player = await KinetoPlayer.loadAsset('assets/parity/sounds_demo.kvk');
    addTearDown(player.dispose);
    final fired = <KinetoPointerEvent>[];
    await tester.pumpWidget(
      MaterialApp(
        home: KinetoPlayerView(
          player: player,
          onPointerEvent: (event, _) => fired.add(event),
        ),
      ),
    );
    final center = tester.getCenter(find.byType(KinetoPlayerView));
    final gesture = await tester.startGesture(center);
    await gesture.moveBy(const Offset(5, 5));
    await gesture.cancel();
    expect(fired, [KinetoPointerEvent.down, KinetoPointerEvent.up]);
  });

  testWidgets('swapping players and autoplay keeps each view on its own reference',
      (tester) async {
    final first = await KinetoPlayer.loadAsset('assets/sample.kvk');
    final second = await KinetoPlayer.loadAsset('assets/parity/sounds_demo.kvk');
    Widget host(KinetoPlayer player, bool autoplay) =>
        MaterialApp(home: KinetoPlayerView(player: player, autoplay: autoplay));

    await tester.pumpWidget(host(first, true));
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpWidget(host(second, true));
    first.dispose();
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpWidget(host(second, false));
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpWidget(const SizedBox());
    second.dispose();
    await tester.pump(const Duration(milliseconds: 200));
  });

  testWidgets('diagnostics name the platform path and report its state', (tester) async {
    final player = await KinetoPlayer.loadAsset('assets/sample.kvk');
    addTearDown(player.dispose);
    await tester.pumpWidget(MaterialApp(home: player.asWidget()));
    await tester.pump(const Duration(seconds: 1));
    final diagnostics = player.diagnostics();
    expect(diagnostics.platform, Platform.operatingSystem);
    expect(diagnostics.renderer, contains(Platform.isAndroid ? 'Android' : 'iOS'));
    expect(diagnostics.renderer, isNot(contains('unavailable')));
    // Dart's Platform.environment is empty on iOS, so a run has to say it targets
    // the simulator: --dart-define=KINETO_NO_GPU=true.
    if (const bool.fromEnvironment('KINETO_NO_GPU')) {
      // The iOS simulator lacks the indirect execution vello needs. The view has
      // to report that rather than take the app down.
      expect(diagnostics.lastError, contains('INDIRECT_EXECUTION'));
    } else {
      expect(diagnostics.lastError, isNull);
    }
    await tester.pumpWidget(const SizedBox());
  });
}
