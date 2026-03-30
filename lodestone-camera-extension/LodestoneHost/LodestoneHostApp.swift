import SwiftUI
import SystemExtensions
import os.log

private let logger = Logger(subsystem: "com.kdougan.lodestone.app", category: "HostApp")

class ExtensionActivator: NSObject, OSSystemExtensionRequestDelegate {
    static let shared = ExtensionActivator()

    func activate() {
        logger.info("Requesting extension activation...")
        let request = OSSystemExtensionRequest.activationRequest(
            forExtensionWithIdentifier: "com.kdougan.lodestone.app.camera-extension",
            queue: .main
        )
        request.delegate = self
        OSSystemExtensionManager.shared.submitRequest(request)
    }

    func request(_ request: OSSystemExtensionRequest,
                 actionForReplacingExtension existing: OSSystemExtensionProperties,
                 withExtension ext: OSSystemExtensionProperties) -> OSSystemExtensionRequest.ReplacementAction {
        logger.info("Replacing existing extension")
        return .replace
    }

    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) {
        logger.info("Extension needs user approval — check System Settings")
    }

    func request(_ request: OSSystemExtensionRequest,
                 didFinishWithResult result: OSSystemExtensionRequest.Result) {
        switch result {
        case .completed:
            logger.info("Extension activated successfully!")
        case .willCompleteAfterReboot:
            logger.info("Extension will activate after reboot")
        @unknown default:
            logger.info("Extension finished with result: \(String(describing: result))")
        }
    }

    func request(_ request: OSSystemExtensionRequest, didFailWithError error: Error) {
        logger.error("Extension activation failed: \(error.localizedDescription)")
    }
}

@main
struct LodestoneHostApp: App {
    init() {
        ExtensionActivator.shared.activate()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
