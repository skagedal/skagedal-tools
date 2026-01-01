# parse-java-repositories

A command-line tool to parse the output of macOS `/usr/libexec/java_home -X` command.

## Description

This tool parses the XML plist output from the macOS `java_home` utility and displays information about installed Java Virtual Machines in a human-readable format.

## Building

```bash
cargo build --release
```

## Usage

### On macOS with java_home

Pipe the output of `java_home -X` directly to the tool:

```bash
/usr/libexec/java_home -X | parse-java-repositories
```

Or build and run in one command:

```bash
/usr/libexec/java_home -X | cargo run
```

### With a saved XML file

```bash
cat example-input.xml | parse-java-repositories
```

Or:

```bash
parse-java-repositories < example-input.xml
```

## Example Output

```
Found 7 Java installation(s):

1. OpenJDK 25.0.1
   Version:     25.0.1
   Vendor:      Eclipse Adoptium
   Home Path:   /Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home
   Architecture: arm64
   Bundle ID:   net.java.openjdk.jdk
   Platform Ver: 25.0.1
   Enabled:     true

2. Oracle GraalVM 25.0.1+8.1
   Version:     25.0.1
   Vendor:      Oracle Corporation
   Home Path:   /Library/Java/JavaVirtualMachines/graalvm-25.jdk/Contents/Home
   Architecture: arm64
   Bundle ID:   com.oracle.java.jdk
   Platform Ver: 25.0.1
   Enabled:     true

...
```

## Installation

To install the tool to your cargo bin directory:

```bash
cargo install --path .
```

Then you can use it from anywhere:

```bash
/usr/libexec/java_home -X | parse-java-repositories
```
