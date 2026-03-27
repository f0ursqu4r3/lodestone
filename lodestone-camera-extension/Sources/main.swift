import CoreMediaIO
import Foundation

let source = LodestoneProvider()
CMIOExtensionProvider.startService(provider: source.provider)
CFRunLoopRun()
