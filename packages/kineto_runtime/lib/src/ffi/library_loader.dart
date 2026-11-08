import 'dart:ffi' as ffi;
import 'dart:io';

ffi.DynamicLibrary loadKinetoEngineLibrary() {
  if (Platform.isIOS) {
    return ffi.DynamicLibrary.process();
  }
  if (Platform.isAndroid) {
    return ffi.DynamicLibrary.open('libkineto_engine.so');
  }

  final override = Platform.environment['KINETO_ENGINE_DYLIB'];
  if (override != null && override.isNotEmpty) {
    return ffi.DynamicLibrary.open(override);
  }

  for (final path in _candidatePaths()) {
    if (File(path).existsSync()) {
      return ffi.DynamicLibrary.open(path);
    }
  }

  return ffi.DynamicLibrary.process();
}

Iterable<String> _candidatePaths() sync* {
  final fileName = switch (Platform.operatingSystem) {
    'macos' => 'libkineto_engine.dylib',
    'linux' => 'libkineto_engine.so',
    'windows' => 'kineto_engine.dll',
    _ => 'libkineto_engine.dylib',
  };

  // Cargo writes into <repo>/build/cargo, not engine/target: the old location
  // sat in a directory `flutter build ios` walks with `xattr -r`, and a few
  // gigabytes of build output there cost minutes on every iOS build
  // (engine/.cargo/config.toml).
  //
  // The engine/target paths are kept below because they are still correct for
  // a checkout that predates that change, for CARGO_TARGET_DIR pointing back
  // at it, and because a wrong guess here is a missing dylib rather than a
  // wrong one.
  for (final root in const [
    '../../build/cargo',
    'build/cargo',
    '../build/cargo',
    '../../engine/target',
    'engine/target',
    '../engine/target',
  ]) {
    yield '$root/debug/$fileName';
    yield '$root/release/$fileName';
  }
}
