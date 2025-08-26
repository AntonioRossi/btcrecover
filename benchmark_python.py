#!/usr/bin/env python3
"""
Python benchmark script to compare with Rust tokenlist performance.
This script uses the existing btcrpass.py functionality to benchmark
tokenlist parsing and password generation.
"""

import time
import tempfile
import os
import sys
import statistics
from contextlib import contextmanager

# Add the btcrecover directory to path to import btcrpass
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

try:
    import btcrpass
except ImportError:
    print("Error: Could not import btcrpass. Make sure you're running from the correct directory.")
    sys.exit(1)

@contextmanager
def timer():
    """Context manager to measure execution time."""
    start = time.perf_counter()
    yield lambda: time.perf_counter() - start
    
def create_test_tokenlist_file(size):
    """Create test tokenlist files matching the Rust benchmark."""
    fd, path = tempfile.mkstemp(suffix='.txt', text=True)
    
    try:
        with os.fdopen(fd, 'w') as f:
            if size == "small":
                f.write("password\n")
                f.write("123 456 789\n")
                f.write("hello world\n")
                f.write("+required\n")
            elif size == "medium":
                f.write("+required\n")
                for i in range(50):
                    f.write(f"token{i} alt{i} var{i}\n")
                f.write("%2d %a %A\n")
                f.write("prefix suffix\n")
            elif size == "large":
                f.write("+required\n")
                for i in range(500):
                    f.write(f"token{i} alt{i} variant{i} option{i}\n")
                f.write("%3d %2a %A %n\n")
                f.write("^start middle end$\n")
                f.write("prefix1 prefix2 prefix3\n")
                f.write("suffix1 suffix2 suffix3\n")
            elif size == "wildcard_heavy":
                f.write("+required\n")
                f.write("%4d %3a %2A\n")
                f.write("%[0-9] %[a-z] %[A-Z]\n")
                f.write("%2,4d %1,3a\n")
                f.write("test%2d pass%3a\n")
        
        return path
    except:
        os.unlink(path)
        raise

def benchmark_tokenlist_parsing():
    """Benchmark tokenlist parsing performance."""
    print("=== Tokenlist Parsing Benchmark ===")
    
    results = {}
    
    for size in ["small", "medium", "large"]:
        print(f"\nTesting {size} tokenlist...")
        
        tokenlist_file = create_test_tokenlist_file(size)
        times = []
        
        try:
            # Warm up
            for _ in range(3):
                btcrpass.parse_tokenlist(tokenlist_file)
            
            # Actual benchmark
            for _ in range(10):
                # Reset global state
                btcrpass.token_lists = []
                btcrpass.has_any_duplicate_tokens = False
                
                with timer() as get_time:
                    btcrpass.parse_tokenlist(tokenlist_file)
                
                times.append(get_time() * 1000)  # Convert to milliseconds
            
            avg_time = statistics.mean(times)
            std_dev = statistics.stdev(times) if len(times) > 1 else 0
            
            results[f"python_parsing_{size}"] = {
                "avg_ms": avg_time,
                "std_ms": std_dev,
                "min_ms": min(times),
                "max_ms": max(times)
            }
            
            print(f"  Average: {avg_time:.2f}ms ± {std_dev:.2f}ms")
            
        finally:
            os.unlink(tokenlist_file)
    
    return results

def benchmark_password_generation():
    """Benchmark password generation performance."""
    print("\n=== Password Generation Benchmark ===")
    
    results = {}
    
    for size in ["small", "medium", "wildcard_heavy"]:
        print(f"\nTesting {size} password generation...")
        
        tokenlist_file = create_test_tokenlist_file(size)
        times = []
        
        try:
            # Parse tokenlist once
            btcrpass.token_lists = []
            btcrpass.parse_tokenlist(tokenlist_file)
            
            # Warm up
            for _ in range(3):
                passwords = []
                for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                    passwords.append(password)
                    if i >= 999:  # Generate 1000 passwords
                        break
            
            # Actual benchmark
            for _ in range(5):
                with timer() as get_time:
                    passwords = []
                    for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                        passwords.append(password)
                        if i >= 999:  # Generate 1000 passwords
                            break
                
                times.append(get_time() * 1000)  # Convert to milliseconds
            
            avg_time = statistics.mean(times)
            std_dev = statistics.stdev(times) if len(times) > 1 else 0
            
            results[f"python_generation_{size}"] = {
                "avg_ms": avg_time,
                "std_ms": std_dev,
                "min_ms": min(times),
                "max_ms": max(times),
                "passwords_generated": len(passwords)
            }
            
            print(f"  Average: {avg_time:.2f}ms ± {std_dev:.2f}ms ({len(passwords)} passwords)")
            
        finally:
            os.unlink(tokenlist_file)
    
    return results

def benchmark_wildcard_expansion():
    """Benchmark wildcard expansion performance."""
    print("\n=== Wildcard Expansion Benchmark ===")
    
    tokenlist_file = create_test_tokenlist_file("wildcard_heavy")
    times = []
    
    try:
        # Parse tokenlist
        btcrpass.token_lists = []
        btcrpass.parse_tokenlist(tokenlist_file)
        
        # Warm up
        for _ in range(3):
            passwords = []
            for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                passwords.append(password)
                if i >= 499:  # Generate 500 passwords
                    break
        
        # Actual benchmark
        for _ in range(5):
            with timer() as get_time:
                passwords = []
                for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                    passwords.append(password)
                    if i >= 499:  # Generate 500 passwords
                        break
            
            times.append(get_time() * 1000)  # Convert to milliseconds
        
        avg_time = statistics.mean(times)
        std_dev = statistics.stdev(times) if len(times) > 1 else 0
        
        results = {
            "python_wildcard_expansion": {
                "avg_ms": avg_time,
                "std_ms": std_dev,
                "min_ms": min(times),
                "max_ms": max(times),
                "passwords_generated": len(passwords)
            }
        }
        
        print(f"  Average: {avg_time:.2f}ms ± {std_dev:.2f}ms ({len(passwords)} passwords)")
        
    finally:
        os.unlink(tokenlist_file)
    
    return results

def benchmark_duplicate_checking():
    """Benchmark duplicate checking performance."""
    print("\n=== Duplicate Checking Benchmark ===")
    
    tokenlist_file = create_test_tokenlist_file("medium")
    times = []
    
    try:
        # Parse tokenlist with duplicate checking enabled
        btcrpass.token_lists = []
        btcrpass.args = type('Args', (), {})()  # Mock args object
        btcrpass.args.no_dupchecks = False  # Enable duplicate checking
        btcrpass.parse_tokenlist(tokenlist_file)
        
        # Warm up
        for _ in range(3):
            passwords = []
            for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                passwords.append(password)
                if i >= 1999:  # Generate 2000 passwords
                    break
        
        # Actual benchmark
        for _ in range(3):
            with timer() as get_time:
                passwords = []
                for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                    passwords.append(password)
                    if i >= 1999:  # Generate 2000 passwords
                        break
            
            times.append(get_time() * 1000)  # Convert to milliseconds
        
        avg_time = statistics.mean(times)
        std_dev = statistics.stdev(times) if len(times) > 1 else 0
        
        results = {
            "python_with_duplicates": {
                "avg_ms": avg_time,
                "std_ms": std_dev,
                "min_ms": min(times),
                "max_ms": max(times),
                "passwords_generated": len(passwords)
            }
        }
        
        print(f"  Average: {avg_time:.2f}ms ± {std_dev:.2f}ms ({len(passwords)} passwords)")
        
    finally:
        os.unlink(tokenlist_file)
    
    return results

def benchmark_large_tokenlist():
    """Benchmark large tokenlist performance."""
    print("\n=== Large Tokenlist Benchmark ===")
    
    tokenlist_file = create_test_tokenlist_file("large")
    times = []
    
    try:
        # Parse tokenlist
        btcrpass.token_lists = []
        btcrpass.parse_tokenlist(tokenlist_file)
        
        # Warm up
        for _ in range(2):
            passwords = []
            for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                passwords.append(password)
                if i >= 4999:  # Generate 5000 passwords
                    break
        
        # Actual benchmark
        for _ in range(3):
            with timer() as get_time:
                passwords = []
                for i, password in enumerate(btcrpass.tokenlist_base_password_generator()):
                    passwords.append(password)
                    if i >= 4999:  # Generate 5000 passwords
                        break
            
            times.append(get_time() * 1000)  # Convert to milliseconds
        
        avg_time = statistics.mean(times)
        std_dev = statistics.stdev(times) if len(times) > 1 else 0
        
        results = {
            "python_large": {
                "avg_ms": avg_time,
                "std_ms": std_dev,
                "min_ms": min(times),
                "max_ms": max(times),
                "passwords_generated": len(passwords)
            }
        }
        
        print(f"  Average: {avg_time:.2f}ms ± {std_dev:.2f}ms ({len(passwords)} passwords)")
        
    finally:
        os.unlink(tokenlist_file)
    
    return results

def main():
    """Run all benchmarks and collect results."""
    print("Python Tokenlist Performance Benchmark")
    print("=" * 50)
    
    all_results = {}
    
    try:
        # Run all benchmarks
        all_results.update(benchmark_tokenlist_parsing())
        all_results.update(benchmark_password_generation())
        all_results.update(benchmark_wildcard_expansion())
        all_results.update(benchmark_duplicate_checking())
        all_results.update(benchmark_large_tokenlist())
        
        # Save results to file
        import json
        with open('python_benchmark_results.json', 'w') as f:
            json.dump(all_results, f, indent=2)
        
        print(f"\n=== Summary ===")
        print(f"Results saved to python_benchmark_results.json")
        
        # Print summary
        for test_name, result in all_results.items():
            print(f"{test_name}: {result['avg_ms']:.2f}ms ± {result['std_ms']:.2f}ms")
            
    except Exception as e:
        print(f"Benchmark failed: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    main()
