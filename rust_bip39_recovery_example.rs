// Pure Rust BIP39 Passphrase Recovery Tool
// Optimized for Apple Silicon M4 Max

use clap::Parser;
use rayon::prelude::*;
use ring::pbkdf2;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

// For BIP39/BIP32
use bip39::{Language, Mnemonic};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::util::bip32::{DerivationPath, ExtendedPrivKey};
use bitcoin::Address;
use sha2::Sha512;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// BIP39 mnemonic phrase (24 words)
    #[clap(short, long)]
    mnemonic: String,
    
    /// Target address or xpub to find
    #[clap(short, long)]
    target: String,
    
    /// File containing passphrase candidates
    #[clap(short, long)]
    passphrase_file: String,
    
    /// Number of threads (default: all CPU cores)
    #[clap(short = 'j', long, default_value_t = 0)]
    threads: usize,
    
    /// Derivation path (default: m/44'/0'/0'/0/0)
    #[clap(short, long, default_value = "m/44'/0'/0'/0/0")]
    derivation_path: String,
}

struct BIP39Recovery {
    mnemonic: String,
    target: String,
    derivation_path: DerivationPath,
    found: Arc<AtomicBool>,
    checked_count: Arc<AtomicUsize>,
}

impl BIP39Recovery {
    fn new(mnemonic: String, target: String, derivation_path: String) -> Self {
        Self {
            mnemonic,
            target,
            derivation_path: derivation_path.parse().expect("Invalid derivation path"),
            found: Arc::new(AtomicBool::new(false)),
            checked_count: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    fn derive_address(&self, passphrase: &str) -> Result<String, Box<dyn std::error::Error>> {
        // PBKDF2-SHA512 with 2048 iterations (BIP39 standard)
        let mut seed = [0u8; 64];
        let salt = format!("mnemonic{}", passphrase);
        
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA512,
            std::num::NonZeroU32::new(2048).unwrap(),
            salt.as_bytes(),
            self.mnemonic.as_bytes(),
            &mut seed,
        );
        
        // BIP32 master key derivation
        let secp = Secp256k1::new();
        let master = ExtendedPrivKey::new_master(bitcoin::Network::Bitcoin, &seed)?;
        
        // Derive the key at the specified path
        let derived = master.derive_priv(&secp, &self.derivation_path)?;
        
        // Generate address
        let public_key = derived.to_pub(&secp);
        let address = Address::p2pkh(&public_key.public_key, bitcoin::Network::Bitcoin);
        
        Ok(address.to_string())
    }
    
    fn check_passphrase(&self, passphrase: &str) -> bool {
        // Early exit if already found
        if self.found.load(Ordering::Relaxed) {
            return false;
        }
        
        // Update counter
        let count = self.checked_count.fetch_add(1, Ordering::Relaxed);
        if count % 1000 == 0 {
            println!("Checked {} passphrases...", count);
        }
        
        // Derive and check
        match self.derive_address(passphrase) {
            Ok(address) => {
                if address == self.target {
                    println!("\n✅ FOUND! Passphrase: '{}'", passphrase);
                    println!("Address: {}", address);
                    self.found.store(true, Ordering::Relaxed);
                    return true;
                }
            }
            Err(e) => {
                eprintln!("Error deriving address for '{}': {}", passphrase, e);
            }
        }
        
        false
    }
    
    fn recover_parallel(&self, passphrases: Vec<String>) -> Option<String> {
        let start = Instant::now();
        
        // Use Rayon for parallel processing
        let result = passphrases
            .par_iter()
            .find_any(|passphrase| self.check_passphrase(passphrase))
            .cloned();
        
        let elapsed = start.elapsed();
        let total_checked = self.checked_count.load(Ordering::Relaxed);
        let rate = total_checked as f64 / elapsed.as_secs_f64();
        
        println!("\n📊 Performance Statistics:");
        println!("  Total checked: {}", total_checked);
        println!("  Time elapsed: {:.2}s", elapsed.as_secs_f64());
        println!("  Rate: {:.2} passphrases/second", rate);
        
        result
    }
}

fn load_passphrases(filename: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let passphrases: Vec<String> = reader
        .lines()
        .filter_map(Result::ok)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    
    println!("Loaded {} passphrases from {}", passphrases.len(), filename);
    Ok(passphrases)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    // Set thread pool size
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .unwrap();
    }
    
    println!("🔐 BIP39 Passphrase Recovery Tool (Rust Edition)");
    println!("================================================");
    println!("Mnemonic: {} words", args.mnemonic.split_whitespace().count());
    println!("Target: {}", args.target);
    println!("Threads: {}", rayon::current_num_threads());
    println!("Derivation: {}", args.derivation_path);
    println!();
    
    // Load passphrases
    let passphrases = load_passphrases(&args.passphrase_file)?;
    
    // Create recovery instance
    let recovery = BIP39Recovery::new(
        args.mnemonic,
        args.target,
        args.derivation_path,
    );
    
    // Run parallel recovery
    println!("Starting recovery...\n");
    match recovery.recover_parallel(passphrases) {
        Some(passphrase) => {
            println!("\n🎉 SUCCESS! Found passphrase: '{}'", passphrase);
            Ok(())
        }
        None => {
            println!("\n❌ Passphrase not found in the provided list");
            Err("Passphrase not found".into())
        }
    }
}

// Cargo.toml contents:
/*
[package]
name = "bip39-recovery"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4.4", features = ["derive"] }
rayon = "1.8"
ring = "0.17"
bip39 = "2.0"
bitcoin = "0.31"
sha2 = "0.10"
hex = "0.4"

[profile.release]
lto = true
codegen-units = 1
opt-level = 3
target-cpu = "native"  # Optimize for your specific CPU

# For Apple Silicon specifically:
[target.aarch64-apple-darwin]
rustflags = ["-C", "target-cpu=native", "-C", "target-feature=+crc,+aes,+sha2"]
*/