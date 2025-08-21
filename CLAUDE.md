# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

BTCRecover is an open-source cryptocurrency wallet password and seed recovery tool written in Python. It supports recovery for Bitcoin, Ethereum, and many other cryptocurrencies across various wallet formats. The tool uses brute-force and smart recovery techniques with support for GPU acceleration.

## Essential Commands

### Installation
```bash
# Install base requirements
pip3 install -r requirements.txt

# Install full requirements (includes GPU support and all wallet types)
pip3 install -r requirements-full.txt

# Check RIPEMD160 support (important for performance)
python check_ripemd160.py
```

### Running Tests
```bash
# Run all tests
python run-all-tests.py

# Run password recovery tests
python -m pytest btcrecover/test/test_passwords.py

# Run seed recovery tests  
python -m pytest btcrecover/test/test_seeds.py

# Test specific wallet recovery
python btcrecover.py --wallet btcrecover/test/test-wallets/[wallet-file] --passwordlist docs/Usage_Examples/common_passwordlist.txt
```

### Main Recovery Tools
```bash
# Password recovery
python btcrecover.py --wallet [wallet-file] --tokenlist [tokenlist-file]

# Seed phrase recovery
python seedrecover.py --wallet-type [type] --addrs [address] --mnemonic-length [12/24]

# Batch seed recovery
python seedrecover_batch.py --batch-file [batch-file]

# Create address database
python create-address-db.py --dblength [size] --inputlist [address-list]

# Check address database
python check-address-db.py --dbfile [db-file]
```

## Architecture

### Core Modules
- **btcrecover/btcrpass.py**: Main password recovery engine with wallet detection and multi-threading support
- **btcrecover/btcrseed.py**: Seed phrase recovery engine supporting BIP39/SLIP39 mnemonics
- **btcrecover/addressset.py**: Address database functionality for faster searches
- **btcrecover/opencl_helpers.py**: GPU acceleration support via OpenCL

### Wallet Support Structure
- **btcrecover/test/test-wallets/**: Test wallet files for various formats
- **extract-scripts/**: Scripts to extract password hashes from wallet files for offline recovery
- **btcrecover/wordlists/**: BIP39 wordlists in multiple languages

### Key Features Implementation
- **Multi-threading**: Uses Python's multiprocessing module for parallel password testing
- **GPU Acceleration**: OpenCL kernels in `btcrecover/opencl/` for SHA512, scrypt operations
- **Typo Simulation**: Smart password mutations based on common typing errors
- **Token Lists**: Flexible password generation using token patterns and wildcards

## Development Workflow

### Adding New Wallet Support
1. Add wallet detection logic in `btcrecover/btcrpass.py`
2. Implement password verification method
3. Add test wallet file in `btcrecover/test/test-wallets/`
4. Create extraction script in `extract-scripts/` if needed
5. Add test cases in `btcrecover/test/test_passwords.py`

### Performance Optimization
- Use `--threads` parameter to control CPU parallelization
- Enable GPU with `--enable-opencl` for supported operations
- Use address databases for seed recovery without known addresses
- Utilize `--autosave` to resume interrupted recovery sessions

### Testing Approach
- Test files use known passwords/seeds for verification
- Common test password: "btcr-test-password"
- Test token lists in `btcrecover/test/test-listfiles/`
- Benchmark lists in `benchmark-lists/` for performance testing

## Important Considerations

### Security Notes
- This tool is for legitimate recovery of owned wallets only
- Extract scripts allow offline recovery without exposing full wallet data
- Never share wallet files or extracted data publicly

### Platform Specifics
- Windows: Use `python` command, limit of 64 threads
- Linux/Mac: Use `python3` command, may need to enable OpenSSL legacy provider for RIPEMD160
- Large CPU systems (>64 cores): Best to use Linux

### Common Issues and Solutions
- **Missing ripemd160**: Enable OpenSSL legacy provider or use built-in Python implementation
- **Memory issues**: Use `--max-tokens` to limit password complexity
- **Slow performance**: Enable GPU acceleration or use more threads
- **Unicode errors**: Ensure terminal supports UTF-8 encoding