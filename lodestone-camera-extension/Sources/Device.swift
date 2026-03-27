import CoreMediaIO
import Foundation

/// Virtual camera device that exposes a single output stream.
class LodestoneDevice: NSObject, CMIOExtensionDeviceSource {

    private(set) var device: CMIOExtensionDevice!
    private var _stream: LodestoneStream!

    override init() {
        super.init()

        _stream = LodestoneStream()

        device = CMIOExtensionDevice(
            localizedName: "Lodestone Virtual Camera",
            deviceID: UUID(),
            legacyDeviceID: nil,
            source: self
        )

        do {
            try device.addStream(_stream.stream)
        } catch {
            fatalError("Failed to add stream to device: \(error)")
        }
    }

    // MARK: - CMIOExtensionDeviceSource

    var availableProperties: Set<CMIOExtensionProperty> {
        return [.deviceModel]
    }

    func deviceProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionDeviceProperties
    {
        let deviceProperties = CMIOExtensionDeviceProperties(dictionary: [:])
        if properties.contains(.deviceModel) {
            deviceProperties.model = "Lodestone Virtual Camera"
        }
        return deviceProperties
    }

    func setDeviceProperties(_ deviceProperties: CMIOExtensionDeviceProperties) throws {
        // No settable properties
    }
}
