package tech.skagedal;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class GitReposLatestActivityTest {

    private static void run(Path cwd, String... command) throws IOException, InterruptedException {
        final var process = new ProcessBuilder()
            .directory(cwd.toFile())
            .command(command)
            .redirectErrorStream(true)
            .start();
        final var exit = process.waitFor();
        if (exit != 0) {
            throw new RuntimeException(
                "Command " + String.join(" ", command) + " failed: "
                    + new String(process.getInputStream().readAllBytes()));
        }
    }

    private static void initRepoWithCommit(Path repo, String commitDate) throws IOException, InterruptedException {
        Files.createDirectories(repo);
        run(repo, "git", "init", "-q", "-b", "main");
        run(repo, "git", "config", "user.email", "test@example.com");
        run(repo, "git", "config", "user.name", "Test");
        Files.writeString(repo.resolve("README.md"), "hello\n");
        run(repo, "git", "add", ".");
        final var pb = new ProcessBuilder()
            .directory(repo.toFile())
            .command("git", "commit", "-q", "-m", "initial")
            .redirectErrorStream(true);
        pb.environment().put("GIT_AUTHOR_DATE", commitDate);
        pb.environment().put("GIT_COMMITTER_DATE", commitDate);
        final var p = pb.start();
        if (p.waitFor() != 0) {
            throw new RuntimeException(
                "git commit failed: " + new String(p.getInputStream().readAllBytes()));
        }
    }

    @Test
    void getLatestCommitTimeReturnsCommitterDate(@TempDir Path tmp) throws Exception {
        final var repo = tmp.resolve("repo");
        initRepoWithCommit(repo, "2024-06-15T12:34:56+00:00");

        final var ts = GitReposLatestActivity.getLatestCommitTime(repo);
        assertEquals(Instant.parse("2024-06-15T12:34:56Z"), ts);
    }

    @Test
    void getLatestCommitTimeFailsForNonRepo(@TempDir Path tmp) {
        assertThrows(RuntimeException.class,
            () -> GitReposLatestActivity.getLatestCommitTime(tmp));
    }

    @Test
    void findRepositoriesListsSubdirectories(@TempDir Path tmp) throws Exception {
        final var top = tmp.resolve("top");
        Files.createDirectories(top.resolve("alpha"));
        Files.createDirectories(top.resolve("beta"));
        Files.writeString(top.resolve("not-a-dir"), "x");

        final var repos = GitReposLatestActivity.findRepositories(List.of(top));

        assertEquals(3, repos.size(),
            "findRepositories returns every entry, including files; filtering happens later");
        assertTrue(repos.contains(top.resolve("alpha")));
        assertTrue(repos.contains(top.resolve("beta")));
    }

    @Test
    void findRepositoriesAcrossMultipleTopPaths(@TempDir Path tmp) throws Exception {
        final var topA = tmp.resolve("a");
        final var topB = tmp.resolve("b");
        Files.createDirectories(topA.resolve("one"));
        Files.createDirectories(topB.resolve("two"));
        Files.createDirectories(topB.resolve("three"));

        final var repos = GitReposLatestActivity.findRepositories(List.of(topA, topB));
        assertEquals(3, repos.size());
    }
}
