import CoreMediaIO
import CoreVideo
import Foundation
import IOKit.audio

/// Virtual camera device that owns the pixel buffer pool and frame timer.
class LodestoneDevice: NSObject, CMIOExtensionDeviceSource {

    private(set) var device: CMIOExtensionDevice!
    private(set) var streamSource: LodestoneStream!

    private var streamingCounter: UInt32 = 0
    private var timer: DispatchSourceTimer?
    private let timerQueue = DispatchQueue(label: "timerQueue", qos: .userInteractive, attributes: [], autoreleaseFrequency: .workItem, target: .global(qos: .userInteractive))

    private var videoDescription: CMFormatDescription!
    private var bufferPool: CVPixelBufferPool!
    private var bufferAuxAttributes: NSDictionary!

    private let frameWidth: Int32 = 1920
    private let frameHeight: Int32 = 1080
    private let frameRate: Int = 30

    override init() {
        super.init()

        device = CMIOExtensionDevice(
            localizedName: "Lodestone Virtual Camera",
            deviceID: UUID(),
            legacyDeviceID: nil,
            source: self
        )

        let dims = CMVideoDimensions(width: frameWidth, height: frameHeight)
        CMVideoFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            codecType: kCVPixelFormatType_32BGRA,
            width: dims.width,
            height: dims.height,
            extensions: nil,
            formatDescriptionOut: &videoDescription
        )

        let pixelBufferAttributes: NSDictionary = [
            kCVPixelBufferWidthKey: dims.width,
            kCVPixelBufferHeightKey: dims.height,
            kCVPixelBufferPixelFormatTypeKey: videoDescription.mediaSubType,
            kCVPixelBufferIOSurfacePropertiesKey: [:] as NSDictionary,
        ]
        CVPixelBufferPoolCreate(kCFAllocatorDefault, nil, pixelBufferAttributes, &bufferPool)

        let streamFormat = CMIOExtensionStreamFormat(
            formatDescription: videoDescription,
            maxFrameDuration: CMTime(value: 1, timescale: Int32(frameRate)),
            minFrameDuration: CMTime(value: 1, timescale: Int32(frameRate)),
            validFrameDurations: nil
        )
        bufferAuxAttributes = [kCVPixelBufferPoolAllocationThresholdKey: 5]

        streamSource = LodestoneStream(
            localizedName: "Lodestone Virtual Camera",
            streamID: UUID(),
            streamFormat: streamFormat,
            device: device
        )

        do {
            try device.addStream(streamSource.stream)
        } catch {
            fatalError("Failed to add stream: \(error)")
        }
    }

    // MARK: - CMIOExtensionDeviceSource

    var availableProperties: Set<CMIOExtensionProperty> {
        return [.deviceTransportType, .deviceModel]
    }

    func deviceProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionDeviceProperties
    {
        let deviceProperties = CMIOExtensionDeviceProperties(dictionary: [:])
        if properties.contains(.deviceTransportType) {
            deviceProperties.transportType = kIOAudioDeviceTransportTypeVirtual
        }
        if properties.contains(.deviceModel) {
            deviceProperties.model = "Lodestone Virtual Camera"
        }
        return deviceProperties
    }

    func setDeviceProperties(_ deviceProperties: CMIOExtensionDeviceProperties) throws {
    }

    // MARK: - Streaming

    func startStreaming() {
        guard bufferPool != nil else { return }

        streamingCounter += 1

        timer = DispatchSource.makeTimerSource(flags: .strict, queue: timerQueue)
        timer!.schedule(deadline: .now(), repeating: 1.0 / Double(frameRate), leeway: .seconds(0))

        timer!.setEventHandler { [weak self] in
            self?.onTimerTick()
        }

        timer!.resume()
    }

    func stopStreaming() {
        if streamingCounter > 1 {
            streamingCounter -= 1
        } else {
            streamingCounter = 0
            timer?.cancel()
            timer = nil
        }
    }

    private func onTimerTick() {
        var err: OSStatus = 0

        var pixelBuffer: CVPixelBuffer?
        err = CVPixelBufferPoolCreatePixelBufferWithAuxAttributes(
            kCFAllocatorDefault, bufferPool, bufferAuxAttributes, &pixelBuffer)
        guard err == 0, let pixelBuffer else { return }

        CVPixelBufferLockBaseAddress(pixelBuffer, [])
        let bufferPtr = CVPixelBufferGetBaseAddress(pixelBuffer)!
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let rowBytes = CVPixelBufferGetBytesPerRow(pixelBuffer)
        // Black frame
        memset(bufferPtr, 0, rowBytes * height)
        CVPixelBufferUnlockBaseAddress(pixelBuffer, [])

        var sbuf: CMSampleBuffer!
        var timingInfo = CMSampleTimingInfo()
        timingInfo.presentationTimeStamp = CMClockGetTime(CMClockGetHostTimeClock())
        err = CMSampleBufferCreateForImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            dataReady: true,
            makeDataReadyCallback: nil,
            refcon: nil,
            formatDescription: videoDescription,
            sampleTiming: &timingInfo,
            sampleBufferOut: &sbuf
        )
        if err == 0 {
            streamSource.stream.send(
                sbuf,
                discontinuity: [],
                hostTimeInNanoseconds: UInt64(timingInfo.presentationTimeStamp.seconds * Double(NSEC_PER_SEC))
            )
        }
    }
}
