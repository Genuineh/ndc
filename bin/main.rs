//! NDC CLI Entry Point
//!
//! This binary provides the command-line interface for NDC.
//! Design philosophy (from OpenCode): human users interact via natural language.

use std::process;

#[tokio::main]
async fn main() {
    if let Err(e) = ndc_interface::run().await {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
