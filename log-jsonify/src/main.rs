use std::io::{self, BufRead, Write};
use serde_json::{json, Value};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    for line in stdin.lock().lines() {
        match line {
            Ok(line_content) => {
                // Try to parse the line as JSON
                match serde_json::from_str::<Value>(&line_content) {
                    Ok(_) => {
                        // Valid JSON - output unchanged
                        writeln!(stdout_lock, "{}", line_content).unwrap();
                    }
                    Err(_) => {
                        // Invalid JSON - wrap in envelope
                        let envelope = json!({
                            "message": line_content
                        });
                        writeln!(stdout_lock, "{}", envelope).unwrap();
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                std::process::exit(1);
            }
        }
    }
}
