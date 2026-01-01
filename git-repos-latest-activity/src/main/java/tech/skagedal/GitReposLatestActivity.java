package tech.skagedal;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.*;
import java.util.stream.Collectors;

public class GitReposLatestActivity {
  record RepoActivity(Path path, Instant timestamp) implements Comparable<RepoActivity> {
    @Override
    public int compareTo(RepoActivity other) {
      return this.timestamp.compareTo(other.timestamp);
    }
  }

  static Optional<Instant> getLatestCommitTime(final Path path) {
    final var processBuilder = new ProcessBuilder()
        .directory(path.toFile())
        .command("git", "log", "-1", "--format=%cI");
    try {
      final var process = processBuilder.start();
      final var exitCode = process.waitFor();
      if (exitCode != 0) {
        // Not a git repository or no commits
        return Optional.empty();
      }
      final var output = new String(process.getInputStream().readAllBytes()).trim();
      if (output.isBlank()) {
        return Optional.empty();
      }
      return Optional.of(Instant.parse(output));
    } catch (IOException | InterruptedException e) {
      // Failed to check git status, skip this directory
      return Optional.empty();
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

  static boolean isGitRepository(Path path) {
    return Files.isDirectory(path.resolve(".git"));
  }

  static void printRepoActivities(List<Path> topPaths) {
    try {
      final var activities = findRepositories(topPaths)
          .stream()
          .parallel()
          .filter(Files::isDirectory)
          .filter(p -> !p.getFileName().toString().equals(".git"))
          .filter(GitReposLatestActivity::isGitRepository)
          .map(p -> {
            final var timestamp = getLatestCommitTime(p);
            return timestamp.map(instant -> new RepoActivity(p, instant));
          })
          .filter(Optional::isPresent)
          .map(Optional::get)
          .sorted()
          .toList();

      activities.forEach(activity ->
          System.out.println(activity.path + " " + activity.timestamp)
      );
    } catch (IOException e) {
      throw new RuntimeException("Failed to list subdirectories", e);
    }
  }

  public static void main(String[] args) {
    final var paths = Arrays.stream(args).map(Path::of).toList();
    printRepoActivities(paths);
  }
}
