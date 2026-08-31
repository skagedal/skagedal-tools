import Foundation

extension URL {
    /// The path as it makes sense to print: relative to the current directory
    /// when it is below it, absolute otherwise. Keeps the tool's output short
    /// without ever being ambiguous about which file it wrote.
    public var relativeDisplayPath: String {
        // Symlinks are resolved on both sides so that the comparison survives
        // the ones macOS itself puts in the way — /tmp is a symlink to
        // /private/tmp, and /var to /private/var.
        let current = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
            .resolvingSymlinksInPath().path
        let path = resolvingSymlinksInPath().path
        guard path != current else { return "." }
        guard path.hasPrefix(current + "/") else { return path }
        return String(path.dropFirst(current.count + 1))
    }
}
