import CoreLocation
import Foundation

enum Authorization {
    /// A new manager reports notDetermined until locationd has answered its
    /// delegate, so wait for that first callback before trusting the status.
    @MainActor
    static var isGranted: Bool {
        let manager = CLLocationManager()
        let observer = AnswerObserver()
        manager.delegate = observer
        observer.wait(timeout: 2)
        return manager.authorizationStatus == .authorizedAlways
    }

    /// Asks for permission and spins the run loop until the answer comes, since
    /// the prompt is delivered to a delegate rather than returned.
    @MainActor
    static func request() throws {
        let manager = CLLocationManager()
        let observer = AnswerObserver()
        manager.delegate = observer
        manager.requestWhenInUseAuthorization()
        observer.wait(timeout: 120)
        switch manager.authorizationStatus {
        case .authorizedAlways:
            print("Location Services permission granted.")
        case .notDetermined:
            print(
                "No answer. Look for the prompt, or System Settings → Privacy & Security → Location Services."
            )
            throw Failure()
        default:
            printError(
                "Permission denied. Change it in System Settings → Privacy & Security → Location Services.")
            throw Failure()
        }
    }

    struct Failure: Error {}

    private static func printError(_ message: String) {
        FileHandle.standardError.write(Data((message + "\n").utf8))
    }

    /// Runs the main run loop until locationd has given an answer. The
    /// callback is delivered on the main run loop, so it has to keep running;
    /// blocking on a semaphore instead would never see the answer.
    private final class AnswerObserver: NSObject, CLLocationManagerDelegate, @unchecked Sendable {
        private var answered = false

        func wait(timeout: TimeInterval) {
            let deadline = Date().addingTimeInterval(timeout)
            while !answered, case let remaining = deadline.timeIntervalSinceNow, remaining > 0 {
                CFRunLoopRunInMode(.defaultMode, remaining, false)
            }
        }

        func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
            if manager.authorizationStatus != .notDetermined {
                answered = true
                CFRunLoopStop(CFRunLoopGetMain())
            }
        }
    }
}
