package tech.skagedal;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

public class GitDirtyChecker {
  static boolean isDirty(final Path path) {
    final var processBuilder = new ProcessBuilder()
        .directory(path.toFile())
        .command("git", "status", "--porcelain");
    try {
      final var process = processBuilder.start();
      final var exitCode = process.waitFor();
      if (exitCode != 0) {
        throw new RuntimeException("Git command failed with exit code " + exitCode + " in " + path);
      }
      final var output = new String(process.getInputStream().readAllBytes());
      return !output.isBlank();
    } catch (IOException | InterruptedException e) {
      throw new RuntimeException("Failed to check git status in " + path, e);
    }
  }

  static Collection<Path> findRepositories(List<Path> topPaths) throws IOException {
    final var repositories = new ArrayList<Path>();
    for (final var path : topPaths) {
      try (final var stream = Files.list(path)) {
        stream.forEach(repositories::add);
      }
    }
    return repositories;
  }

  static void printDirtyRepos(List<Path> topPaths) {
    try  {
      findRepositories(topPaths)
          .stream()
          .parallel()
          .filter(Files::isDirectory)
          .forEach(p -> {
            if (isDirty(p)) {
              IO.println(p);
            }
          });
    } catch (IOException e) {
      throw new RuntimeException("Failed to list subdirectories", e);
    }
  }

  static final String USAGE = """
      Usage: git-dirty-checker <directory> [<directory> ...]

      Finds git repositories with uncommitted changes in subdirectories.
      """;

  static void main(String[] args) {
    final var argList = Arrays.asList(args);
    if (argList.contains("--help") || argList.contains("-h")) {
      System.out.print(USAGE);
      return;
    }
    if (args.length == 0) {
      System.err.print(USAGE);
      System.exit(1);
    }
    final var paths = argList.stream()
        .filter(a -> !a.startsWith("-"))
        .map(Path::of)
        .toList();
    printDirtyRepos(paths);
  }
}
