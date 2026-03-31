import CoreMediaIO
import CoreVideo
import Foundation
import IOKit.audio
import IOSurface

/// Virtual camera device that owns the pixel buffer pool and frame timer.
/// Reads composited frames from a shared IOSurface published by Lodestone,
/// falling back to black frames when Lodestone isn't running.
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

    // IOSurface shared from the main Lodestone app
    private static let appGroupID = "group.com.kdougan.lodestone.app"
    private static let surfaceIDKey = "virtualCameraSurfaceID"
    private var sharedSurface: IOSurface?
    private var currentSurfaceID: IOSurfaceID = 0

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
            sharedSurface = nil
            currentSurfaceID = 0
        }
    }

    // MARK: - Frame Production

    private func onTimerTick() {
        // Check if Lodestone published a new IOSurface
        lookupSharedSurface()

        var err: OSStatus = 0

        if let surface = sharedSurface {
            // Wrap the shared IOSurface directly into a CVPixelBuffer (zero-copy)
            var pb: Unmanaged<CVPixelBuffer>?
            err = CVPixelBufferCreateWithIOSurface(
                kCFAllocatorDefault,
                surface,
                [
                    kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA,
                    kCVPixelBufferWidthKey: frameWidth,
                    kCVPixelBufferHeightKey: frameHeight,
                ] as CFDictionary,
                &pb
            )
            if err == 0, let pb = pb {
                deliverPixelBuffer(pb.takeRetainedValue())
                return
            }
        }

        // Fallback: deliver a black frame from the pool
        var pixelBuffer: CVPixelBuffer?
        err = CVPixelBufferPoolCreatePixelBufferWithAuxAttributes(
            kCFAllocatorDefault, bufferPool, bufferAuxAttributes, &pixelBuffer)
        guard err == 0, let pixelBuffer else { return }

        CVPixelBufferLockBaseAddress(pixelBuffer, [])
        let bufferPtr = CVPixelBufferGetBaseAddress(pixelBuffer)!
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let rowBytes = CVPixelBufferGetBytesPerRow(pixelBuffer)
        memset(bufferPtr, 0, rowBytes * height)
        CVPixelBufferUnlockBaseAddress(pixelBuffer, [])

        deliverPixelBuffer(pixelBuffer)
    }

    private func deliverPixelBuffer(_ pixelBuffer: CVPixelBuffer) {
        var sbuf: CMSampleBuffer!
        var timingInfo = CMSampleTimingInfo()
        timingInfo.presentationTimeStamp = CMClockGetTime(CMClockGetHostTimeClock())
        let err = CMSampleBufferCreateForImageBuffer(
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

    // MARK: - IOSurface Lookup

    /// Read the IOSurface ID from UserDefaults and look up the shared surface.
    private func lookupSharedSurface() {
        let defaults = UserDefaults(suiteName: LodestoneDevice.appGroupID)
        let storedID = UInt32(defaults?.integer(forKey: LodestoneDevice.surfaceIDKey) ?? 0)

        guard storedID != 0 else {
            sharedSurface = nil
            currentSurfaceID = 0
            return
        }

        if storedID != currentSurfaceID {
            sharedSurface = IOSurfaceLookup(storedID)
            currentSurfaceID = storedID
        }
    }
}
