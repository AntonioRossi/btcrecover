use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use btcrecover_rust::tokenlist::{BtcRecoverTokenList, TokenListConfig};
use btcrecover_rust::parallel_generator::ParallelPasswordGenerator;
use std::time::Duration;
use tempfile::NamedTempFile;
use std::io::Write;

fn create_test_tokenlist_file(size: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    
    match size {
        "small" => {
            writeln!(file, "password").unwrap();
            writeln!(file, "123 456 789").unwrap();
            writeln!(file, "hello world").unwrap();
            writeln!(file, "+required").unwrap();
        },
        "medium" => {
            writeln!(file, "+required").unwrap();
            for i in 0..50 {
                writeln!(file, "token{} alt{} var{}", i, i, i).unwrap();
            }
            writeln!(file, "%2d %a %A").unwrap();
            writeln!(file, "prefix suffix").unwrap();
        },
        "large" => {
            writeln!(file, "+required").unwrap();
            for i in 0..500 {
                writeln!(file, "token{} alt{} variant{} option{}", i, i, i, i).unwrap();
            }
            writeln!(file, "%3d %2a %A %n").unwrap();
            writeln!(file, "^start middle end$").unwrap();
            writeln!(file, "prefix1 prefix2 prefix3").unwrap();
            writeln!(file, "suffix1 suffix2 suffix3").unwrap();
        },
        "wildcard_heavy" => {
            writeln!(file, "+required").unwrap();
            writeln!(file, "%4d %3a %2A").unwrap();
            writeln!(file, "%[0-9] %[a-z] %[A-Z]").unwrap();
            writeln!(file, "%2,4d %1,3a").unwrap();
            writeln!(file, "test%2d pass%3a").unwrap();
        },
        _ => panic!("Unknown size: {}", size),
    }
    
    file.flush().unwrap();
    file
}

fn bench_tokenlist_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("tokenlist_parsing");
    
    for size in ["small", "medium", "large"].iter() {
        let file = create_test_tokenlist_file(size);
        
        group.bench_with_input(BenchmarkId::new("rust_sequential", size), size, |b, _| {
            b.iter(|| {
                let config = TokenListConfig::default();
                let tokenlist = BtcRecoverTokenList::from_file(
                    black_box(file.path()), 
                    black_box(config)
                ).unwrap();
                black_box(tokenlist)
            })
        });
    }
    
    group.finish();
}

fn bench_password_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("password_generation");
    group.measurement_time(Duration::from_secs(30));
    
    for size in ["small", "medium", "wildcard_heavy"].iter() {
        let file = create_test_tokenlist_file(size);
        let config = TokenListConfig::default();
        let tokenlist = BtcRecoverTokenList::from_file(file.path(), config.clone()).unwrap();
        
        // Sequential generation
        group.bench_with_input(BenchmarkId::new("rust_sequential", size), size, |b, _| {
            b.iter(|| {
                let passwords: Vec<String> = tokenlist.generate_passwords()
                    .take(black_box(1000))
                    .collect();
                black_box(passwords)
            })
        });
        
        // Parallel generation
        let parallel_gen = ParallelPasswordGenerator::new(
            tokenlist.get_token_list().clone(), 
            config.clone()
        );
        
        group.bench_with_input(BenchmarkId::new("rust_parallel", size), size, |b, _| {
            b.iter(|| {
                let passwords: Vec<String> = parallel_gen.generate_passwords_parallel()
                    .take(black_box(1000))
                    .collect();
                black_box(passwords)
            })
        });
    }
    
    group.finish();
}

fn bench_wildcard_expansion(c: &mut Criterion) {
    let mut group = c.benchmark_group("wildcard_expansion");
    
    let file = create_test_tokenlist_file("wildcard_heavy");
    let config = TokenListConfig::default();
    let tokenlist = BtcRecoverTokenList::from_file(file.path(), config).unwrap();
    
    group.bench_function("rust_wildcard_expansion", |b| {
        b.iter(|| {
            let passwords: Vec<String> = tokenlist.generate_passwords()
                .take(black_box(500))
                .collect();
            black_box(passwords)
        })
    });
    
    group.finish();
}

fn bench_duplicate_checking(c: &mut Criterion) {
    let mut group = c.benchmark_group("duplicate_checking");
    
    let file = create_test_tokenlist_file("medium");
    let mut config = TokenListConfig::default();
    config.enable_duplicate_checking = true;
    let tokenlist = BtcRecoverTokenList::from_file(file.path(), config).unwrap();
    
    group.bench_function("rust_with_duplicates", |b| {
        b.iter(|| {
            let passwords: Vec<String> = tokenlist.generate_passwords()
                .take(black_box(2000))
                .collect();
            black_box(passwords)
        })
    });
    
    group.finish();
}

fn bench_large_tokenlist(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_tokenlist");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);
    
    let file = create_test_tokenlist_file("large");
    let config = TokenListConfig::default();
    let tokenlist = BtcRecoverTokenList::from_file(file.path(), config.clone()).unwrap();
    
    group.bench_function("rust_large_sequential", |b| {
        b.iter(|| {
            let passwords: Vec<String> = tokenlist.generate_passwords()
                .take(black_box(5000))
                .collect();
            black_box(passwords)
        })
    });
    
    let parallel_gen = ParallelPasswordGenerator::new(
        tokenlist.get_token_list().clone(), 
        config
    );
    
    group.bench_function("rust_large_parallel", |b| {
        b.iter(|| {
            let passwords: Vec<String> = parallel_gen.generate_passwords_parallel()
                .take(black_box(5000))
                .collect();
            black_box(passwords)
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_tokenlist_parsing,
    bench_password_generation,
    bench_wildcard_expansion,
    bench_duplicate_checking,
    bench_large_tokenlist
);
criterion_main!(benches);
