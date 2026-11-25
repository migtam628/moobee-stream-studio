import Foundation

@main
struct RecordingStopPolicyTests {
    static func main() {
        let domain = "com.apple.ScreenCaptureKit.SCStreamErrorDomain"
        precondition(isAlreadyStoppedStream(NSError(domain: domain, code: -3808)))
        precondition(!isAlreadyStoppedStream(NSError(domain: domain, code: -3801)))
        precondition(!isAlreadyStoppedStream(NSError(domain: domain, code: -3807)))
        precondition(!isAlreadyStoppedStream(NSError(domain: "Other", code: -3808)))
        precondition(recoveredDurationIsPlausible(3.8, elapsed: 4))
        precondition(recoveredDurationIsPlausible(1.5, elapsed: 1.7))
        precondition(!recoveredDurationIsPlausible(1, elapsed: 4))
        precondition(!recoveredDurationIsPlausible(0, elapsed: 4))
        precondition(!recoveredDurationIsPlausible(.nan, elapsed: 4))
        precondition(!recoveredDurationIsPlausible(10, elapsed: 4))
        print("ScreenCaptureKit stop recovery policy passed")
    }
}
