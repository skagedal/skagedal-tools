# parse-java-repositories

A command-line tool to parse the output of macOS `/usr/libexec/java_home -X` command.

## Description

This tool parses the XML plist output from the macOS `java_home` utility and outputs information about installed Java Virtual Machines as a JSON object.

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

## Installation

To install the tool to your cargo bin directory:

```bash
cargo install --path .
```

Then you can use it from anywhere:

```bash
/usr/libexec/java_home -X | parse-java-repositories
```
