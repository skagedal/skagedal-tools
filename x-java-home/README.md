# x-java-home

A drop-in replacement for macOS `/usr/libexec/java_home` with JSON output support.

## Description

This tool wraps the macOS `java_home` utility and provides all its functionality, but instead of the `-X/--xml` flag for getting detailed information about all installed JVMs, it provides a `--json` flag that outputs the same information in JSON format.

In default mode, it behaves exactly like `/usr/libexec/java_home`, returning the path to the default Java home directory.

## Building

```bash
cargo build --release
```

## Usage

The tool supports all the same flags as `/usr/libexec/java_home`:

```
Usage: x-java-home [OPTIONS]

Options:
  -v, --version <VERSION>    Filter versions (as if JAVA_VERSION had been set in the environment)
  -a, --arch <ARCH>          Filter architecture (as if JAVA_ARCH had been set in the environment)
  -F, --failfast             Fail when filters return no JVMs, do not continue with default
      --exec <EXEC>...       Execute the $JAVA_HOME/bin/<command> with the remaining arguments
      --json                 Print full JVM list and additional data as JSON
  -V, --verbose              Print full JVM list with architectures
  -h, --help                 Print help
```

### Examples

#### Get the default Java home path (same as java_home)

```bash
x-java-home
```

Output:
```
/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home
```

#### Get a specific Java version

```bash
x-java-home -v 21
```

Output:
```
/Library/Java/JavaVirtualMachines/zulu-21.jdk/Contents/Home
```

#### Get all JVMs in verbose format

```bash
x-java-home -V
```

Output:
```
Matching Java Virtual Machines (7):
    25.0.1 (arm64) "OpenJDK 25.0.1" - "Eclipse Adoptium" /Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home
    25.0.1 (arm64) "Oracle GraalVM 25.0.1+8.1" - "Oracle Corporation" /Library/Java/JavaVirtualMachines/graalvm-25.jdk/Contents/Home
    ...
/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home
```

#### Get all JVMs as JSON

```bash
x-java-home --json
```

Output:
```json
{
  "jvms": [
    {
      "JVMArch": "arm64",
      "JVMBundleID": "net.java.openjdk.jdk",
      "JVMEnabled": true,
      "JVMHomePath": "/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home",
      "JVMName": "OpenJDK 25.0.1",
      "JVMPlatformVersion": "25.0.1",
      "JVMVendor": "Eclipse Adoptium",
      "JVMVersion": "25.0.1"
    },
    {
      "JVMArch": "arm64",
      "JVMBundleID": "com.oracle.java.jdk",
      "JVMEnabled": true,
      "JVMHomePath": "/Library/Java/JavaVirtualMachines/graalvm-25.jdk/Contents/Home",
      "JVMName": "Oracle GraalVM 25.0.1+8.1",
      "JVMPlatformVersion": "25.0.1",
      "JVMVendor": "Oracle Corporation",
      "JVMVersion": "25.0.1"
    }
  ]
}
```

#### Get specific version as JSON

```bash
x-java-home -v 21 --json
```

This will return JSON data for Java 21 installations.

#### Execute a command with a specific Java version

```bash
x-java-home -v 17 --exec javac MyProgram.java
```

This executes `$JAVA_HOME/bin/javac MyProgram.java` using Java 17.

## Installation

To install the tool to your cargo bin directory:

```bash
cargo install --path .
```

Then you can use it from anywhere as a drop-in replacement for `java_home`:

```bash
# Create an alias
alias java_home='x-java-home'

# Or use it directly
x-java-home --json
```

## Differences from java_home

The only difference from the standard `/usr/libexec/java_home` tool is:

- **Removed:** `-X/--xml` flag (XML output)
- **Added:** `--json` flag (JSON output)

All other functionality remains identical.
