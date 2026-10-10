import ArgumentParser
import CoreWLAN
import Foundation

@main
struct WifiInfo: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "wifi-info",
        abstract: "Print the current Wi-Fi network's name and BSSID.",
        discussion: """
            macOS redacts both unless the asking process has Location Services \
            permission, which only an app bundle can be granted. Run \
            `wifi-info --authorize` once to ask for it.
            """
    )

    @Flag(help: "Print only the network name.")
    var ssid = false

    @Flag(help: "Print only the BSSID, the access point's MAC address.")
    var bssid = false

    @Flag(help: "Print both as a JSON object.")
    var json = false

    @Flag(help: "Ask for Location Services permission, waiting for the answer.")
    var authorize = false

    @Flag(help: .hidden)
    var launched = false

    func validate() throws {
        if [ssid, bssid, json].filter({ $0 }).count > 1 {
            throw ValidationError("--ssid, --bssid and --json are alternatives; pick one")
        }
    }

    func run() throws {
        guard launched else {
            try Relaunch.throughLaunchServices(arguments: CommandLine.arguments.dropFirst() + ["--launched"])
            return
        }
        if authorize {
            try MainActor.assumeIsolated { try Authorization.request() }
            return
        }
        guard MainActor.assumeIsolated({ Authorization.isGranted }) else {
            throw ValidationError("no Location Services permission; run `wifi-info --authorize`")
        }
        guard let interface = CWWiFiClient.shared().interface(), let name = interface.ssid() else {
            throw ExitCode(1)
        }
        let station = interface.bssid()
        if ssid {
            print(name)
        } else if bssid {
            print(station ?? "")
        } else if json {
            print(try Network(ssid: name, bssid: station).json())
        } else {
            print("ssid\t\(name)")
            print("bssid\t\(station ?? "")")
        }
    }
}

struct Network: Encodable {
    let ssid: String
    let bssid: String?

    enum CodingKeys: CodingKey {
        case ssid, bssid
    }

    // Spelled out so a missing BSSID is null rather than an absent key.
    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(ssid, forKey: .ssid)
        try container.encode(bssid, forKey: .bssid)
    }

    func json() throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return String(decoding: try encoder.encode(self), as: UTF8.self)
    }
}
