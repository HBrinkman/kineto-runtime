import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('should keep all Dart FFI symbols rooted when using iOS SPM', () {
    // Arrange
    final bindings = File('lib/src/ffi/bindings.dart').readAsStringSync();
    final manifest = File(
      'ios/kineto_runtime/Package.swift',
    ).readAsStringSync();
    final ffiSymbols = RegExp(
      r"'(kineto_[a-z_]+)'",
    ).allMatches(bindings).map((match) => match.group(1)!).toSet();

    // Act
    final missingRoots =
        ffiSymbols.where((symbol) => !manifest.contains('"$symbol"')).toList();

    // Assert
    expect(ffiSymbols, isNotEmpty);
    expect(missingRoots, isEmpty);
  });
}
