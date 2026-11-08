import 'dart:typed_data';

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kineto_runtime/kineto_runtime.dart';

/// The public player contract, against the sound values
/// engine/kineto_engine/tests/audio_events.rs pins. Shared by the host test and
/// the on-device test.
void definePlayerContract(Future<Uint8List> Function() soundsDemo) {
  Future<KinetoPlayer> sounds() async {
    final player = KinetoPlayer.loadBytes(await soundsDemo());
    addTearDown(player.dispose);
    return player;
  }

  test('garbage is a typed load error', () {
    expect(
      () => KinetoPlayer.loadBytes(Uint8List.fromList([0, 1, 2])),
      throwsA(
        isA<KinetoRuntimeException>()
            .having((e) => e.status, 'status', KinetoStatus.loadFailed),
      ),
    );
  });

  test('a document describes itself', () async {
    final player = await sounds();
    expect(player.stateMachines().map((m) => m.id), ['machine-button']);
    final press = player.inputs('machine-button').singleWhere((i) => i.name == 'press');
    expect(press.kind, KinetoInputKind.trigger);
    expect(player.inputs('no-such-machine'), isEmpty);
    expect(
      player.clips().where((c) => c.id == 'clip-idle' && c.duration > 0),
      isNotEmpty,
    );
  });

  test('a trigger reports its sounds in order, once', () async {
    final player = await sounds();
    expect(
      player.audioEvents().map((e) => (e.id, e.name)),
      [('event-click', 'ClickSound'), ('event-success', 'Success')],
    );
    expect(player.audioEvents().first.sourceName, 'click.wav');

    player.setTrigger('machine-button', 'press');
    player.advance(0.016);
    expect(player.takeFiredAudioEvents(), ['event-click']);
    expect(player.takeFiredAudioEvents(), isEmpty);

    player.advance(1.1);
    expect(player.takeFiredAudioEvents(), ['event-success']);
  });

  test('timeline audio follows the playhead', () async {
    final player = await sounds();
    expect(player.activeAudioClips(), isEmpty);
    player.selectAnimation('clip-idle');
    player.play();
    player.advance(1.5);
    final clip = player.activeAudioClips().single;
    expect(clip.trackId, 'track-1');
    expect(clip.fileName, 'success.wav');
    expect(clip.sourcePosition, closeTo(0.5, 1e-3));
    expect(clip.volume, closeTo(0.8, 1e-3));
    player.advance(1.0);
    expect(player.activeAudioClips(), isEmpty);
    expect(player.audioAssets().map((a) => a.fileName), contains('success.wav'));
  });

  test('a view point maps through the engine fit, and lands on the artwork', () async {
    final player = await sounds();
    final small = player.documentPoint(const Offset(200, 150), const Size(400, 300));
    final large = player.documentPoint(const Offset(800, 600), const Size(1600, 1200));
    expect(small.dx, closeTo(large.dx, 1e-6));
    expect(small.dy, closeTo(large.dy, 1e-6));
    expect(player.pointerEvent(KinetoPointerEvent.down, small.dx, small.dy), 1);
    expect(player.takeFiredAudioEvents(), ['event-click']);
  });

  test('a viewport hit test honours the viewport', () async {
    final player = await sounds();
    // The middle of any viewport is the shape; a raw document point there is not.
    final hit = player.hitTestViewportPoint(const Offset(50, 40), const Size(100, 80));
    expect(hit, isNotNull);
    expect(player.hitTestDocumentPoint(const Offset(50, 40)), isNull);
  });

  test('unknown names are errors carrying the engine message', () async {
    final player = await sounds();
    final invalid = throwsA(
      isA<KinetoRuntimeException>()
          .having((e) => e.status, 'status', KinetoStatus.invalidArgument),
    );
    expect(() => player.setTrigger('machine-button', 'nope'), invalid);
    expect(() => player.selectAnimation('nope'), invalid);
    expect(() => player.getTransitionProgress('nope'), invalid);
    expect(player.overrideScalar('nope', 'nope'), isNull);
  });

  test('a disposed player refuses every call', () async {
    final player = KinetoPlayer.loadBytes(await soundsDemo());
    player.dispose();
    player.dispose();
    final closed = throwsA(isA<KinetoRuntimeException>());
    expect(() => player.advance(0.016), closed);
    expect(() => player.play(), closed);
    expect(() => player.takeFiredAudioEvents(), closed);
    expect(() => player.pointerMoved(0, 0), closed);
    expect(() => player.retainForView(), closed);
  });

  test('a retained player outlives its disposal until the reference is gone', () async {
    final player = KinetoPlayer.loadBytes(await soundsDemo());
    expect(player.retainForView(), isNonZero);
    player.dispose();
    expect(player.isDisposed, isTrue);
  });
}
