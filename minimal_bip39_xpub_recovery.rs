// Minimal BIP39 Passphrase Recovery using Xpub
// This is ALL the code you need!

use std::fs::File;
use std::io::{BufRead, BufReader};
use rayon::prelude::*;
use ring::pbkdf2;
use bitcoin::util::bip32::{ExtendedPrivKey, ExtendedPubKey};
use bitcoin::Network;
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Your 24-word mnemonic phrase
    #[clap(short, long)]
    mnemonic: String,
    
    /// Target xpub to find
    #[clap(short, long)]
    xpub: String,
    
    /// File with passphrase candidates
    #[clap(short, long)]
    file: String,
}

fn check_passphrase(mnemonic: &str, passphrase: &str, target_xpub: &str) -> bool {
    // Step 1: PBKDF2-SHA512 (BIP39 standard)
    let mut seed = [0u8; 64];
    let salt = format!("mnemonic{}", passphrase);
    
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA512,
        std::num::NonZeroU32::new(2048).unwrap(),
        salt.as_bytes(),
        mnemonic.as_bytes(),
        &mut seed,
    );
    
    // Step 2: Generate master private key
    let master = match ExtendedPrivKey::new_master(Network::Bitcoin, &seed) {
        Ok(key) => key,
        Err(_) => return false,
    };
    
    // Step 3: Convert to xpub and compare
    let xpub = ExtendedPubKey::from_priv(&bitcoin::secp256k1::Secp256k1::new(), &master);
    
    xpub.to_string() == target_xpub
}

fn main() {
    let args = Args::parse();
    
    // Load passphrases from file
    let file = File::open(&args.file).expect("Cannot open passphrase file");
    let passphrases: Vec<String> = BufReader::new(file)
        .lines()
        .filter_map(Result::ok)
        .collect();
    
    println!("Checking {} passphrases...", passphrases.len());
    
    // Parallel search using all CPU cores
    let found = passphrases
        .par_iter()
        .find_any(|p| {
            let result = check_passphrase(&args.mnemonic, p, &args.xpub);
            if result {
                println!("✅ FOUND: '{}'", p);
            }
            result
        });
    
    match found {
        Some(passphrase) => {
            println!("\n🎉 Success! Passphrase is: '{}'", passphrase);
        }
        None => {
            println!("\n❌ Passphrase not found in list");
        }
    }
}

/* Cargo.toml:
[package]
name = "bip39-xpub-recovery"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4.4", features = ["derive"] }
rayon = "1.8"
ring = "0.17"
bitcoin = { version = "0.31", features = ["secp256k1"] }

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
*/