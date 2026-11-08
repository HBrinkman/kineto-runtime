import Flutter
import KinetoRuntimeC
import Metal
import QuartzCore
import UIKit

public class KinetoRuntimePlugin: NSObject, FlutterPlugin {
    public static func register(with registrar: FlutterPluginRegistrar) {
        registrar.register(
            KinetoRuntimePlayerFactory(),
            withId: "kineto/runtime_player"
        )
    }
}

private final class KinetoRuntimePlayerFactory: NSObject, FlutterPlatformViewFactory {
    func create(
        withFrame frame: CGRect,
        viewIdentifier viewId: Int64,
        arguments args: Any?
    ) -> FlutterPlatformView {
        KinetoRuntimePlatformView(frame: frame, arguments: args)
    }

    func createArgsCodec() -> FlutterMessageCodec & NSObjectProtocol {
        FlutterStandardMessageCodec.sharedInstance()
    }
}

private final class KinetoRuntimePlatformView: NSObject, FlutterPlatformView {
    private let hostView: KinetoRuntimeMetalHostView

    init(frame: CGRect, arguments args: Any?) {
        let config = KinetoRuntimePlatformView.parseConfig(args)
        hostView = KinetoRuntimeMetalHostView(
            frame: frame,
            playerHandle: config.handle,
            autoplay: config.autoplay
        )
        super.init()
    }

    func view() -> UIView {
        hostView
    }

    private static func parseConfig(_ args: Any?) -> (handle: OpaquePointer?, autoplay: Bool) {
        guard let dictionary = args as? [String: Any] else {
            return (nil, true)
        }
        let handleAddress = (dictionary["handle"] as? NSNumber)?.uint64Value ?? 0
        let autoplay = (dictionary["autoplay"] as? NSNumber)?.boolValue ?? true
        guard handleAddress != 0 else {
            return (nil, autoplay)
        }
        return (OpaquePointer(bitPattern: UInt(handleAddress)), autoplay)
    }
}

private final class KinetoRuntimeMetalHostView: UIView {
    private let playerHandle: OpaquePointer?
    private let autoplay: Bool
    private let errorLabel = UILabel()
    private var renderSurface: UnsafeMutableRawPointer?
    private var displayLink: CADisplayLink?
    private var lastTimestamp: CFTimeInterval?
    private var lastLoggedFailure: String?

    init(frame: CGRect, playerHandle: OpaquePointer?, autoplay: Bool) {
        self.playerHandle = playerHandle
        self.autoplay = autoplay
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        errorLabel.translatesAutoresizingMaskIntoConstraints = false
        errorLabel.numberOfLines = 0
        errorLabel.textAlignment = .center
        errorLabel.textColor = UIColor(white: 1, alpha: 0.72)
        errorLabel.font = UIFont.preferredFont(forTextStyle: .footnote)
        errorLabel.isHidden = true
        addSubview(errorLabel)
        NSLayoutConstraint.activate([
            errorLabel.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor, constant: 24),
            errorLabel.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -24),
            errorLabel.centerXAnchor.constraint(equalTo: centerXAnchor),
            errorLabel.centerYAnchor.constraint(equalTo: centerYAnchor)
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    deinit {
        stopDisplayLink()
        if let renderSurface {
            kineto_surface_destroy(renderSurface)
        }
        // Dart retained the handle for this view (`native_view_io.dart`), so a
        // player disposed while the view is still drawing is not freed under it.
        if let playerHandle {
            kineto_destroy(playerHandle)
        }
    }

    override class var layerClass: AnyClass {
        CAMetalLayer.self
    }

    private var metalLayer: CAMetalLayer {
        layer as! CAMetalLayer
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil {
            stopDisplayLink()
        } else {
            ensureSurface()
            startDisplayLink()
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        ensureSurface()
    }

    private func ensureSurface() {
        guard let playerHandle else {
            return
        }
        let scale = window?.screen.scale ?? UIScreen.main.scale
        let width = max(1, UInt32((bounds.width * scale).rounded()))
        let height = max(1, UInt32((bounds.height * scale).rounded()))
        metalLayer.device = metalLayer.device ?? MTLCreateSystemDefaultDevice()
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = false
        metalLayer.contentsScale = scale
        metalLayer.drawableSize = CGSize(width: CGFloat(width), height: CGFloat(height))

        if let renderSurface {
            _ = kineto_surface_resize(renderSurface, width, height)
            return
        }

        var status = KinetoStatus_Ok
        let layerPointer = Unmanaged.passUnretained(metalLayer).toOpaque()
        renderSurface = kineto_surface_create_from_ca_metal_layer(
            layerPointer,
            width,
            height,
            &status
        )
        if status.rawValue != KinetoStatus_Ok.rawValue {
            logSurfaceFailureIfNeeded(status: status)
            renderSurface = nil
        } else {
            errorLabel.isHidden = true
        }
    }

    private func startDisplayLink() {
        guard displayLink == nil else {
            return
        }
        let link = CADisplayLink(target: self, selector: #selector(displayLinkDidTick(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
        lastTimestamp = nil
    }

    @objc private func displayLinkDidTick(_ link: CADisplayLink) {
        guard let playerHandle else {
            return
        }
        ensureSurface()
        guard let renderSurface else {
            return
        }

        let delta = lastTimestamp.map { link.timestamp - $0 } ?? 0
        lastTimestamp = link.timestamp
        if autoplay {
            logFailureIfNeeded(
                operation: "advance",
                status: kineto_advance(playerHandle, delta),
                playerHandle: playerHandle
            )
        }
        logFailureIfNeeded(
            operation: "render",
            status: kineto_render(playerHandle, renderSurface),
            playerHandle: playerHandle
        )
    }

    private func logFailureIfNeeded(
        operation: String,
        status: KinetoStatus,
        playerHandle: OpaquePointer
    ) {
        guard status.rawValue != KinetoStatus_Ok.rawValue else {
            return
        }
        let message = kineto_last_error_message(playerHandle).map(String.init(cString:)) ?? ""
        showError("Kineto runtime \(operation) failed")
        let failure = "\(operation):\(status.rawValue):\(message)"
        guard failure != lastLoggedFailure else {
            return
        }
        lastLoggedFailure = failure
        NSLog("KinetoRuntime \(operation) failed (\(status.rawValue)): \(message)")
    }

    private func logSurfaceFailureIfNeeded(status: KinetoStatus) {
        guard status.rawValue != KinetoStatus_Ok.rawValue else {
            return
        }
        let message = kineto_surface_last_error_message().map(String.init(cString:)) ?? ""
        showError("Kineto renderer unavailable\n\(message)")
        let failure = "surface_create:\(status.rawValue):\(message)"
        guard failure != lastLoggedFailure else {
            return
        }
        lastLoggedFailure = failure
        NSLog("KinetoRuntime surface_create failed (\(status.rawValue)): \(message)")
    }

    private func showError(_ message: String) {
        errorLabel.text = message
        errorLabel.isHidden = false
    }
}
