use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

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

fn main() -> Result<()> {
    // Read input from stdin
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("Failed to read from stdin")?;

    // Parse the plist
    let jvms: Vec<JVMInfo> = plist::from_bytes(input.as_bytes())
        .context("Failed to parse plist XML")?;

    // Create output structure
    let output = Output { jvms };

    // Serialize to JSON and print
    let json = serde_json::to_string_pretty(&output)
        .context("Failed to serialize to JSON")?;

    println!("{}", json);

    Ok(())
}
