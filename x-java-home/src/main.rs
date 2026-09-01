use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)]
struct JVMInfo {
    #[serde(rename = "JVMArch")]
    JVMArch: String,
    #[serde(rename = "JVMBundleID")]
    JVMBundleID: String,
    #[serde(rename = "JVMEnabled")]
    JVMEnabled: bool,
    #[serde(rename = "JVMHomePath")]
    JVMHomePath: String,
    #[serde(rename = "JVMName")]
    JVMName: String,
    #[serde(rename = "JVMPlatformVersion")]
    JVMPlatformVersion: String,
    #[serde(rename = "JVMVendor")]
    JVMVendor: String,
    #[serde(rename = "JVMVersion")]
    JVMVersion: String,
}

#[derive(Debug, Serialize)]
struct Output {
    jvms: Vec<JVMInfo>,
}

#[derive(Parser, Debug)]
#[command(name = "x-java-home")]
#[command(about = "A drop-in replacement for /usr/libexec/java_home with JSON output support", long_about = None)]
struct Args {
    /// Filter versions (as if JAVA_VERSION had been set in the environment)
    #[arg(short = 'v', long = "version")]
    version: Option<String>,

    /// Filter architecture (as if JAVA_ARCH had been set in the environment)
    #[arg(short = 'a', long = "arch")]
    arch: Option<String>,

    /// Filter by JVM name (case-insensitive substring match)
    #[arg(short = 'n', long = "name")]
    name: Option<String>,

    /// Fail when filters return no JVMs, do not continue with default
    #[arg(short = 'F', long = "failfast")]
    failfast: bool,

    /// Execute the $JAVA_HOME/bin/<command> with the remaining arguments
    #[arg(long = "exec", num_args = 1..)]
    exec: Option<Vec<String>>,

    /// Print full JVM list and additional data as JSON
    #[arg(long = "json")]
    json: bool,

    /// Print full JVM list with architectures
    #[arg(short = 'V', long = "verbose")]
    verbose: bool,
}

fn parse_jvms(plist_bytes: &[u8]) -> Result<Vec<JVMInfo>> {
    plist::from_bytes(plist_bytes).context("Failed to parse plist XML from java_home")
}

fn filter_by_name(jvms: &mut Vec<JVMInfo>, name_filter: &str) {
    let name_lower = name_filter.to_lowercase();
    jvms.retain(|jvm| jvm.JVMName.to_lowercase().contains(&name_lower));
}

fn main() -> Result<()> {
    let args = Args::parse();

    // If --name or --json is provided, we need to fetch and parse the XML output
    if args.name.is_some() || args.json {
        // Build the java_home command with -X flag to get XML output
        let mut cmd = Command::new("/usr/libexec/java_home");
        cmd.arg("-X");

        // Add version filter if provided
        if let Some(version) = &args.version {
            cmd.arg("-v").arg(version);
        }

        // Add architecture filter if provided
        if let Some(arch) = &args.arch {
            cmd.arg("-a").arg(arch);
        }

        // Add failfast flag if provided
        if args.failfast {
            cmd.arg("-F");
        }

        let output = cmd.output().context("Failed to execute java_home")?;

        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            std::process::exit(output.status.code().unwrap_or(1));
        }

        // Parse the plist XML
        let mut jvms: Vec<JVMInfo> =
            parse_jvms(&output.stdout).context("Failed to parse plist XML from java_home")?;

        // Filter by name if provided
        if let Some(name_filter) = &args.name {
            filter_by_name(&mut jvms, name_filter);
        }

        // Handle output
        if args.json {
            // Create output structure
            let json_output = Output { jvms };

            // Serialize to JSON and print
            let json = serde_json::to_string_pretty(&json_output)
                .context("Failed to serialize to JSON")?;

            println!("{}", json);
        } else {
            // Output the first matching JVM's home path
            if let Some(jvm) = jvms.first() {
                println!("{}", jvm.JVMHomePath);
            } else {
                eprintln!("No matching JVM found");
                std::process::exit(1);
            }
        }
    } else {
        // Pass through to java_home for standard behavior
        let mut cmd = Command::new("/usr/libexec/java_home");

        // Add version filter if provided
        if let Some(version) = &args.version {
            cmd.arg("-v").arg(version);
        }

        // Add architecture filter if provided
        if let Some(arch) = &args.arch {
            cmd.arg("-a").arg(arch);
        }

        // Add failfast flag if provided
        if args.failfast {
            cmd.arg("-F");
        }

        // Handle --exec flag
        if let Some(exec_args) = &args.exec {
            cmd.arg("--exec");
            cmd.args(exec_args);
        }

        // Add verbose flag if provided
        if args.verbose {
            cmd.arg("-V");
        }

        // Execute java_home and pass through output
        let output = cmd.output().context("Failed to execute java_home")?;

        // Print stdout
        print!("{}", String::from_utf8_lossy(&output.stdout));

        // Print stderr
        eprint!("{}", String::from_utf8_lossy(&output.stderr));

        // Exit with the same status code
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE_PLIST: &[u8] = include_bytes!("../example-input.xml");

    #[test]
    fn parses_example_plist() {
        let jvms = parse_jvms(EXAMPLE_PLIST).unwrap();
        assert_eq!(jvms.len(), 7);
        assert_eq!(jvms[0].JVMName, "OpenJDK 25.0.1");
        assert_eq!(jvms[0].JVMArch, "arm64");
        assert!(jvms[0].JVMEnabled);
        assert_eq!(
            jvms[0].JVMHomePath,
            "/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home"
        );
    }

    #[test]
    fn parse_jvms_rejects_invalid_input() {
        assert!(parse_jvms(b"not a plist").is_err());
    }

    #[test]
    fn filter_by_name_matches_substring_case_insensitive() {
        let mut jvms = parse_jvms(EXAMPLE_PLIST).unwrap();
        filter_by_name(&mut jvms, "zulu");
        assert_eq!(jvms.len(), 4);
        assert!(jvms.iter().all(|j| j.JVMName.contains("Zulu")));
    }

    #[test]
    fn filter_by_name_uppercase_query_still_matches() {
        let mut jvms = parse_jvms(EXAMPLE_PLIST).unwrap();
        filter_by_name(&mut jvms, "GRAALVM");
        assert_eq!(jvms.len(), 1);
        assert_eq!(jvms[0].JVMName, "Oracle GraalVM 25.0.1+8.1");
    }

    #[test]
    fn filter_by_name_no_matches_yields_empty() {
        let mut jvms = parse_jvms(EXAMPLE_PLIST).unwrap();
        filter_by_name(&mut jvms, "nonexistent");
        assert!(jvms.is_empty());
    }
}
