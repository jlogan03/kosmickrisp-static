import Foundation
import Metal

var observations: [String] = []
func report(_ title: String, _ message: String) {
    let escaped = message.replacingOccurrences(of: "%", with: "%25")
        .replacingOccurrences(of: "\n", with: "%0A")
        .replacingOccurrences(of: "\r", with: "%0D")
    observations.append("\(title): \(escaped)")
    print("\(title): \(message)")
}

let source = """
#include <metal_stdlib>
using namespace metal;
kernel void probe(device uint *values [[buffer(0)]], uint i [[thread_position_in_grid]]) {
    values[i] = values[i] * 3 + 1;
}
"""
let families: [(String, MTLGPUFamily)] = [
    ("Metal 4", .metal4), ("Metal 3", .metal3),
    ("Mac 2", .mac2), ("Common 3", .common3),
    ("Common 2", .common2), ("Common 1", .common1),
    ("Apple 1", .apple1), ("Apple 7", .apple7), ("Apple 8", .apple8),
]
let versions: [(String, MTLLanguageVersion)] = [
    ("2.4", .version2_4), ("3.0", .version3_0), ("3.1", .version3_1),
    ("3.2", .version3_2), ("4.0", .version4_0),
]
let devices = MTLCopyAllDevices()
report("Metal devices", "Count: \(devices.count)")
precondition(!devices.isEmpty, "No Metal device")
for device in devices {
    report("Metal device", device.name)
    for (name, family) in families {
        report("GPU family", "\(name): \(device.supportsFamily(family))")
    }
    report("Features", "Argument buffers tier 2: \(device.argumentBuffersSupport == .tier2), dynamic libraries: \(device.supportsDynamicLibraries), function pointers: \(device.supportsFunctionPointers), unified memory: \(device.hasUnifiedMemory)")
    let count = 128
    let buffer = device.makeBuffer(length: count * MemoryLayout<UInt32>.stride, options: .storageModeShared)!
    report("Buffer address", "Nonzero: \(buffer.gpuAddress != 0)")
    let values = buffer.contents().bindMemory(to: UInt32.self, capacity: count)
    let queue = device.makeCommandQueue()!
    var successes = 0
    for (name, version) in versions {
        do {
            let options = MTLCompileOptions()
            options.languageVersion = version
            let library = try device.makeLibrary(source: source, options: options)
            let pipeline = try device.makeComputePipelineState(function: library.makeFunction(name: "probe")!)
            for i in 0..<count { values[i] = UInt32(i) }
            let command = queue.makeCommandBuffer()!
            let encoder = command.makeComputeCommandEncoder()!
            encoder.setComputePipelineState(pipeline)
            encoder.setBuffer(buffer, offset: 0, index: 0)
            encoder.dispatchThreadgroups(MTLSize(width: count, height: 1, depth: 1),
                                        threadsPerThreadgroup: MTLSize(width: 1, height: 1, depth: 1))
            encoder.endEncoding()
            command.commit()
            command.waitUntilCompleted()
            if let error = command.error { throw error }
            precondition(command.status == .completed, "Compute did not complete")
            precondition((0..<count).allSatisfy { values[$0] == UInt32($0 * 3 + 1) }, "Wrong compute result")
            report("MSL \(name)", "Compiled and ran: all \(count) values match")
            successes += 1
        } catch {
            report("MSL \(name)", "Failed: \(error)")
        }
    }
    precondition(successes > 0, "No language version passed the compute test")
}
print("::notice title=Metal capabilities::\(observations.joined(separator: " | "))")
