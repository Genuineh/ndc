//! NDC CLI Entry Point
//!
//! This binary provides the command-line interface for NDC.

use std::process;

#[tokio::main]
async fn main() {
    if let Err(e) = ndc_interface::run_cli().await {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
