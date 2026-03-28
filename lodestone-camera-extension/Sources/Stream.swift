import CoreMediaIO
import CoreVideo
import Foundation
import IOSurface

/// The core stream source that reads composited frames from a shared IOSurface
/// and delivers them to consuming applications (Zoom, Discord, etc.).
class LodestoneStream: NSObject, CMIOExtensionStreamSource {

    private(set) var stream: CMIOExtensionStream!

    private static let appGroupID = "group.com.kdougan.lodestone.app"
    private static let surfaceIDKey = "virtualCameraSurfaceID"
    private static let fpsKey = "virtualCameraFPS"
    private static let widthKey = "virtualCameraWidth"
    private static let heightKey = "virtualCameraHeight"

    private var timer: DispatchSourceTimer?
    private var surface: IOSurface?
    private var currentSurfaceID: IOSurfaceID = 0
    private var sequenceNumber: UInt64 = 0

    private let frameWidth: Int32
    private let frameHeight: Int32
    private let frameRate: Double

    private var formatDescription: CMFormatDescription?

    override init() {
        let defaults = UserDefaults(suiteName: LodestoneStream.appGroupID)
        frameWidth = Int32(defaults?.integer(forKey: LodestoneStream.widthKey) ?? 1920)
        frameHeight = Int32(defaults?.integer(forKey: LodestoneStream.heightKey) ?? 1080)
        frameRate = Double(defaults?.integer(forKey: LodestoneStream.fpsKey) ?? 30)

        // Create format description for BGRA
        var fmtDesc: CMFormatDescription?
        CMVideoFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            codecType: kCVPixelFormatType_32BGRA,
            width: frameWidth,
            height: frameHeight,
            extensions: nil,
            formatDescriptionOut: &fmtDesc
        )

        formatDescription = fmtDesc

        super.init()

        stream = CMIOExtensionStream(
            localizedName: "Lodestone Virtual Camera",
            streamID: UUID(),
            direction: .source,
            clockType: .hostTime,
            source: self
        )
    }

    // MARK: - CMIOExtensionStreamSource

    var availableProperties: Set<CMIOExtensionProperty> {
        return [
            .streamActiveFormatIndex,
            .streamFrameDuration,
        ]
    }

    func streamProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionStreamProperties
    {
        let streamProperties = CMIOExtensionStreamProperties(dictionary: [:])
        if properties.contains(.streamActiveFormatIndex) {
            streamProperties.activeFormatIndex = 0
        }
        if properties.contains(.streamFrameDuration) {
            streamProperties.frameDuration = CMTime(
                value: 1, timescale: CMTimeScale(frameRate))
        }
        return streamProperties
    }

    func setStreamProperties(_ streamProperties: CMIOExtensionStreamProperties) throws {
        // No settable properties
    }

    func authorizedToStartStream(for client: CMIOExtensionClient) -> Bool {
        return true
    }

    func startStream() throws {
        lookupSurface()
        startTimer()
    }

    func stopStream() throws {
        timer?.cancel()
        timer = nil
        surface = nil
        currentSurfaceID = 0
    }

    var formats: [CMIOExtensionStreamFormat] {
        guard let fmtDesc = formatDescription else { return [] }
        return [
            CMIOExtensionStreamFormat(
                formatDescription: fmtDesc,
                maxFrameDuration: CMTime(value: 1, timescale: CMTimeScale(frameRate)),
                minFrameDuration: CMTime(value: 1, timescale: CMTimeScale(frameRate)),
                validFrameDurations: nil
            )
        ]
    }

    // MARK: - Private

    /// Read the IOSurface ID from UserDefaults and look up the surface.
    private func lookupSurface() {
        let defaults = UserDefaults(suiteName: LodestoneStream.appGroupID)
        let storedID = UInt32(defaults?.integer(forKey: LodestoneStream.surfaceIDKey) ?? 0)

        guard storedID != 0 else {
            surface = nil
            currentSurfaceID = 0
            return
        }

        if storedID != currentSurfaceID {
            surface = IOSurfaceLookup(storedID)
            currentSurfaceID = storedID
        }
    }

    /// Start a timer that fires at the configured frame rate.
    private func startTimer() {
        let interval = 1.0 / frameRate
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .userInteractive))
        timer.schedule(deadline: .now(), repeating: interval)
        timer.setEventHandler { [weak self] in
            self?.onTimerTick()
        }
        timer.resume()
        self.timer = timer
    }

    /// Called each frame. Reads from the IOSurface or delivers a black frame.
    private func onTimerTick() {
        // Check if the surface ID changed (Lodestone may have restarted)
        lookupSurface()

        let pixelBuffer: CVPixelBuffer

        if let surface = surface {
            // Wrap the IOSurface directly into a CVPixelBuffer (zero-copy)
            var pb: Unmanaged<CVPixelBuffer>?
            let status = CVPixelBufferCreateWithIOSurface(
                kCFAllocatorDefault,
                surface,
                [
                    kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA,
                    kCVPixelBufferWidthKey: frameWidth,
                    kCVPixelBufferHeightKey: frameHeight,
                ] as CFDictionary,
                &pb
            )

            if status == kCVReturnSuccess, let pb = pb {
                pixelBuffer = pb.takeRetainedValue()
            } else {
                pixelBuffer = createBlackFrame()
            }
        } else {
            pixelBuffer = createBlackFrame()
        }

        deliverFrame(pixelBuffer: pixelBuffer)
    }

    /// Create a black CVPixelBuffer (used when Lodestone isn't running).
    private func createBlackFrame() -> CVPixelBuffer {
        var pb: CVPixelBuffer?
        CVPixelBufferCreate(
            kCFAllocatorDefault,
            Int(frameWidth),
            Int(frameHeight),
            kCVPixelFormatType_32BGRA,
            [
                kCVPixelBufferIOSurfacePropertiesKey: [:] as CFDictionary
            ] as CFDictionary,
            &pb
        )

        guard let pixelBuffer = pb else {
            fatalError("Failed to create black pixel buffer")
        }

        CVPixelBufferLockBaseAddress(pixelBuffer, [])
        if let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) {
            let size = CVPixelBufferGetDataSize(pixelBuffer)
            memset(baseAddress, 0, size)
        }
        CVPixelBufferUnlockBaseAddress(pixelBuffer, [])

        return pixelBuffer
    }

    /// Wrap the pixel buffer in a CMSampleBuffer and send it to the stream.
    private func deliverFrame(pixelBuffer: CVPixelBuffer) {
        var fmtDesc: CMFormatDescription?
        CMVideoFormatDescriptionCreateForImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescriptionOut: &fmtDesc
        )

        guard let formatDescription = fmtDesc else { return }

        let now = CMClockGetTime(CMClockGetHostTimeClock())
        let duration = CMTime(value: 1, timescale: CMTimeScale(frameRate))

        var timingInfo = CMSampleTimingInfo(
            duration: duration,
            presentationTimeStamp: now,
            decodeTimeStamp: .invalid
        )

        var sampleBuffer: CMSampleBuffer?
        CMSampleBufferCreateReadyWithImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescription: formatDescription,
            sampleTiming: &timingInfo,
            sampleBufferOut: &sampleBuffer
        )

        guard let buffer = sampleBuffer else { return }

        do {
            try stream.send(
                buffer,
                discontinuity: [],
                hostTimeInNanoseconds: UInt64(now.seconds * Double(NSEC_PER_SEC))
            )
        } catch {
            // Frame delivery failed — likely no active consumers
        }

        sequenceNumber += 1
    }
}
