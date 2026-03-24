import CoreMediaIO
import Foundation

let provider = LodestoneProvider()
CMIOExtensionProvider.startService(provider: provider)
CFRunLoopRun()
