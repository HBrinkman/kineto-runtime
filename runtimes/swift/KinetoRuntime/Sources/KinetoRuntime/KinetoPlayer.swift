import Foundation
import KinetoRuntimeC
import Metal
import QuartzCore
import UIKit

public enum KinetoRuntimeError: Error, Equatable, CustomStringConvertible {
    case loadFailed(KinetoStatus, String)
    case operationFailed(KinetoStatus, String)
    case invalidSurface

    public var description: String {
        switch self {
        case let .loadFailed(status, message):
            "Kineto load failed (\(status.rawValue)): \(message)"
        case let .operationFailed(status, message):
            "Kineto operation failed (\(status.rawValue)): \(message)"
        case .invalidSurface:
            "Kineto surface is not available"
        }
    }
}

public final class KinetoPlayer {
    private var handle: OpaquePointer?

    public convenience init(contentsOf url: URL) throws {
        try self.init(data: Data(contentsOf: url))
    }

    public init(data: Data) throws {
        var status = KinetoStatus_Ok
        let loaded = data.withUnsafeBytes { rawBuffer in
            kineto_load(
                rawBuffer.bindMemory(to: UInt8.self).baseAddress,
                UInt(rawBuffer.count),
                &status
            )
        }
        guard let loaded, isOk(status) else {
            throw KinetoRuntimeError.loadFailed(status, "Unable to load .kvk data")
        }
        handle = loaded
    }

    deinit {
        if let handle {
            kineto_destroy(handle)
        }
    }

    public func advance(_ deltaTime: TimeInterval) throws {
        try withHandle { handle in
            try check(kineto_advance(handle, deltaTime))
        }
    }

    public func setBool(_ value: Bool, input inputName: String, in stateMachineId: String) throws {
        try withHandle { handle in
            try stateMachineId.withCString { stateMachine in
                try inputName.withCString { input in
                    try check(kineto_set_bool(handle, stateMachine, input, value))
                }
            }
        }
    }

    /// Sets a number input to a normalised `0...1` value — the pointer entry
    /// point. Values outside the range are clamped where they are read.
    public func setNumber(_ value: Double, input inputName: String, in stateMachineId: String) throws {
        try withHandle { handle in
            try stateMachineId.withCString { stateMachine in
                try inputName.withCString { input in
                    try check(kineto_set_number(handle, stateMachine, input, value))
                }
            }
        }
    }

    /// A pointer event at a document point. Whatever listens there drives its
    /// input; the host does not need to know which one (issue #74).
    /// Returns how many listeners fired.
    @discardableResult
    public func pointerEvent(_ event: PointerEvent, x: Double, y: Double) throws -> Int {
        try withHandle { handle in
            var fired: UInt32 = 0
            try check(kineto_pointer_event(handle, event.raw, x, y, &fired))
            return Int(fired)
        }
    }

    /// A pointer *move* — a drag rather than a tap.
    ///
    /// Fires no listeners: a move is continuous input, not a discrete event.
    /// It drives what reads the pointer continuously — following targets,
    /// extent-dragged bindings, pointer-sourced inputs. Without it a following
    /// target only updates on tap, so "look at my finger" means "look where I
    /// last tapped" (issue #111).
    public func pointerMoved(x: Double, y: Double) throws {
        try withHandle { handle in
            try check(kineto_pointer_moved(handle, x, y))
        }
    }

    /// A point on the drawable, in document space.
    ///
    /// The renderer fits the document into the drawable and draws it through
    /// the camera; hit testing happens in the document's own coordinates, so a
    /// touch has to come back through exactly that. The conversion lives in
    /// the engine so the two cannot drift.
    public func documentPoint(
        surfaceWidth: Double,
        surfaceHeight: Double,
        x: Double,
        y: Double
    ) throws -> (x: Double, y: Double) {
        try withHandle { handle in
            var outX = x
            var outY = y
            try check(
                kineto_document_point(
                    handle,
                    surfaceWidth,
                    surfaceHeight,
                    x,
                    y,
                    &outX,
                    &outY
                )
            )
            return (outX, outY)
        }
    }

    /// The artboard background the surface clears to, as straight (not
    /// premultiplied) RGBA bytes (issue #233). All zeros is transparent: no
    /// background, or a fully clear one — the same answer wasm and the vello
    /// surface give for the same file.
    public func backgroundColor() throws -> (r: UInt8, g: UInt8, b: UInt8, a: UInt8) {
        try withHandle { handle in
            var rgba: [UInt8] = [0, 0, 0, 0]
            try check(kineto_background_color(handle, &rgba))
            return (rgba[0], rgba[1], rgba[2], rgba[3])
        }
    }

    public enum PointerEvent {
        case down, up, enter, exit

        var raw: KinetoPointerEvent {
            switch self {
            case .down: return KinetoPointerEvent(rawValue: 0)
            case .up: return KinetoPointerEvent(rawValue: 1)
            case .enter: return KinetoPointerEvent(rawValue: 2)
            case .exit: return KinetoPointerEvent(rawValue: 3)
            }
        }
    }

    public func setTrigger(_ inputName: String, in stateMachineId: String) throws {
        try withHandle { handle in
            try stateMachineId.withCString { stateMachine in
                try inputName.withCString { input in
                    try check(kineto_set_trigger(handle, stateMachine, input))
                }
            }
        }
    }

    public func activeState(in stateMachineId: String) -> String? {
        guard let handle else {
            return nil
        }
        return copyStateMachineString(stateMachineId, handle: handle, kineto_get_active_state)
    }

    public func activeTransition(in stateMachineId: String) -> String? {
        guard let handle else {
            return nil
        }
        return copyStateMachineString(stateMachineId, handle: handle, kineto_get_active_transition)
    }

    public func transitionProgress(in stateMachineId: String) throws -> Double {
        try withHandle { handle in
            try stateMachineId.withCString { stateMachine in
                var status = KinetoStatus_Ok
                let progress = kineto_get_transition_progress(handle, stateMachine, &status)
                try check(status)
                return progress
            }
        }
    }

    /// One state machine in the document (issue #111 introspection).
    public struct MachineInfo: Equatable, Identifiable, Sendable {
        public let id: String
        public let name: String
    }

    /// One input a machine exposes.
    public struct InputInfo: Equatable, Identifiable, Sendable {
        public enum Kind: UInt8, Sendable {
            case bool = 0
            case trigger = 1
            case number = 2
        }

        public let id: String
        public let name: String
        public let kind: Kind
        public let defaultBool: Bool
        public let defaultNumber: Double
    }

    /// One animation clip.
    public struct ClipInfo: Equatable, Identifiable, Sendable {
        public let id: String
        public let name: String
        public let duration: Double
    }

    /// The document's state machines, in document order.
    public func stateMachines() -> [MachineInfo] {
        guard let handle, let joined = kineto_state_machines(handle) else {
            return []
        }
        return Self.records(String(cString: joined), fields: 2).map {
            MachineInfo(id: $0[0], name: $0[1])
        }
    }

    /// One machine's inputs. Empty when nothing carries `machineID`.
    public func inputs(in machineID: String) -> [InputInfo] {
        guard let handle else {
            return []
        }
        let joined = machineID.withCString { kineto_machine_inputs(handle, $0) }
        guard let joined else {
            return []
        }
        return Self.records(String(cString: joined), fields: 5).compactMap { fields in
            guard
                let raw = UInt8(fields[2]),
                let kind = InputInfo.Kind(rawValue: raw)
            else {
                // A kind this build does not know is skipped rather than
                // guessed at: driving an input by the wrong rule is worse
                // than not offering it.
                return nil
            }
            return InputInfo(
                id: fields[0],
                name: fields[1],
                kind: kind,
                defaultBool: fields[3] == "1",
                defaultNumber: Double(fields[4]) ?? 0
            )
        }
    }

    /// The document's animation clips.
    public func clips() -> [ClipInfo] {
        guard let handle, let joined = kineto_animations(handle) else {
            return []
        }
        return Self.records(String(cString: joined), fields: 3).map {
            ClipInfo(id: $0[0], name: $0[1], duration: Double($0[2]) ?? 0)
        }
    }

    /// Splits the runtime's newline/tab record format, dropping any row that
    /// does not carry the expected field count — a malformed row is a bug to
    /// notice elsewhere, not something to half-render.
    private static func records(_ joined: String, fields: Int) -> [[String]] {
        guard !joined.isEmpty else {
            return []
        }
        return joined.split(separator: "\n").compactMap { line in
            let parts = line.split(
                separator: "\t",
                maxSplits: fields - 1,
                omittingEmptySubsequences: false
            ).map(String.init)
            return parts.count == fields ? parts : nil
        }
    }

    /// Starts the active animation clip.
    ///
    /// The timeline only advances while the player is playing, so a clip's
    /// audio has no playhead to follow until this is called (issue #111).
    public func play() throws {
        try withHandle { handle in
            try check(kineto_play(handle))
        }
    }

    /// Pauses playback, leaving the playhead where it is.
    public func pause() throws {
        try withHandle { handle in
            try check(kineto_pause(handle))
        }
    }

    /// Makes `animationID` the active clip, rewound to its start.
    public func selectAnimation(_ animationID: String) throws {
        try withHandle { handle in
            try animationID.withCString { id in
                try check(kineto_select_animation(handle, id))
            }
        }
    }

    /// One audio file the document refers to (issue #111).
    ///
    /// A `.kvk` names its audio and carries none of it, so this is what a host
    /// asks the user for.
    public struct AudioAssetInfo: Equatable, Identifiable, Sendable {
        public let id: String
        /// The file name the asset was imported from.
        public let fileName: String
        public let duration: Double
    }

    /// Every audio file this document needs, timeline and machine alike.
    public func audioAssets() -> [AudioAssetInfo] {
        guard let handle, let joined = kineto_audio_assets(handle) else {
            return []
        }
        return Self.records(String(cString: joined), fields: 3).map { fields in
            AudioAssetInfo(
                id: fields[0],
                fileName: fields[1],
                duration: Double(fields[2]) ?? 0
            )
        }
    }

    /// One audio clip that should be sounding right now, on the timeline
    /// (issue #111).
    ///
    /// `sourcePosition` is the point *inside the file* that belongs at the
    /// current playhead, so a host can start there and stay in step.
    public struct ActiveAudioClip: Equatable, Identifiable, Sendable {
        public var id: String { trackID }
        public let trackID: String
        public let assetID: String
        /// The file the clip was imported from, which is how a host matches it
        /// against its own bundle.
        public let fileName: String
        public let sourcePosition: Double
        public let volume: Double
    }

    /// The timeline audio that should be sounding at the current playhead.
    ///
    /// Poll it after each `advance`. Empty means silence, which is also the
    /// answer while paused somewhere no clip covers.
    public func activeAudioClips() -> [ActiveAudioClip] {
        guard let handle, let joined = kineto_active_audio_clips(handle) else {
            return []
        }
        return Self.records(String(cString: joined), fields: 5).compactMap { fields in
            guard
                let position = Double(fields[3]),
                let volume = Double(fields[4])
            else {
                return nil
            }
            return ActiveAudioClip(
                trackID: fields[0],
                assetID: fields[1],
                fileName: fields[2],
                sourcePosition: position,
                volume: volume
            )
        }
    }

    /// One sound a state machine can fire (issue #111).
    ///
    /// A `.kvk` carries no audio bytes: it names the sound, and the app ships
    /// the file. `sourceName` is the file the sound was authored against, when
    /// the author recorded one.
    public struct AudioEvent: Equatable, Sendable {
        public let id: String
        public let name: String
        public let sourceName: String?
    }

    /// The sounds fired since the last call, in the order they fired.
    ///
    /// Call it after `advance` and after `pointerEvent`, and play whatever
    /// your app ships for each id. Draining, so a per-frame caller is never
    /// handed the same firing twice; never de-duplicated, because the same
    /// sound twice is two firings — which is what a double-tap is.
    public func takeFiredAudioEvents() -> [String] {
        guard let handle else {
            return []
        }
        var count: UInt32 = 0
        guard let joined = kineto_take_fired_audio_events(handle, &count), count > 0 else {
            return []
        }
        return String(cString: joined).split(separator: "\n").map(String.init)
    }

    /// Every sound the document names, so a firing can be mapped onto the
    /// app's own audio.
    public func audioEvents() -> [AudioEvent] {
        guard let handle, let joined = kineto_audio_events(handle) else {
            return []
        }
        let records = String(cString: joined)
        guard !records.isEmpty else {
            return []
        }
        // `id\tname\tsourceName`, with an empty third field when the sound
        // carries no breadcrumb.
        return Self.records(records, fields: 3).map { fields in
            AudioEvent(
                id: fields[0],
                name: fields[1],
                sourceName: fields[2].isEmpty ? nil : fields[2]
            )
        }
    }

    public func hitTestDocumentPoint(_ point: CGPoint) -> String? {
        guard let handle else {
            return nil
        }
        let required = kineto_hit_test_document_point(handle, Double(point.x), Double(point.y), nil, 0)
        guard required > 1 else {
            return nil
        }
        var buffer = [CChar](repeating: 0, count: Int(required))
        let copied = kineto_hit_test_document_point(
            handle,
            Double(point.x),
            Double(point.y),
            &buffer,
            UInt(buffer.count)
        )
        guard copied == required else {
            return nil
        }
        return String(cString: buffer)
    }

    fileprivate var rawHandle: OpaquePointer? {
        handle
    }

    private func withHandle<T>(_ body: (OpaquePointer) throws -> T) throws -> T {
        guard let handle else {
            throw KinetoRuntimeError.operationFailed(KinetoStatus_NullPointer, "Player has been destroyed")
        }
        return try body(handle)
    }

    private func check(_ status: KinetoStatus) throws {
        guard isOk(status) else {
            throw KinetoRuntimeError.operationFailed(status, lastErrorMessage())
        }
    }

    private func lastErrorMessage() -> String {
        guard let handle, let message = kineto_last_error_message(handle) else {
            return ""
        }
        return String(cString: message)
    }

    private func copyStateMachineString(
        _ stateMachineId: String,
        handle: OpaquePointer,
        _ copy: (OpaquePointer?, UnsafePointer<CChar>?, UnsafeMutablePointer<CChar>?, UInt) -> UInt
    ) -> String? {
        stateMachineId.withCString { stateMachine in
            let required = copy(handle, stateMachine, nil, 0)
            guard required > 1 else {
                return nil
            }
            var buffer = [CChar](repeating: 0, count: Int(required))
            let copied = copy(handle, stateMachine, &buffer, UInt(buffer.count))
            guard copied == required else {
                return nil
            }
            return String(cString: buffer)
        }
    }
}

public final class KinetoPlayerViewController: UIViewController {
    public let player: KinetoPlayer
    public var isPlaybackEnabled: Bool

    /// Plays the sounds the machine fires (issue #111).
    ///
    /// Optional, and nil by default: a `.kvk` carries no audio files, so a
    /// host that ships none has nothing to play. Set one and every frame
    /// pumps it — including the pointer events this controller does not
    /// deliver itself, as long as the host pumps after those too.
    public var audio: KinetoAudioDirector?

    private var renderSurface: UnsafeMutableRawPointer?
    private var displayLink: CADisplayLink?
    private let errorLabel = UILabel()
    private var lastSurfaceError: String?
    private var lastTimestamp: CFTimeInterval?

    public init(player: KinetoPlayer, autoplay: Bool = true) {
        self.player = player
        self.isPlaybackEnabled = autoplay
        super.init(nibName: nil, bundle: nil)
    }

    public required init?(coder: NSCoder) {
        nil
    }

    deinit {
        stopDisplayLink()
        if let renderSurface {
            kineto_surface_destroy(renderSurface)
        }
    }

    public override func loadView() {
        view = KinetoMetalView()
    }

    public override func viewDidLoad() {
        super.viewDidLoad()
        configureErrorLabel()
    }

    public override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        ensureSurface()
        startDisplayLink()
    }

    public override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        stopDisplayLink()
        // The machine stops advancing, so its sounds stop too — the same rule
        // the editor follows when you leave the Logic tab (issue #111).
        audio?.stopAll()
    }

    public override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        ensureSurface()
    }

    // MARK: - Touch

    /// Called with the ids of the listeners a touch fired, so a host can log
    /// or react. Optional: the machine drives itself either way.
    public var onPointerEvent: ((KinetoPlayer.PointerEvent, Int) -> Void)?

    public override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesBegan(touches, with: event)
        deliver(.down, touches)
    }

    public override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesMoved(touches, with: event)
        guard let touch = touches.first, let point = documentPoint(of: touch) else {
            return
        }
        // A drag drives, it does not fire: there is no Move listener.
        try? player.pointerMoved(x: point.x, y: point.y)
    }

    public override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesEnded(touches, with: event)
        deliver(.up, touches)
    }

    public override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        super.touchesCancelled(touches, with: event)
        // A cancelled touch is an "up" as far as the machine is concerned:
        // leaving a pointer-down listener latched would strand the state.
        deliver(.up, touches)
    }

    /// A touch, in the document's own coordinates.
    ///
    /// Two conversions, and the second one is easy to forget: view space to
    /// drawable pixels through the layer's contents scale, then drawable to
    /// document through the fit the renderer applies. That second step landed
    /// when the surface started fitting the document into the drawable
    /// (issue #111) — until then a drawable pixel *was* a document unit, and
    /// this multiplied by the scale and stopped. Every touch has been landing
    /// where the artwork is not ever since, which reads as "the target does
    /// not react".
    private func documentPoint(of touch: UITouch) -> (x: Double, y: Double)? {
        let point = touch.location(in: view)
        let scale = (view as? KinetoMetalView)?.metalLayer.contentsScale
            ?? view.window?.screen.scale
            ?? 1
        let drawable = (view as? KinetoMetalView)?.metalLayer.drawableSize
            ?? CGSize(
                width: view.bounds.width * scale,
                height: view.bounds.height * scale
            )
        return try? player.documentPoint(
            surfaceWidth: Double(drawable.width),
            surfaceHeight: Double(drawable.height),
            x: Double(point.x) * scale,
            y: Double(point.y) * scale
        )
    }

    /// Hands a touch to the runtime in document coordinates.
    private func deliver(_ event: KinetoPlayer.PointerEvent, _ touches: Set<UITouch>) {
        guard let touch = touches.first, let point = documentPoint(of: touch) else {
            return
        }
        do {
            let fired = try player.pointerEvent(event, x: point.x, y: point.y)
            // A listener may have fired a sound, and a firing outside a tick
            // reaches the host only if something asks.
            if let audio {
                audio.pump(player)
            }
            onPointerEvent?(event, fired)
        } catch {
            onPointerEvent?(event, 0)
        }
    }

    public func renderFrame(deltaTime: TimeInterval = 0) throws {
        if isPlaybackEnabled {
            try player.advance(deltaTime)
            // After the advance, so a sound is never heard for a transition
            // this frame did not actually take.
            if let audio {
                audio.pump(player)
                audio.collectFinished()
            }
        }
        guard let renderSurface, let handle = player.rawHandle else {
            throw KinetoRuntimeError.invalidSurface
        }
        try checkRenderStatus(kineto_render(handle, renderSurface))
    }

    private func ensureSurface() {
        guard let metalView = view as? KinetoMetalView else {
            return
        }
        let scale = metalView.window?.screen.scale ?? UIScreen.main.scale
        let width = max(1, UInt32((metalView.bounds.width * scale).rounded()))
        let height = max(1, UInt32((metalView.bounds.height * scale).rounded()))
        metalView.metalLayer.device = metalView.metalLayer.device ?? MTLCreateSystemDefaultDevice()
        metalView.metalLayer.pixelFormat = .bgra8Unorm
        metalView.metalLayer.framebufferOnly = false
        metalView.metalLayer.contentsScale = scale
        metalView.metalLayer.drawableSize = CGSize(width: CGFloat(width), height: CGFloat(height))

        if let renderSurface {
            _ = kineto_surface_resize(renderSurface, width, height)
            return
        }

        var status = KinetoStatus_Ok
        let layer = Unmanaged.passUnretained(metalView.metalLayer).toOpaque()
        renderSurface = kineto_surface_create_from_ca_metal_layer(layer, width, height, &status)
        if !isOk(status) {
            renderSurface = nil
            lastSurfaceError = surfaceLastErrorMessage()
            showRenderError(lastSurfaceError ?? "")
        } else {
            lastSurfaceError = nil
            clearRenderError()
        }
    }

    private func startDisplayLink() {
        guard displayLink == nil else {
            return
        }
        let displayLink = CADisplayLink(target: self, selector: #selector(displayLinkDidTick(_:)))
        displayLink.add(to: .main, forMode: .common)
        self.displayLink = displayLink
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
        lastTimestamp = nil
    }

    @objc private func displayLinkDidTick(_ link: CADisplayLink) {
        let delta = lastTimestamp.map { link.timestamp - $0 } ?? 0
        lastTimestamp = link.timestamp
        do {
            try renderFrame(deltaTime: delta)
            clearRenderError()
        } catch {
            if case KinetoRuntimeError.invalidSurface = error, let lastSurfaceError {
                showRenderError(lastSurfaceError)
            } else {
                showRenderError(String(describing: error))
            }
        }
    }

    private func checkRenderStatus(_ status: KinetoStatus) throws {
        guard isOk(status) else {
            let message = player.rawHandle.flatMap(kineto_last_error_message).map(String.init(cString:)) ?? ""
            throw KinetoRuntimeError.operationFailed(status, message)
        }
    }

    private func configureErrorLabel() {
        errorLabel.translatesAutoresizingMaskIntoConstraints = false
        errorLabel.numberOfLines = 0
        errorLabel.textAlignment = .center
        errorLabel.font = .preferredFont(forTextStyle: .footnote)
        errorLabel.textColor = .secondaryLabel
        errorLabel.backgroundColor = UIColor.systemBackground.withAlphaComponent(0.92)
        errorLabel.isHidden = true
        view.addSubview(errorLabel)
        NSLayoutConstraint.activate([
            errorLabel.leadingAnchor.constraint(equalTo: view.layoutMarginsGuide.leadingAnchor),
            errorLabel.trailingAnchor.constraint(equalTo: view.layoutMarginsGuide.trailingAnchor),
            errorLabel.centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
    }

    private func showRenderError(_ message: String) {
        errorLabel.text = "Kineto renderer unavailable\n\(message)"
        errorLabel.isHidden = false
    }

    private func clearRenderError() {
        errorLabel.isHidden = true
    }

    private func surfaceLastErrorMessage() -> String {
        guard let message = kineto_surface_last_error_message() else {
            return ""
        }
        return String(cString: message)
    }
}

private final class KinetoMetalView: UIView {
    override class var layerClass: AnyClass {
        CAMetalLayer.self
    }

    var metalLayer: CAMetalLayer {
        layer as! CAMetalLayer
    }
}

private func isOk(_ status: KinetoStatus) -> Bool {
    status.rawValue == KinetoStatus_Ok.rawValue
}
