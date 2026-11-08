/// Pointer events a host can deliver to the runtime (issue #74).
///
/// Order matches the C enum: a listener in the document decides what a touch
/// at a point means, so the host names a place and an event, never an input.
enum KinetoPointerEvent { down, up, enter, exit }

enum KinetoStatus {
  ok(0),
  nullPointer(1),
  invalidArgument(2),
  loadFailed(3),
  advanceFailed(4),
  renderFailed(5),
  unsupported(6);

  const KinetoStatus(this.code);

  final int code;

  static KinetoStatus fromCode(int code) {
    for (final status in values) {
      if (status.code == code) return status;
    }
    return invalidArgument;
  }
}

final class KinetoRuntimeException implements Exception {
  const KinetoRuntimeException(this.status, this.message);

  final KinetoStatus status;
  final String message;

  @override
  String toString() => 'KinetoRuntimeException(${status.name}): $message';
}

final class RuntimeTapTarget {
  const RuntimeTapTarget({required this.nodeId});

  final String nodeId;
}

/// One state machine in the document.
final class KinetoMachineInfo {
  const KinetoMachineInfo({required this.id, required this.name});

  final String id;
  final String name;
}

/// Order matches the C ABI's input records.
enum KinetoInputKind { bool, trigger, number }

/// One input a state machine exposes.
final class KinetoInputInfo {
  const KinetoInputInfo({
    required this.id,
    required this.name,
    required this.kind,
    required this.defaultBool,
    required this.defaultNumber,
  });

  final String id;
  final String name;
  final KinetoInputKind kind;
  final bool defaultBool;
  final double defaultNumber;
}

/// One animation clip.
final class KinetoClipInfo {
  const KinetoClipInfo({
    required this.id,
    required this.name,
    required this.duration,
  });

  final String id;
  final String name;
  final double duration;
}

/// One audio file a document refers to. A `.kvk` names its audio and carries
/// none of it, so this is what a host has to ship.
final class KinetoAudioAssetInfo {
  const KinetoAudioAssetInfo({
    required this.id,
    required this.fileName,
    required this.duration,
  });

  final String id;
  final String fileName;
  final double duration;
}

/// One audio clip that should be sounding at the current playhead.
final class KinetoActiveAudioClip {
  const KinetoActiveAudioClip({
    required this.trackId,
    required this.assetId,
    required this.fileName,
    required this.sourcePosition,
    required this.volume,
  });

  final String trackId;
  final String assetId;
  final String fileName;

  /// The point inside the file that belongs at the current playhead.
  final double sourcePosition;
  final double volume;
}

/// One sound a state machine can fire.
final class KinetoAudioEvent {
  const KinetoAudioEvent({
    required this.id,
    required this.name,
    this.sourceName,
  });

  final String id;
  final String name;
  final String? sourceName;
}

final class KinetoRuntimeDiagnostics {
  const KinetoRuntimeDiagnostics({
    required this.platform,
    required this.renderer,
    required this.isDisposed,
    this.assetName,
    this.nativeHandleAddress,
    this.lastError,
  });

  final String platform;
  final String renderer;
  final bool isDisposed;
  final String? assetName;
  final int? nativeHandleAddress;
  final String? lastError;
}

/// Splits the runtime's newline/tab record format, dropping any row without the
/// expected field count: a malformed row is a bug to notice elsewhere, not
/// something to half-render.
List<List<String>> parseKinetoRecords(String joined, int fields) => [
      for (final line in joined.split('\n'))
        if (line.isNotEmpty)
          if (_splitFields(line, fields) case final parts?) parts,
    ];

List<String>? _splitFields(String line, int fields) {
  final parts = line.split('\t');
  if (parts.length < fields) return null;
  // The last field keeps any further tabs, matching the Swift and Kotlin parsers.
  return [...parts.take(fields - 1), parts.skip(fields - 1).join('\t')];
}
