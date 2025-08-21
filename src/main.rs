//! # BTCRecover Tokenlist CLI Binary
//!
//! This is the command-line interface for the BTCRecover Rust library.
//! For library usage, import `btcrecover_rust` directly.

use btcrecover_rust::CliApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    CliApp::run().map_err(|e| e.into())
}

