package tech.skagedal;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.*;

public class GitReposLatestActivity {
    record RepoActivity(Path path, Instant timestamp) implements Comparable<RepoActivity> {
        @Override
        public int compareTo(RepoActivity other) {
            return this.timestamp.compareTo(other.timestamp);
        }
    }

    static Instant getLatestCommitTime(final Path path) {
        final var processBuilder = new ProcessBuilder()
            .directory(path.toFile())
            .command("git", "log", "-1", "--format=%cI");
        try {
            final var process = processBuilder.start();
            final var exitCode = process.waitFor();
            if (exitCode != 0) {
                throw new RuntimeException("Git command failed with exit code " + exitCode + " in " + path);
            }
            final var output = new String(process.getInputStream().readAllBytes()).trim();
            if (output.isBlank()) {
                throw new RuntimeException("No commits found in " + path);
            }
            return Instant.parse(output);
        } catch (IOException | InterruptedException e) {
            throw new RuntimeException("Failed to get latest commit time in " + path, e);
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

    static void printRepoActivities(List<Path> topPaths) {
        try {
            final var activities = findRepositories(topPaths)
                .stream()
                .parallel()
                .filter(Files::isDirectory)
                .map(p -> new RepoActivity(p, getLatestCommitTime(p)))
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
