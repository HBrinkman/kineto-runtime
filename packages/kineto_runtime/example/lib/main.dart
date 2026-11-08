import 'package:flutter/material.dart';
import 'package:kineto_runtime/kineto_runtime.dart';

void main() => runApp(const KinetoRuntimeExampleApp());

class KinetoRuntimeExampleApp extends StatelessWidget {
  const KinetoRuntimeExampleApp({super.key});

  @override
  Widget build(BuildContext context) {
    return const MaterialApp(
      title: 'kineto_runtime example',
      home: SceneViewerPage(),
    );
  }
}

class SceneViewerPage extends StatefulWidget {
  const SceneViewerPage({super.key});

  @override
  State<SceneViewerPage> createState() => _SceneViewerPageState();
}

class _SceneViewerPageState extends State<SceneViewerPage> {
  // The Android smoke script plays a document with a tappable shape.
  static const _sampleAsset = String.fromEnvironment(
    'KINETO_ASSET',
    defaultValue: 'assets/sample.kvk',
  );

  KinetoPlayer? _player;
  String? _loadError;

  @override
  void initState() {
    super.initState();
    _loadScene();
  }

  @override
  void dispose() {
    _player?.dispose();
    super.dispose();
  }

  Future<void> _loadScene() async {
    try {
      final player = await KinetoPlayer.loadAsset(_sampleAsset);
      if (!mounted) return;
      setState(() {
        _player = player;
        _loadError = null;
      });
    } on Object catch (error) {
      if (!mounted) return;
      setState(() => _loadError = error.toString());
    }
  }

  @override
  Widget build(BuildContext context) {
    final player = _player;
    final diagnostics = player?.diagnostics(
      assetName: _sampleAsset,
      lastError: _loadError,
    );
    return Scaffold(
      backgroundColor: const Color(0xFF101419),
      appBar: AppBar(
        title: const Text('Kineto Runtime'),
        backgroundColor: const Color(0xFF18202A),
        foregroundColor: Colors.white,
      ),
      body: Column(
        children: [
          _DiagnosticsPanel(
            diagnostics: diagnostics,
            loadError: _loadError,
          ),
          Expanded(
            child: _RuntimeStage(
              player: player,
              loadError: _loadError,
            ),
          ),
        ],
      ),
    );
  }
}

class _RuntimeStage extends StatelessWidget {
  const _RuntimeStage({
    required this.player,
    required this.loadError,
  });

  final KinetoPlayer? player;
  final String? loadError;

  @override
  Widget build(BuildContext context) {
    if (loadError != null) {
      return Center(
        child: Text(
          'Unable to load sample',
          style: Theme.of(context).textTheme.titleMedium?.copyWith(
                color: Colors.white,
              ),
        ),
      );
    }

    final player = this.player;
    if (player == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return KinetoPlayerView(
      player: player,
      onPointerEvent: (event, fired) {
        // The Android smoke script reads this from logcat.
        debugPrint(
          'kineto_smoke pointer=${event.name} fired=$fired '
          'sounds=${player.takeFiredAudioEvents()}',
        );
      },
    );
  }
}

class _DiagnosticsPanel extends StatelessWidget {
  const _DiagnosticsPanel({
    required this.diagnostics,
    required this.loadError,
  });

  final KinetoRuntimeDiagnostics? diagnostics;
  final String? loadError;

  @override
  Widget build(BuildContext context) {
    final textStyle = Theme.of(context).textTheme.bodySmall?.copyWith(
          color: Colors.white.withValues(alpha: 0.72),
        );
    final diagnostics = this.diagnostics;

    return DecoratedBox(
      decoration: const BoxDecoration(color: Color(0xFF18202A)),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: DefaultTextStyle(
          style: textStyle ?? const TextStyle(color: Colors.white70),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text('Asset: ${diagnostics?.assetName ?? 'loading'}'),
              Text('Platform: ${diagnostics?.platform ?? 'detecting'}'),
              Text('Renderer: ${diagnostics?.renderer ?? 'loading runtime'}'),
              Text(
                'Native handle: '
                '${_hexAddress(diagnostics?.nativeHandleAddress)}',
              ),
              if (loadError != null) Text('Error: $loadError'),
            ],
          ),
        ),
      ),
    );
  }
}

/// Android tags heap pointers in their top byte, so an address can read as a
/// negative Dart int. BigInt, because `int.toUnsigned(64)` leaves it unchanged.
String _hexAddress(int? address) => address == null
    ? '-'
    : BigInt.from(address).toUnsigned(64).toRadixString(16);
