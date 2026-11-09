Pod::Spec.new do |s|
  s.name             = 'kineto_runtime'
  s.version          = '0.1.0'
  s.summary          = 'Flutter bridge for the Kineto Rust runtime.'
  s.description      = 'Registers the iOS runtime player platform view and links the Kineto Rust runtime C ABI.'
  s.homepage         = 'https://Kineto.keyvotion.com'
  s.license          = { :type => 'MIT', :file => '../LICENSE' }
  s.author           = { 'Kineto' => 'hello@keyvotion.com' }
  s.source           = { :path => '.' }
  # CocoaPods remains a fallback for Flutter apps that have not enabled SPM.
  # Both integrations compile the same bridge source.
  s.source_files     = 'kineto_runtime/Sources/kineto_runtime/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '15.0'
  s.swift_version = '5.0'
  s.static_framework = true

  # Same XCFramework the Swift package uses: a symlink into runtimes/ in a
  # repository checkout, a real directory in the published package. realpath
  # resolves the link so Xcode gets a path that exists from any directory.
  xcframework = File.realpath(File.join(__dir__, 'kineto_runtime', 'KinetoRuntimeC.xcframework'))
  headers = File.join(xcframework, 'ios-arm64', 'Headers')
  module_map = File.join(headers, 'module.modulemap')
  device_lib = File.join(xcframework, 'ios-arm64')
  simulator_lib = File.join(xcframework, 'ios-arm64_x86_64-simulator')

  s.pod_target_xcconfig = {
    'HEADER_SEARCH_PATHS' => "$(inherited) \"#{headers}\"",
    'OTHER_SWIFT_FLAGS' => "$(inherited) -Xcc -fmodule-map-file=#{module_map}"
  }

  # The Dart side drives the engine through dart:ffi
  # (DynamicLibrary.process()), which the static linker cannot see — with
  # -dead_strip (Xcode release default) every C ABI symbol not referenced
  # from native code is stripped from the app binary and the FFI lookup
  # fails at runtime ("engine not linked", issue #54). Marking each entry
  # point as a link root (-u) keeps the full ABI alive without
  # -force_load'ing the whole archive. Keep this list in sync with
  # engine/kineto_engine/src/ffi/c.rs and tool/check_runtime_symbols.sh.
  # Every `#[no_mangle] pub extern "C"` in engine/kineto_engine/src/ffi/c.rs
  # except the Android-only surface constructor, which is not compiled into
  # the iOS slice. Regenerate with:
  #   grep -A1 '^#\[no_mangle\]' engine/kineto_engine/src/ffi/c.rs \
  #     | grep -oE 'kineto_[a-z_]+' | sort -u
  ffi_roots = %w[
    kineto_active_audio_clips
    kineto_advance
    kineto_animations
    kineto_audio_assets
    kineto_audio_events
    kineto_destroy
    kineto_override_scalar
    kineto_retain
    kineto_get_active_state
    kineto_get_active_transition
    kineto_get_transition_progress
    kineto_hit_test_document_point
    kineto_last_error_message
    kineto_load
    kineto_machine_inputs
    kineto_pause
    kineto_play
    kineto_document_point
    kineto_background_color
    kineto_pointer_event
    kineto_pointer_moved
    kineto_render
    kineto_select_animation
    kineto_set_bool
    kineto_set_number
    kineto_set_trigger
    kineto_state_machines
    kineto_surface_create_from_ca_metal_layer
    kineto_surface_destroy
    kineto_surface_last_error_message
    kineto_surface_resize
    kineto_take_fired_audio_events
  ].map { |sym| "-Wl,-u,_#{sym}" }.join(' ')

  s.user_target_xcconfig = {
    'LIBRARY_SEARCH_PATHS[sdk=iphoneos*]' => "$(inherited) \"#{device_lib}\"",
    'LIBRARY_SEARCH_PATHS[sdk=iphonesimulator*]' => "$(inherited) \"#{simulator_lib}\"",
    'OTHER_LDFLAGS[sdk=iphoneos*]' => "$(inherited) -lkineto_engine #{ffi_roots}",
    'OTHER_LDFLAGS[sdk=iphonesimulator*]' => "$(inherited) -lkineto_engine_sim #{ffi_roots}"
  }
end
