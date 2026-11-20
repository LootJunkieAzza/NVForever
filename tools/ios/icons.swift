// [design] Reuse NVForever's own icon; generated iPad sizes never enter the repository.
// App icons have an opaque black backing rather than the desktop icon's transparency.
import Foundation
import CoreGraphics
import ImageIO

let sourceURL = URL(fileURLWithPath: CommandLine.arguments[1])
let source = CGImageSourceCreateWithURL(sourceURL as CFURL, nil)!
let image = CGImageSourceCreateImageAtIndex(source, 0, nil)!
let destination = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
for size in [20, 29, 40, 58, 76, 80, 152, 167, 1024] {
    let context = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8,
        bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    let bounds = CGRect(x: 0, y: 0, width: size, height: size)
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fill(bounds)
    context.interpolationQuality = .high
    context.draw(image, in: bounds)
    let url = destination.appendingPathComponent("Icon-\(size).png")
    let output = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(output, context.makeImage()!, nil)
    precondition(CGImageDestinationFinalize(output), "Writing app icon")
}
