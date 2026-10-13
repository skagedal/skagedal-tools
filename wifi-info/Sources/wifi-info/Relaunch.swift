import ArgumentParser
import Foundation

/// Location Services permission is checked against the responsible process.
/// Run from a terminal, that is the terminal, which has no permission and gets
/// the network name redacted. Launched by LaunchServices, the app is
/// responsible for itself, so a terminal invocation goes round through `open`
/// and relays what the app printed.
enum Relaunch {
    static func throughLaunchServices(arguments: [String]) throws {
        // The bin entry is a symlink, and Bundle.main does not see through it.
        let bundle = URL(fileURLWithPath: Bundle.main.executablePath ?? "")
            .resolvingSymlinksInPath()
            .deletingLastPathComponent()  // MacOS
            .deletingLastPathComponent()  // Contents
            .deletingLastPathComponent()
        guard bundle.pathExtension == "app" else {
            throw ValidationError(
                "not running from inside wifi-info.app; install it with skagedal-tools' install")
        }
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("wifi-info-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let out = directory.appendingPathComponent("stdout")
        let err = directory.appendingPathComponent("stderr")

        let open = Process()
        open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        // -W waits, -g keeps it in the background, -n starts a fresh instance
        // so the arguments reach it.
        open.arguments =
            ["-W", "-g", "-n", "--stdout", out.path, "--stderr", err.path, bundle.path, "--args"] + arguments
        let openErrors = Pipe()
        open.standardError = openErrors
        try open.run()
        open.waitUntilExit()

        let output = (try? Data(contentsOf: out)) ?? Data()
        let errors = (try? Data(contentsOf: err)) ?? Data()
        FileHandle.standardOutput.write(output)
        FileHandle.standardError.write(errors)
        // open does not pass on the app's exit status, so an answer with
        // nothing on stdout is the failure case. Its own status is no better:
        // when the app exits before -W starts watching it, open fails with
        // "Unable to block on applications" although the answer is there.
        if output.isEmpty {
            FileHandle.standardError.write(openErrors.fileHandleForReading.readDataToEndOfFile())
            throw ExitCode(1)
        }
    }
}
