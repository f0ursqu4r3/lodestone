/// CoreMediaIO DAL (Device Abstraction Layer) plugin for Lodestone Virtual Camera.
///
/// Presents a single virtual camera device that reads composited BGRA frames
/// from a shared IOSurface published by the Lodestone app via NSUserDefaults.
/// Compatible with macOS 12+ (pre-CMIOExtension API).

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"

#import <CoreMediaIO/CMIOHardwarePlugIn.h>
#import <CoreMediaIO/CMIOHardwareObject.h>
#import <CoreMediaIO/CMIOHardwareDevice.h>
#import <CoreMediaIO/CMIOHardwareStream.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <IOSurface/IOSurface.h>
#import <Foundation/Foundation.h>

// ---------------------------------------------------------------------------
// MARK: - Constants
// ---------------------------------------------------------------------------

static NSString *const kAppGroupSuite = @"group.com.kdougan.lodestone.app";
static NSString *const kSurfaceIDKey  = @"virtualCameraSurfaceID";
static NSString *const kWidthKey      = @"virtualCameraWidth";
static NSString *const kHeightKey     = @"virtualCameraHeight";

static const Float64 kFrameRate = 30.0;
static const int32_t kDefaultWidth  = 1920;
static const int32_t kDefaultHeight = 1080;
static const uint32_t kMaxFrames = 8;

// ---------------------------------------------------------------------------
// MARK: - Global state
// ---------------------------------------------------------------------------

static CMIOHardwarePlugInRef  gPlugInRef    = NULL;
static CMIOObjectID           gPlugInID     = kCMIOObjectUnknown;
static CMIOObjectID           gDeviceID     = kCMIOObjectUnknown;
static CMIOObjectID           gStreamID     = kCMIOObjectUnknown;

static CMSimpleQueueRef       gQueue        = NULL;
static dispatch_source_t      gTimer        = NULL;

static CMIODeviceStreamQueueAlteredProc gAlteredProc = NULL;
static void *gAlteredRefCon = NULL;

static BOOL                   gStreamRunning = NO;
static uint64_t               gSequenceNumber = 0;

// Forward-declare the vtable.
static CMIOHardwarePlugInInterface gPlugInVtable;

// ---------------------------------------------------------------------------
// MARK: - Helpers
// ---------------------------------------------------------------------------

/// Read current width/height from UserDefaults (or use defaults).
static void GetConfiguredDimensions(int32_t *outWidth, int32_t *outHeight) {
    NSUserDefaults *defaults = [[NSUserDefaults alloc] initWithSuiteName:kAppGroupSuite];
    NSInteger w = [defaults integerForKey:kWidthKey];
    NSInteger h = [defaults integerForKey:kHeightKey];
    *outWidth  = (w > 0) ? (int32_t)w : kDefaultWidth;
    *outHeight = (h > 0) ? (int32_t)h : kDefaultHeight;
}

/// Create a black CVPixelBuffer.
static CVPixelBufferRef CreateBlackFrame(int32_t width, int32_t height) {
    NSDictionary *attrs = @{
        (__bridge NSString *)kCVPixelBufferIOSurfacePropertiesKey : @{}
    };
    CVPixelBufferRef pb = NULL;
    CVPixelBufferCreate(kCFAllocatorDefault, width, height,
                        kCVPixelFormatType_32BGRA,
                        (__bridge CFDictionaryRef)attrs, &pb);
    if (pb) {
        CVPixelBufferLockBaseAddress(pb, 0);
        void *base = CVPixelBufferGetBaseAddress(pb);
        if (base) {
            size_t size = CVPixelBufferGetDataSize(pb);
            memset(base, 0, size);
        }
        CVPixelBufferUnlockBaseAddress(pb, 0);
    }
    return pb;
}

/// Build a CMSampleBuffer from a CVPixelBuffer with timing info.
static CMSampleBufferRef CreateSampleBuffer(CVPixelBufferRef pixelBuffer) {
    CMFormatDescriptionRef fmtDesc = NULL;
    CMVideoFormatDescriptionCreateForImageBuffer(kCFAllocatorDefault, pixelBuffer, &fmtDesc);
    if (!fmtDesc) return NULL;

    CMTime now = CMClockGetTime(CMClockGetHostTimeClock());
    CMTime duration = CMTimeMake(1, (int32_t)kFrameRate);

    CMSampleTimingInfo timing;
    timing.duration              = duration;
    timing.presentationTimeStamp = now;
    timing.decodeTimeStamp       = kCMTimeInvalid;

    CMSampleBufferRef sampleBuffer = NULL;
    CMSampleBufferCreateReadyWithImageBuffer(kCFAllocatorDefault,
                                             pixelBuffer,
                                             fmtDesc,
                                             &timing,
                                             &sampleBuffer);
    CFRelease(fmtDesc);
    return sampleBuffer;
}

// ---------------------------------------------------------------------------
// MARK: - Frame timer
// ---------------------------------------------------------------------------

static void StopTimer(void) {
    if (gTimer) {
        dispatch_source_cancel(gTimer);
        gTimer = NULL;
    }
    gStreamRunning = NO;
}

static void TimerTick(void) {
    if (!gQueue) return;

    int32_t width, height;
    GetConfiguredDimensions(&width, &height);

    NSUserDefaults *defaults = [[NSUserDefaults alloc] initWithSuiteName:kAppGroupSuite];
    uint32_t surfaceID = (uint32_t)[defaults integerForKey:kSurfaceIDKey];

    CVPixelBufferRef pixelBuffer = NULL;

    if (surfaceID != 0) {
        IOSurfaceRef surface = IOSurfaceLookup(surfaceID);
        if (surface) {
            NSDictionary *attrs = @{
                (__bridge NSString *)kCVPixelBufferPixelFormatTypeKey : @(kCVPixelFormatType_32BGRA),
                (__bridge NSString *)kCVPixelBufferWidthKey           : @(width),
                (__bridge NSString *)kCVPixelBufferHeightKey          : @(height),
            };
            CVPixelBufferRef pb = NULL;
            CVReturn status = CVPixelBufferCreateWithIOSurface(
                kCFAllocatorDefault, surface,
                (__bridge CFDictionaryRef)attrs, &pb);
            if (status == kCVReturnSuccess && pb) {
                pixelBuffer = pb;
            }
            CFRelease(surface);
        }
    }

    if (!pixelBuffer) {
        pixelBuffer = CreateBlackFrame(width, height);
    }
    if (!pixelBuffer) return;

    CMSampleBufferRef sampleBuffer = CreateSampleBuffer(pixelBuffer);
    CVPixelBufferRelease(pixelBuffer);
    if (!sampleBuffer) return;

    // Enqueue if there is room.
    if (CMSimpleQueueGetCount(gQueue) < CMSimpleQueueGetCapacity(gQueue)) {
        // CMSimpleQueue retains via the enqueue; we transfer our +1 reference.
        CMSimpleQueueEnqueue(gQueue, sampleBuffer);
    } else {
        CFRelease(sampleBuffer);
    }

    if (gAlteredProc) {
        gAlteredProc(gStreamID, sampleBuffer, gAlteredRefCon);
    }

    gSequenceNumber++;
}

static void StartTimer(void) {
    if (gTimer) return;

    gStreamRunning = YES;
    gSequenceNumber = 0;

    dispatch_queue_t queue = dispatch_queue_create("com.lodestone.camera-dal.timer",
                                                    DISPATCH_QUEUE_SERIAL);
    gTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, queue);

    uint64_t interval = (uint64_t)(NSEC_PER_SEC / kFrameRate);
    dispatch_source_set_timer(gTimer, DISPATCH_TIME_NOW, interval, interval / 10);
    dispatch_source_set_event_handler(gTimer, ^{
        TimerTick();
    });
    dispatch_resume(gTimer);
}

// ---------------------------------------------------------------------------
// MARK: - Property helpers
// ---------------------------------------------------------------------------

/// Return a CFString property in the standard way.
static OSStatus CopyStringProperty(const char *str,
                                    UInt32 dataSize, UInt32 *dataUsed,
                                    void *data) {
    CFStringRef cfStr = CFStringCreateWithCString(kCFAllocatorDefault, str, kCFStringEncodingUTF8);
    if (!cfStr) return kCMIOHardwareUnspecifiedError;
    if (dataSize < sizeof(CFStringRef)) { CFRelease(cfStr); return kCMIOHardwareBadPropertySizeError; }
    *dataUsed = sizeof(CFStringRef);
    *(CFStringRef *)data = cfStr; // caller owns
    return kCMIOHardwareNoError;
}

// ---------------------------------------------------------------------------
// MARK: - Plugin interface functions
// ---------------------------------------------------------------------------

static HRESULT PlugIn_QueryInterface(void *thisPointer, REFIID uuid, LPVOID *interface) {
    CFUUIDRef requested = CFUUIDCreateFromUUIDBytes(kCFAllocatorDefault, uuid);
    CFUUIDRef pluginIID = CFUUIDGetConstantUUIDWithBytes(kCFAllocatorDefault,
        0xB8, 0x9D, 0xFA, 0xBA, 0x93, 0xBF, 0x11, 0xD8,
        0x8E, 0xA6, 0x00, 0x0A, 0x95, 0xAF, 0x9C, 0x6A); // kCMIOHardwarePlugInInterfaceID
    CFUUIDRef unknownIID = CFUUIDGetConstantUUIDWithBytes(kCFAllocatorDefault,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46); // IUnknownUUID

    BOOL match = CFEqual(requested, pluginIID) || CFEqual(requested, unknownIID);
    CFRelease(requested);

    if (!match) {
        *interface = NULL;
        return E_NOINTERFACE;
    }

    *interface = gPlugInRef;
    return S_OK;
}

static ULONG PlugIn_AddRef(void *thisPointer) {
    return 1; // singleton, never released
}

static ULONG PlugIn_Release(void *thisPointer) {
    return 1; // singleton, never released
}

static OSStatus PlugIn_Initialize(CMIOHardwarePlugInRef self) {
    // Superseded by InitializeWithObjectID; should not be called.
    return kCMIOHardwareUnspecifiedError;
}

static OSStatus PlugIn_InitializeWithObjectID(CMIOHardwarePlugInRef self,
                                               CMIOObjectID objectID) {
    gPlugInID = objectID;

    // --- Create Device ---
    CMIOObjectID deviceID = 0;
    {
        OSStatus err = CMIOObjectCreate(gPlugInRef, gPlugInID, kCMIODeviceClassID, &deviceID);
        if (err != kCMIOHardwareNoError) return err;
        gDeviceID = deviceID;
    }

    // --- Create Stream ---
    CMIOObjectID streamID = 0;
    {
        OSStatus err = CMIOObjectCreate(gPlugInRef, gDeviceID, kCMIOStreamClassID, &streamID);
        if (err != kCMIOHardwareNoError) return err;
        gStreamID = streamID;
    }

    // --- Notify: plugin owns device ---
    {
        CMIOObjectPropertyAddress addr;
        addr.mSelector = kCMIOObjectPropertyOwnedObjects;
        addr.mScope    = kCMIOObjectPropertyScopeGlobal;
        addr.mElement  = kCMIOObjectPropertyElementMain;
        CMIOObjectPropertiesChanged(gPlugInRef, gPlugInID, 1, &addr);
    }

    // --- Notify: device owns stream ---
    {
        CMIOObjectPropertyAddress addr;
        addr.mSelector = kCMIOObjectPropertyOwnedObjects;
        addr.mScope    = kCMIOObjectPropertyScopeGlobal;
        addr.mElement  = kCMIOObjectPropertyElementMain;
        CMIOObjectPropertiesChanged(gPlugInRef, gDeviceID, 1, &addr);
    }

    // --- Notify: device streams ---
    {
        CMIOObjectPropertyAddress addr;
        addr.mSelector = kCMIODevicePropertyStreams;
        addr.mScope    = kCMIOObjectPropertyScopeGlobal;
        addr.mElement  = kCMIOObjectPropertyElementMain;
        CMIOObjectPropertiesChanged(gPlugInRef, gDeviceID, 1, &addr);
    }

    // --- Notify: device is alive ---
    {
        CMIOObjectPropertyAddress addr;
        addr.mSelector = kCMIODevicePropertyDeviceIsAlive;
        addr.mScope    = kCMIOObjectPropertyScopeGlobal;
        addr.mElement  = kCMIOObjectPropertyElementMain;
        CMIOObjectPropertiesChanged(gPlugInRef, gDeviceID, 1, &addr);
    }

    return kCMIOHardwareNoError;
}

static OSStatus PlugIn_Teardown(CMIOHardwarePlugInRef self) {
    StopTimer();

    if (gQueue) {
        // Drain the queue.
        while (CMSimpleQueueGetCount(gQueue) > 0) {
            CFTypeRef item = CMSimpleQueueDequeue(gQueue);
            if (item) CFRelease(item);
        }
        CFRelease(gQueue);
        gQueue = NULL;
    }

    gAlteredProc = NULL;
    gAlteredRefCon = NULL;

    return kCMIOHardwareNoError;
}

// ---------------------------------------------------------------------------
// MARK: - Property dispatch: HasProperty
// ---------------------------------------------------------------------------

static Boolean PlugIn_ObjectHasProperty(CMIOHardwarePlugInRef self,
                                         CMIOObjectID objectID,
                                         const CMIOObjectPropertyAddress *address) {
    // --- Plugin ---
    if (objectID == gPlugInID) {
        switch (address->mSelector) {
            case kCMIOObjectPropertyOwnedObjects:
                return true;
            default:
                return false;
        }
    }

    // --- Device ---
    if (objectID == gDeviceID) {
        switch (address->mSelector) {
            case kCMIOObjectPropertyName:
            case kCMIOObjectPropertyManufacturer:
            case kCMIOObjectPropertyOwnedObjects:
            case kCMIODevicePropertyDeviceUID:
            case kCMIODevicePropertyModelUID:
            case kCMIODevicePropertyTransportType:
            case kCMIODevicePropertyStreams:
            case kCMIODevicePropertyDeviceIsAlive:
            case kCMIODevicePropertyDeviceHasChanged:
            case kCMIODevicePropertyDeviceIsRunning:
            case kCMIODevicePropertyDeviceIsRunningSomewhere:
            case kCMIODevicePropertyDeviceCanBeDefaultDevice:
                return true;
            default:
                return false;
        }
    }

    // --- Stream ---
    if (objectID == gStreamID) {
        switch (address->mSelector) {
            case kCMIOStreamPropertyDirection:
            case kCMIOStreamPropertyFormatDescription:
            case kCMIOStreamPropertyFormatDescriptions:
            case kCMIOStreamPropertyFrameRate:
            case kCMIOStreamPropertyFrameRates:
            case kCMIOStreamPropertyMinimumFrameRate:
                return true;
            default:
                return false;
        }
    }

    return false;
}

// ---------------------------------------------------------------------------
// MARK: - Property dispatch: IsSettable
// ---------------------------------------------------------------------------

static OSStatus PlugIn_ObjectIsPropertySettable(CMIOHardwarePlugInRef self,
                                                 CMIOObjectID objectID,
                                                 const CMIOObjectPropertyAddress *address,
                                                 Boolean *isSettable) {
    *isSettable = false;
    return kCMIOHardwareNoError;
}

// ---------------------------------------------------------------------------
// MARK: - Property dispatch: GetDataSize
// ---------------------------------------------------------------------------

static OSStatus PlugIn_ObjectGetPropertyDataSize(CMIOHardwarePlugInRef self,
                                                  CMIOObjectID objectID,
                                                  const CMIOObjectPropertyAddress *address,
                                                  UInt32 qualifierDataSize,
                                                  const void *qualifierData,
                                                  UInt32 *dataSize) {
    // --- Plugin ---
    if (objectID == gPlugInID) {
        switch (address->mSelector) {
            case kCMIOObjectPropertyOwnedObjects:
                *dataSize = sizeof(CMIOObjectID);
                return kCMIOHardwareNoError;
        }
    }

    // --- Device ---
    if (objectID == gDeviceID) {
        switch (address->mSelector) {
            case kCMIOObjectPropertyName:
            case kCMIOObjectPropertyManufacturer:
            case kCMIODevicePropertyDeviceUID:
            case kCMIODevicePropertyModelUID:
                *dataSize = sizeof(CFStringRef);
                return kCMIOHardwareNoError;

            case kCMIOObjectPropertyOwnedObjects:
            case kCMIODevicePropertyStreams:
                *dataSize = sizeof(CMIOObjectID);
                return kCMIOHardwareNoError;

            case kCMIODevicePropertyTransportType:
                *dataSize = sizeof(UInt32);
                return kCMIOHardwareNoError;

            case kCMIODevicePropertyDeviceIsAlive:
            case kCMIODevicePropertyDeviceHasChanged:
            case kCMIODevicePropertyDeviceIsRunning:
            case kCMIODevicePropertyDeviceIsRunningSomewhere:
            case kCMIODevicePropertyDeviceCanBeDefaultDevice:
                *dataSize = sizeof(UInt32);
                return kCMIOHardwareNoError;
        }
    }

    // --- Stream ---
    if (objectID == gStreamID) {
        switch (address->mSelector) {
            case kCMIOStreamPropertyDirection:
                *dataSize = sizeof(UInt32);
                return kCMIOHardwareNoError;

            case kCMIOStreamPropertyFormatDescription:
                *dataSize = sizeof(CMFormatDescriptionRef);
                return kCMIOHardwareNoError;

            case kCMIOStreamPropertyFormatDescriptions:
                *dataSize = sizeof(CFArrayRef);
                return kCMIOHardwareNoError;

            case kCMIOStreamPropertyFrameRate:
            case kCMIOStreamPropertyMinimumFrameRate:
                *dataSize = sizeof(Float64);
                return kCMIOHardwareNoError;

            case kCMIOStreamPropertyFrameRates:
                *dataSize = sizeof(CFArrayRef);
                return kCMIOHardwareNoError;
        }
    }

    return kCMIOHardwareUnknownPropertyError;
}

// ---------------------------------------------------------------------------
// MARK: - Property dispatch: GetData
// ---------------------------------------------------------------------------

static OSStatus PlugIn_ObjectGetPropertyData(CMIOHardwarePlugInRef self,
                                              CMIOObjectID objectID,
                                              const CMIOObjectPropertyAddress *address,
                                              UInt32 qualifierDataSize,
                                              const void *qualifierData,
                                              UInt32 dataSize,
                                              UInt32 *dataUsed,
                                              void *data) {
    // --- Plugin ---
    if (objectID == gPlugInID) {
        switch (address->mSelector) {
            case kCMIOObjectPropertyOwnedObjects:
                if (dataSize < sizeof(CMIOObjectID)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(CMIOObjectID);
                *(CMIOObjectID *)data = gDeviceID;
                return kCMIOHardwareNoError;
        }
    }

    // --- Device ---
    if (objectID == gDeviceID) {
        switch (address->mSelector) {
            case kCMIOObjectPropertyName:
                return CopyStringProperty("Lodestone Virtual Camera",
                                          dataSize, dataUsed, data);
            case kCMIOObjectPropertyManufacturer:
                return CopyStringProperty("Lodestone",
                                          dataSize, dataUsed, data);
            case kCMIODevicePropertyDeviceUID:
                return CopyStringProperty("com.lodestone.virtual-camera",
                                          dataSize, dataUsed, data);
            case kCMIODevicePropertyModelUID:
                return CopyStringProperty("com.lodestone.virtual-camera.model",
                                          dataSize, dataUsed, data);

            case kCMIOObjectPropertyOwnedObjects:
            case kCMIODevicePropertyStreams:
                if (dataSize < sizeof(CMIOObjectID)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(CMIOObjectID);
                *(CMIOObjectID *)data = gStreamID;
                return kCMIOHardwareNoError;

            case kCMIODevicePropertyTransportType:
                if (dataSize < sizeof(UInt32)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(UInt32);
                *(UInt32 *)data = 0; // virtual
                return kCMIOHardwareNoError;

            case kCMIODevicePropertyDeviceIsAlive:
            case kCMIODevicePropertyDeviceCanBeDefaultDevice:
                if (dataSize < sizeof(UInt32)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(UInt32);
                *(UInt32 *)data = 1;
                return kCMIOHardwareNoError;

            case kCMIODevicePropertyDeviceHasChanged:
                if (dataSize < sizeof(UInt32)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(UInt32);
                *(UInt32 *)data = 0;
                return kCMIOHardwareNoError;

            case kCMIODevicePropertyDeviceIsRunning:
            case kCMIODevicePropertyDeviceIsRunningSomewhere:
                if (dataSize < sizeof(UInt32)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(UInt32);
                *(UInt32 *)data = gStreamRunning ? 1 : 0;
                return kCMIOHardwareNoError;
        }
    }

    // --- Stream ---
    if (objectID == gStreamID) {
        switch (address->mSelector) {
            case kCMIOStreamPropertyDirection:
                if (dataSize < sizeof(UInt32)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(UInt32);
                *(UInt32 *)data = 1; // source
                return kCMIOHardwareNoError;

            case kCMIOStreamPropertyFormatDescription: {
                if (dataSize < sizeof(CMFormatDescriptionRef))
                    return kCMIOHardwareBadPropertySizeError;

                int32_t width, height;
                GetConfiguredDimensions(&width, &height);

                CMFormatDescriptionRef fmtDesc = NULL;
                CMVideoFormatDescriptionCreate(kCFAllocatorDefault,
                                               kCVPixelFormatType_32BGRA,
                                               width, height, NULL, &fmtDesc);
                if (!fmtDesc) return kCMIOHardwareUnspecifiedError;

                *dataUsed = sizeof(CMFormatDescriptionRef);
                *(CMFormatDescriptionRef *)data = fmtDesc; // caller owns
                return kCMIOHardwareNoError;
            }

            case kCMIOStreamPropertyFormatDescriptions: {
                if (dataSize < sizeof(CFArrayRef))
                    return kCMIOHardwareBadPropertySizeError;

                int32_t width, height;
                GetConfiguredDimensions(&width, &height);

                CMFormatDescriptionRef fmtDesc = NULL;
                CMVideoFormatDescriptionCreate(kCFAllocatorDefault,
                                               kCVPixelFormatType_32BGRA,
                                               width, height, NULL, &fmtDesc);
                if (!fmtDesc) return kCMIOHardwareUnspecifiedError;

                CFArrayRef array = CFArrayCreate(kCFAllocatorDefault,
                                                 (const void **)&fmtDesc, 1,
                                                 &kCFTypeArrayCallBacks);
                CFRelease(fmtDesc);

                *dataUsed = sizeof(CFArrayRef);
                *(CFArrayRef *)data = array; // caller owns
                return kCMIOHardwareNoError;
            }

            case kCMIOStreamPropertyFrameRate:
            case kCMIOStreamPropertyMinimumFrameRate:
                if (dataSize < sizeof(Float64)) return kCMIOHardwareBadPropertySizeError;
                *dataUsed = sizeof(Float64);
                *(Float64 *)data = kFrameRate;
                return kCMIOHardwareNoError;

            case kCMIOStreamPropertyFrameRates: {
                if (dataSize < sizeof(CFArrayRef))
                    return kCMIOHardwareBadPropertySizeError;

                Float64 rate = kFrameRate;
                CFNumberRef num = CFNumberCreate(kCFAllocatorDefault,
                                                 kCFNumberFloat64Type, &rate);
                CFArrayRef array = CFArrayCreate(kCFAllocatorDefault,
                                                 (const void **)&num, 1,
                                                 &kCFTypeArrayCallBacks);
                CFRelease(num);

                *dataUsed = sizeof(CFArrayRef);
                *(CFArrayRef *)data = array; // caller owns
                return kCMIOHardwareNoError;
            }
        }
    }

    return kCMIOHardwareUnknownPropertyError;
}

// ---------------------------------------------------------------------------
// MARK: - Property dispatch: SetData (no-op)
// ---------------------------------------------------------------------------

static OSStatus PlugIn_ObjectSetPropertyData(CMIOHardwarePlugInRef self,
                                              CMIOObjectID objectID,
                                              const CMIOObjectPropertyAddress *address,
                                              UInt32 qualifierDataSize,
                                              const void *qualifierData,
                                              UInt32 dataSize,
                                              const void *data) {
    return kCMIOHardwareUnsupportedOperationError;
}

// ---------------------------------------------------------------------------
// MARK: - Stream buffer queue
// ---------------------------------------------------------------------------

static OSStatus PlugIn_StreamCopyBufferQueue(CMIOHardwarePlugInRef self,
                                              CMIOStreamID streamID,
                                              CMIODeviceStreamQueueAlteredProc alteredProc,
                                              void *alteredRefCon,
                                              CMSimpleQueueRef *queueOut) {
    // Create the queue on first call.
    if (!gQueue) {
        CMSimpleQueueCreate(kCFAllocatorDefault, kMaxFrames, &gQueue);
        if (!gQueue) return kCMIOHardwareUnspecifiedError;
    }

    gAlteredProc   = alteredProc;
    gAlteredRefCon = alteredRefCon;

    CFRetain(gQueue);
    *queueOut = gQueue;

    // Start frame delivery when a consumer connects.
    if (alteredProc) {
        StartTimer();
    } else {
        StopTimer();
    }

    return kCMIOHardwareNoError;
}

// ---------------------------------------------------------------------------
// MARK: - Stub functions for unused interface slots
// ---------------------------------------------------------------------------

static void PlugIn_ObjectShow(CMIOHardwarePlugInRef self, CMIOObjectID objectID) {
    // No-op.
}

static OSStatus PlugIn_DeviceSuspend(CMIOHardwarePlugInRef self, CMIODeviceID deviceID) {
    return kCMIOHardwareNoError;
}

static OSStatus PlugIn_DeviceResume(CMIOHardwarePlugInRef self, CMIODeviceID deviceID) {
    return kCMIOHardwareNoError;
}

static OSStatus PlugIn_DeviceStartStream(CMIOHardwarePlugInRef self,
                                          CMIODeviceID deviceID,
                                          CMIOStreamID streamID) {
    StartTimer();
    return kCMIOHardwareNoError;
}

static OSStatus PlugIn_DeviceStopStream(CMIOHardwarePlugInRef self,
                                         CMIODeviceID deviceID,
                                         CMIOStreamID streamID) {
    StopTimer();
    return kCMIOHardwareNoError;
}

static OSStatus PlugIn_DeviceProcessAVCCommand(CMIOHardwarePlugInRef self,
                                                CMIODeviceID deviceID,
                                                CMIODeviceAVCCommand *command) {
    return kCMIOHardwareUnspecifiedError;
}

static OSStatus PlugIn_DeviceProcessRS422Command(CMIOHardwarePlugInRef self,
                                                  CMIODeviceID deviceID,
                                                  CMIODeviceRS422Command *command) {
    return kCMIOHardwareUnspecifiedError;
}

static OSStatus PlugIn_StreamDeckPlay(CMIOHardwarePlugInRef self, CMIOStreamID streamID) {
    return kCMIOHardwareUnspecifiedError;
}

static OSStatus PlugIn_StreamDeckStop(CMIOHardwarePlugInRef self, CMIOStreamID streamID) {
    return kCMIOHardwareUnspecifiedError;
}

static OSStatus PlugIn_StreamDeckJog(CMIOHardwarePlugInRef self, CMIOStreamID streamID,
                                      SInt32 speed) {
    return kCMIOHardwareUnspecifiedError;
}

static OSStatus PlugIn_StreamDeckCueTo(CMIOHardwarePlugInRef self, CMIOStreamID streamID,
                                        Float64 requestedTimecode, Boolean playOnCue) {
    return kCMIOHardwareUnspecifiedError;
}

// ---------------------------------------------------------------------------
// MARK: - vtable
// ---------------------------------------------------------------------------

static CMIOHardwarePlugInInterface gPlugInVtable = {
    // _reserved (required by COM IUnknown)
    ._reserved                      = NULL,

    // IUnknown
    .QueryInterface                 = PlugIn_QueryInterface,
    .AddRef                         = PlugIn_AddRef,
    .Release                        = PlugIn_Release,

    // CMIOHardwarePlugIn
    .Initialize                     = PlugIn_Initialize,
    .InitializeWithObjectID         = PlugIn_InitializeWithObjectID,
    .Teardown                       = PlugIn_Teardown,

    .ObjectShow                     = PlugIn_ObjectShow,
    .ObjectHasProperty              = PlugIn_ObjectHasProperty,
    .ObjectIsPropertySettable       = PlugIn_ObjectIsPropertySettable,
    .ObjectGetPropertyDataSize      = PlugIn_ObjectGetPropertyDataSize,
    .ObjectGetPropertyData          = PlugIn_ObjectGetPropertyData,
    .ObjectSetPropertyData          = PlugIn_ObjectSetPropertyData,

    .DeviceSuspend                  = PlugIn_DeviceSuspend,
    .DeviceResume                   = PlugIn_DeviceResume,
    .DeviceStartStream              = PlugIn_DeviceStartStream,
    .DeviceStopStream               = PlugIn_DeviceStopStream,
    .DeviceProcessAVCCommand        = PlugIn_DeviceProcessAVCCommand,
    .DeviceProcessRS422Command      = PlugIn_DeviceProcessRS422Command,

    .StreamCopyBufferQueue          = PlugIn_StreamCopyBufferQueue,
    .StreamDeckPlay                 = PlugIn_StreamDeckPlay,
    .StreamDeckStop                 = PlugIn_StreamDeckStop,
    .StreamDeckJog                  = PlugIn_StreamDeckJog,
    .StreamDeckCueTo                = PlugIn_StreamDeckCueTo,
};

// ---------------------------------------------------------------------------
// MARK: - Factory function (exported symbol)
// ---------------------------------------------------------------------------

__attribute__((visibility("default")))
CMIOHardwarePlugInRef LodestoneCameraPlugInCreate(CFAllocatorRef allocator,
                                                   CFUUIDRef requestedTypeUUID) {
    // Verify the caller is requesting a CMIOHardwarePlugIn.
    CFUUIDRef hwPlugInType = CFUUIDGetConstantUUIDWithBytes(kCFAllocatorDefault,
        0x30, 0x01, 0x0C, 0x1C, 0x93, 0xBF, 0x11, 0xD8,
        0x8B, 0x5B, 0x00, 0x0A, 0x95, 0xAF, 0x9C, 0x6A); // kCMIOHardwarePlugInTypeID

    if (!CFEqual(requestedTypeUUID, hwPlugInType)) {
        return NULL;
    }

    static CMIOHardwarePlugInInterface *vtablePtr = &gPlugInVtable;
    gPlugInRef = &vtablePtr;
    return gPlugInRef;
}

#pragma clang diagnostic pop
