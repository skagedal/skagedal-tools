import CoreLocation
import Foundation

enum Authorization {
    /// A new manager reports notDetermined until locationd has answered its
    /// delegate, so wait for that first callback before trusting the status.
    @MainActor
    static var isGranted: Bool {
        let manager = CLLocationManager()
        let delegate = Delegate()
        manager.delegate = delegate
        let deadline = Date().addingTimeInterval(2)
        while !delegate.answered && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        return manager.authorizationStatus == .authorizedAlways
    }

    /// Asks for permission and spins the run loop until the answer comes, since
    /// the prompt is delivered to a delegate rather than returned.
    @MainActor
    static func request() throws {
        let manager = CLLocationManager()
        let delegate = Delegate()
        manager.delegate = delegate
        manager.requestWhenInUseAuthorization()
        let deadline = Date().addingTimeInterval(120)
        while !delegate.answered && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        }
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

    private final class Delegate: NSObject, CLLocationManagerDelegate, @unchecked Sendable {
        var answered = false

        func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
            if manager.authorizationStatus != .notDetermined {
                answered = true
            }
        }
    }
}
