# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BTCRecover is an open-source wallet password and seed recovery tool that helps users recover lost cryptocurrency wallet passwords and seed phrases. The project includes both Python implementation (the main tool) and a new Rust implementation for the tokenlist password generation component.

## Architecture

### Python Components (Main Tool)
- **btcrpass.py**: Password recovery module for wallet files
- **seedrecover.py**: Seed phrase recovery for BIP39/44 wallets
- **btcrecover.py**: Main entry point
- **extract-scripts/**: Scripts to extract wallet data for offline recovery
- **lib/**: Third-party libraries and utilities

### Rust Components (Performance-Critical Features)
- **src/lib.rs**: Main library implementation
- **src/tokenlist.rs**: Core tokenlist parsing and generation
- **src/wildcards.rs**: Wildcard expansion logic
- **src/cli.rs**: Command-line interface
- **src/tests.rs**: Comprehensive test suite

## Essential Commands

### Python Development
```bash
# Run all tests
python3 run-all-tests.py

# Run specific test module
python3 -m pytest btcrecover/test/test_passwords.py
python3 -m pytest btcrecover/test/test_seeds.py

# Run compatibility check
python3 compatibility_check.py

# Run benchmarks
python3 benchmark_python.py
```

### Rust Development
```bash
# Build the Rust component
cargo build --release

# Run Rust tests
cargo test

# Run benchmarks
cargo bench

# Run the tokenlist generator
cargo run --bin btcrecover-tokenlist -- -t tokenlist.txt
```

### Password Recovery Examples
```bash
# Basic password recovery
python3 btcrpass.py --wallet wallet.dat --tokenlist tokens.txt

# Seed recovery with address
python3 seedrecover.py --wallet-type bip39 --addrs 1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2 --mnemonic "word1 word2 ..."

# Create address database for faster recovery
python3 create-address-db.py --wallet-type BTC --datadir ./addressdb
```

## Code Architecture

### Token List System
The tokenlist system is the core of BTCRecover's password generation:

1. **Token Parsing**: Reads token files with special syntax:
   - `+` prefix: Required tokens
   - `^` prefix: Position anchors  
   - `%d`, `%a`, etc.: Wildcard expansion
   - Multiple tokens per line: Mutual exclusion

2. **Password Generation**: Combines tokens according to rules:
   - Respects min/max token counts
   - Handles required tokens
   - Applies position constraints
   - Expands wildcards

3. **Rust Implementation**: The new Rust version (`src/`) provides:
   - Faster tokenlist parsing
   - Parallel password generation
   - Memory-efficient wildcard expansion
   - CLI compatibility with Python version

### Recovery Workflow
1. User provides wallet file or seed phrase fragment
2. System generates password/seed candidates from tokenlist
3. Each candidate is tested against the wallet/addresses
4. GPU acceleration available for certain wallet types
5. Results reported with found password/seed

### Key Design Patterns
- **Generator Pattern**: Password/seed generation uses Python generators for memory efficiency
- **Plugin Architecture**: Support for multiple wallet types through modular design
- **Offline Recovery**: Extract scripts enable recovery without exposing full wallet
- **GPU Acceleration**: OpenCL kernels for performance-critical operations

## Testing Strategy

### Python Tests
- Located in `btcrecover/test/`
- Uses unittest framework
- Tests both password and seed recovery
- Includes test wallets in `btcrecover/test/test-wallets/`

### Rust Tests
- Integrated in `src/tests.rs`
- Tests tokenlist parsing, wildcard expansion, and generation
- Compatibility tests ensure Rust matches Python behavior
- Benchmark tests in `benches/`

### Running a Single Test
```bash
# Python - run specific test
python3 -m unittest btcrecover.test.test_passwords.TestPasswordRecovery.test_bitcoin_core

# Rust - run specific test
cargo test test_wildcard_documentation_examples
```

## Important Implementation Notes

1. **Character Encoding**: The system handles Unicode passwords - set `BTCR_CHAR_MODE=unicode` environment variable

2. **Wildcard Performance**: Wildcard expansion can be computationally expensive - the Rust implementation provides significant speedups

3. **Address Databases**: For seed recovery without known addresses, create address databases using `create-address-db.py`

4. **GPU Acceleration**: Available for Bitcoin Core, Electrum, and BIP39 passphrases via OpenCL

5. **Token File Format**: Token files support comments (#), special prefixes (+, ^), wildcards (%), and multiple tokens per line for mutual exclusion

6. **Rust/Python Interop**: The Rust tokenlist generator can be used as a drop-in replacement for the Python version's tokenlist functionality

## Development Workflow

1. **Before Making Changes**: Run tests to ensure baseline functionality
2. **Testing Changes**: Use appropriate test files in `btcrecover/test/test-listfiles/` and `rust vs python test examples/`
3. **Benchmarking**: Compare Python vs Rust performance using `benchmark_python.py` and `cargo bench`
4. **Compatibility**: Ensure Rust implementation matches Python behavior for all tokenlist features