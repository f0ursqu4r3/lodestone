import CoreMediaIO
import Foundation

/// Virtual camera device that exposes a single output stream.
class LodestoneDevice: NSObject, CMIOExtensionDeviceSource {

    let device: CMIOExtensionDevice
    private let _stream: LodestoneStream

    init(provider: CMIOExtensionProvider) {
        let deviceID = UUID()
        _stream = LodestoneStream()

        device = CMIOExtensionDevice(
            localizedName: "Lodestone Virtual Camera",
            deviceID: deviceID,
            legacyDeviceID: nil,
            source: nil
        )

        super.init()

        device.source = self

        do {
            try device.addStream(_stream.stream)
        } catch {
            fatalError("Failed to add stream to device: \(error)")
        }
    }

    // MARK: - CMIOExtensionDeviceSource

    var availableProperties: Set<CMIOExtensionProperty> {
        return [.deviceModel, .deviceTransportType]
    }

    func deviceProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionDeviceProperties
    {
        let deviceProperties = CMIOExtensionDeviceProperties(dictionary: [:])
        if properties.contains(.deviceModel) {
            deviceProperties.model = "Lodestone Virtual Camera"
        }
        if properties.contains(.deviceTransportType) {
            deviceProperties.transportType = kIOAudioDeviceTransportTypeVirtual
        }
        return deviceProperties
    }

    func setDeviceProperties(_ deviceProperties: CMIOExtensionDeviceProperties) throws {
        // No settable properties
    }
}
