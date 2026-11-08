import 'dart:ffi' as ffi;

typedef KinetoLoadNative = ffi.Pointer<ffi.Void> Function(
  ffi.Pointer<ffi.Uint8>,
  ffi.UintPtr,
  ffi.Pointer<ffi.Int32>,
);
typedef KinetoLoadDart = ffi.Pointer<ffi.Void> Function(
  ffi.Pointer<ffi.Uint8>,
  int,
  ffi.Pointer<ffi.Int32>,
);

typedef KinetoAdvanceNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Double,
);
typedef KinetoAdvanceDart = int Function(ffi.Pointer<ffi.Void>, double);

typedef KinetoSetBoolNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  ffi.Bool,
);
typedef KinetoSetBoolDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  bool,
);

typedef KinetoSetNumberNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  ffi.Double,
);
typedef KinetoSetNumberDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  double,
);

typedef KinetoPointerEventNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Int32,
  ffi.Double,
  ffi.Double,
  ffi.Pointer<ffi.Uint32>,
);
typedef KinetoPointerEventDart = int Function(
  ffi.Pointer<ffi.Void>,
  int,
  double,
  double,
  ffi.Pointer<ffi.Uint32>,
);

typedef KinetoSetTriggerNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
);
typedef KinetoSetTriggerDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
);

typedef KinetoCopyStringNative = ffi.UintPtr Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  ffi.UintPtr,
);
typedef KinetoCopyStringDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  int,
);

typedef KinetoTransitionProgressNative = ffi.Double Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Int32>,
);
typedef KinetoTransitionProgressDart = double Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Int32>,
);

typedef KinetoHitTestNative = ffi.UintPtr Function(
  ffi.Pointer<ffi.Void>,
  ffi.Double,
  ffi.Double,
  ffi.Pointer<ffi.Char>,
  ffi.UintPtr,
);
typedef KinetoHitTestDart = int Function(
  ffi.Pointer<ffi.Void>,
  double,
  double,
  ffi.Pointer<ffi.Char>,
  int,
);

typedef KinetoLastErrorNative = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
);
typedef KinetoLastErrorDart = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
);

typedef KinetoDestroyNative = ffi.Void Function(ffi.Pointer<ffi.Void>);
typedef KinetoDestroyDart = void Function(ffi.Pointer<ffi.Void>);

typedef KinetoPlayerStatusNative = ffi.Int32 Function(ffi.Pointer<ffi.Void>);
typedef KinetoPlayerStatusDart = int Function(ffi.Pointer<ffi.Void>);

typedef KinetoPointerMovedNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Double,
  ffi.Double,
);
typedef KinetoPointerMovedDart = int Function(
  ffi.Pointer<ffi.Void>,
  double,
  double,
);

typedef KinetoDocumentPointNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Double,
  ffi.Double,
  ffi.Double,
  ffi.Double,
  ffi.Pointer<ffi.Double>,
  ffi.Pointer<ffi.Double>,
);
typedef KinetoDocumentPointDart = int Function(
  ffi.Pointer<ffi.Void>,
  double,
  double,
  double,
  double,
  ffi.Pointer<ffi.Double>,
  ffi.Pointer<ffi.Double>,
);

typedef KinetoOverrideScalarNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Double>,
);
typedef KinetoOverrideScalarDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Char>,
  ffi.Pointer<ffi.Double>,
);

typedef KinetoBackgroundColorNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Uint8>,
);
typedef KinetoBackgroundColorDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Uint8>,
);

typedef KinetoSelectAnimationNative = ffi.Int32 Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
);
typedef KinetoSelectAnimationDart = int Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
);

typedef KinetoRecordsNative = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
);
typedef KinetoRecordsDart = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
);

typedef KinetoMachineInputsNative = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
);
typedef KinetoMachineInputsDart = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Char>,
);

typedef KinetoTakeFiredNative = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Uint32>,
);
typedef KinetoTakeFiredDart = ffi.Pointer<ffi.Char> Function(
  ffi.Pointer<ffi.Void>,
  ffi.Pointer<ffi.Uint32>,
);

typedef KinetoSurfaceLastErrorNative = ffi.Pointer<ffi.Char> Function();
typedef KinetoSurfaceLastErrorDart = ffi.Pointer<ffi.Char> Function();

final class KinetoEngineBindings {
  KinetoEngineBindings(this.library)
      : destroyNative = library.lookup('kineto_destroy') {
    finalizer = ffi.NativeFinalizer(destroyNative.cast());
  }

  final ffi.DynamicLibrary library;
  final ffi.Pointer<ffi.NativeFunction<KinetoDestroyNative>> destroyNative;
  late final ffi.NativeFinalizer finalizer;

  late final KinetoLoadDart load =
      library.lookupFunction<KinetoLoadNative, KinetoLoadDart>('kineto_load');

  late final KinetoAdvanceDart advance = library
      .lookupFunction<KinetoAdvanceNative, KinetoAdvanceDart>('kineto_advance');

  late final KinetoSetBoolDart setBool =
      library.lookupFunction<KinetoSetBoolNative, KinetoSetBoolDart>(
          'kineto_set_bool');

  late final KinetoSetNumberDart setNumber =
      library.lookupFunction<KinetoSetNumberNative, KinetoSetNumberDart>(
          'kineto_set_number');

  late final KinetoPointerEventDart pointerEvent =
      library.lookupFunction<KinetoPointerEventNative, KinetoPointerEventDart>(
          'kineto_pointer_event');

  late final KinetoSetTriggerDart setTrigger =
      library.lookupFunction<KinetoSetTriggerNative, KinetoSetTriggerDart>(
    'kineto_set_trigger',
  );

  late final KinetoCopyStringDart getActiveState =
      library.lookupFunction<KinetoCopyStringNative, KinetoCopyStringDart>(
    'kineto_get_active_state',
  );

  late final KinetoCopyStringDart getActiveTransition =
      library.lookupFunction<KinetoCopyStringNative, KinetoCopyStringDart>(
    'kineto_get_active_transition',
  );

  late final KinetoTransitionProgressDart getTransitionProgress =
      library.lookupFunction<KinetoTransitionProgressNative,
          KinetoTransitionProgressDart>('kineto_get_transition_progress');

  late final KinetoHitTestDart hitTestDocumentPoint =
      library.lookupFunction<KinetoHitTestNative, KinetoHitTestDart>(
    'kineto_hit_test_document_point',
  );

  late final KinetoLastErrorDart lastError =
      library.lookupFunction<KinetoLastErrorNative, KinetoLastErrorDart>(
    'kineto_last_error_message',
  );

  late final KinetoPlayerStatusDart retain =
      library.lookupFunction<KinetoPlayerStatusNative, KinetoPlayerStatusDart>(
    'kineto_retain',
  );

  late final KinetoPointerMovedDart pointerMoved =
      library.lookupFunction<KinetoPointerMovedNative, KinetoPointerMovedDart>(
    'kineto_pointer_moved',
  );

  late final KinetoDocumentPointDart documentPoint = library
      .lookupFunction<KinetoDocumentPointNative, KinetoDocumentPointDart>(
    'kineto_document_point',
  );

  late final KinetoOverrideScalarDart overrideScalar = library
      .lookupFunction<KinetoOverrideScalarNative, KinetoOverrideScalarDart>(
    'kineto_override_scalar',
  );

  late final KinetoBackgroundColorDart backgroundColor = library
      .lookupFunction<KinetoBackgroundColorNative, KinetoBackgroundColorDart>(
    'kineto_background_color',
  );

  late final KinetoPlayerStatusDart play =
      library.lookupFunction<KinetoPlayerStatusNative, KinetoPlayerStatusDart>(
    'kineto_play',
  );

  late final KinetoPlayerStatusDart pause =
      library.lookupFunction<KinetoPlayerStatusNative, KinetoPlayerStatusDart>(
    'kineto_pause',
  );

  late final KinetoSelectAnimationDart selectAnimation = library
      .lookupFunction<KinetoSelectAnimationNative, KinetoSelectAnimationDart>(
    'kineto_select_animation',
  );

  late final KinetoRecordsDart stateMachines =
      library.lookupFunction<KinetoRecordsNative, KinetoRecordsDart>(
    'kineto_state_machines',
  );

  late final KinetoMachineInputsDart machineInputs =
      library.lookupFunction<KinetoMachineInputsNative, KinetoMachineInputsDart>(
    'kineto_machine_inputs',
  );

  late final KinetoRecordsDart animations =
      library.lookupFunction<KinetoRecordsNative, KinetoRecordsDart>(
    'kineto_animations',
  );

  late final KinetoRecordsDart audioAssets =
      library.lookupFunction<KinetoRecordsNative, KinetoRecordsDart>(
    'kineto_audio_assets',
  );

  late final KinetoRecordsDart activeAudioClips =
      library.lookupFunction<KinetoRecordsNative, KinetoRecordsDart>(
    'kineto_active_audio_clips',
  );

  late final KinetoRecordsDart audioEvents =
      library.lookupFunction<KinetoRecordsNative, KinetoRecordsDart>(
    'kineto_audio_events',
  );

  late final KinetoTakeFiredDart takeFiredAudioEvents =
      library.lookupFunction<KinetoTakeFiredNative, KinetoTakeFiredDart>(
    'kineto_take_fired_audio_events',
  );

  late final KinetoSurfaceLastErrorDart surfaceLastError = library
      .lookupFunction<KinetoSurfaceLastErrorNative, KinetoSurfaceLastErrorDart>(
    'kineto_surface_last_error_message',
  );

  late final KinetoDestroyDart destroy =
      library.lookupFunction<KinetoDestroyNative, KinetoDestroyDart>(
    'kineto_destroy',
  );
}
