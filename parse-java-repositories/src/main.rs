use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::{self, Read};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JVMInfo {
    #[serde(rename = "JVMArch")]
    jvm_arch: String,
    #[serde(rename = "JVMBundleID")]
    jvm_bundle_id: String,
    #[serde(rename = "JVMEnabled")]
    jvm_enabled: bool,
    #[serde(rename = "JVMHomePath")]
    jvm_home_path: String,
    #[serde(rename = "JVMName")]
    jvm_name: String,
    #[serde(rename = "JVMPlatformVersion")]
    jvm_platform_version: String,
    #[serde(rename = "JVMVendor")]
    jvm_vendor: String,
    #[serde(rename = "JVMVersion")]
    jvm_version: String,
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

    // Display the parsed information
    println!("Found {} Java installation(s):\n", jvms.len());

    for (index, jvm) in jvms.iter().enumerate() {
        println!("{}. {}", index + 1, jvm.jvm_name);
        println!("   Version:     {}", jvm.jvm_version);
        println!("   Vendor:      {}", jvm.jvm_vendor);
        println!("   Home Path:   {}", jvm.jvm_home_path);
        println!("   Architecture: {}", jvm.jvm_arch);
        println!("   Bundle ID:   {}", jvm.jvm_bundle_id);
        println!("   Platform Ver: {}", jvm.jvm_platform_version);
        println!("   Enabled:     {}", jvm.jvm_enabled);
        println!();
    }

    Ok(())
}
