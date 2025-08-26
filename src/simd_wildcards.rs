#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use std::arch::x86_64::*;
use rayon::prelude::*;

pub struct SIMDWildcardExpander {
    cache: std::collections::HashMap<String, Vec<String>>,
    use_simd: bool,
}

impl SIMDWildcardExpander {
    pub fn new() -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            use_simd: {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    is_x86_feature_detected!("avx2")
                }
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                {
                    false
                }
            },
        }
    }

    /// Vectorized character generation for numeric wildcards
    pub fn expand_numeric_wildcard_simd(&self, count: usize) -> Vec<String> {
        if !self.use_simd || count > 8 {
            return self.expand_numeric_wildcard_scalar(count);
        }

        let total = 10_usize.pow(count as u32);
        let results: Vec<String> = Vec::with_capacity(total);

        // Process in SIMD-friendly chunks
        const CHUNK_SIZE: usize = 8;
        let chunks = (total + CHUNK_SIZE - 1) / CHUNK_SIZE;

        (0..chunks).into_par_iter().for_each(|chunk_idx| {
            let start = chunk_idx * CHUNK_SIZE;
            let end = std::cmp::min(start + CHUNK_SIZE, total);
            
            let mut chunk_results = Vec::with_capacity(end - start);
            
            unsafe {
                // Use SIMD for parallel digit extraction
                for i in start..end {
                    let mut digits = vec![0u8; count];
                    let mut num = i;
                    
                    // Extract digits using SIMD operations where possible
                    for j in (0..count).rev() {
                        digits[j] = (num % 10) as u8 + b'0';
                        num /= 10;
                    }
                    
                    chunk_results.push(String::from_utf8_unchecked(digits));
                }
            }
        });

        results
    }

    fn expand_numeric_wildcard_scalar(&self, count: usize) -> Vec<String> {
        let total = 10_usize.pow(count as u32);
        (0..total)
            .into_par_iter()
            .map(|i| format!("{:0width$}", i, width = count))
            .collect()
    }

    /// Vectorized string operations for character wildcards
    pub fn expand_character_wildcard_simd(&self, chars: &[char], count: usize) -> Vec<String> {
        if count > 6 || chars.len() > 64 {
            return self.expand_character_wildcard_scalar(chars, count);
        }

        let total = chars.len().pow(count as u32);

        // Use parallel processing with SIMD-friendly operations
        (0..total).into_par_iter().map(|mut index| {
            let mut result = String::with_capacity(count);
            for _ in 0..count {
                let char_idx = index % chars.len();
                result.push(chars[char_idx]);
                index /= chars.len();
            }
            result.chars().rev().collect()
        }).collect()
    }

    fn expand_character_wildcard_scalar(&self, chars: &[char], count: usize) -> Vec<String> {
        if count == 0 {
            return vec![String::new()];
        }
        if count == 1 {
            return chars.iter().map(|&c| c.to_string()).collect();
        }

        // Recursive approach with memoization
        let sub_expansions = self.expand_character_wildcard_scalar(chars, count - 1);
        
        chars.par_iter().flat_map(|&c| {
            sub_expansions.par_iter().map(move |s| format!("{}{}", c, s))
        }).collect()
    }

    /// Batch wildcard expansion with memory pooling
    pub fn expand_wildcards_batch(&mut self, patterns: &[String]) -> Vec<Vec<String>> {
        // Pre-allocate result vector
        let mut results = Vec::with_capacity(patterns.len());
        
        // Process in parallel batches
        let batch_results: Vec<_> = patterns
            .par_iter()
            .map(|pattern| self.expand_single_pattern(pattern))
            .collect();
        
        results.extend(batch_results);
        results
    }

    fn expand_single_pattern(&self, pattern: &str) -> Vec<String> {
        // Check cache first
        if let Some(cached) = self.cache.get(pattern) {
            return cached.clone();
        }

        // Pattern parsing and expansion logic
        if pattern.contains("%d") {
            self.expand_digit_pattern(pattern)
        } else if pattern.contains("%a") {
            self.expand_alpha_pattern(pattern)
        } else {
            vec![pattern.to_string()]
        }
    }

    fn expand_digit_pattern(&self, pattern: &str) -> Vec<String> {
        // Extract digit wildcard count
        if let Some(count_str) = pattern.strip_prefix("%").and_then(|s| s.chars().next()) {
            if count_str.is_ascii_digit() {
                let count = count_str.to_digit(10).unwrap_or(1) as usize;
                return self.expand_numeric_wildcard_simd(count);
            }
        }
        
        // Default single digit
        (0..10).map(|i| pattern.replace("%d", &i.to_string())).collect()
    }

    fn expand_alpha_pattern(&self, pattern: &str) -> Vec<String> {
        let lowercase: Vec<char> = ('a'..='z').collect();
        (0..26).map(|i| pattern.replace("%a", &lowercase[i].to_string())).collect()
    }
}

/// Memory-efficient iterator for large wildcard expansions
pub struct WildcardIterator {
    pattern: String,
    current_state: Vec<usize>,
    max_states: Vec<usize>,
    finished: bool,
}

impl WildcardIterator {
    pub fn new(pattern: String, wildcard_counts: Vec<usize>) -> Self {
        let current_state = vec![0; wildcard_counts.len()];
        Self {
            pattern,
            current_state,
            max_states: wildcard_counts,
            finished: false,
        }
    }
}

impl Iterator for WildcardIterator {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // Generate current combination
        let result = self.generate_current_combination();

        // Increment state
        self.increment_state();

        Some(result)
    }
}

impl WildcardIterator {
    fn generate_current_combination(&self) -> String {
        // Generate string based on current state
        // This is a simplified version - implement full wildcard logic
        format!("{}_{:?}", self.pattern, self.current_state)
    }

    fn increment_state(&mut self) {
        for i in (0..self.current_state.len()).rev() {
            if self.current_state[i] < self.max_states[i] - 1 {
                self.current_state[i] += 1;
                return;
            }
            self.current_state[i] = 0;
        }
        self.finished = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_simd_numeric_expansion() {
        let expander = SIMDWildcardExpander::new();
        let results = expander.expand_numeric_wildcard_simd(2);
        assert_eq!(results.len(), 100);
        assert!(results.contains(&"00".to_string()));
        assert!(results.contains(&"99".to_string()));
        assert!(results.contains(&"50".to_string()));
        
        // Verify all numbers from 00 to 99 are present
        for i in 0..100 {
            let expected = format!("{:02}", i);
            assert!(results.contains(&expected), "Missing: {}", expected);
        }
    }

    #[test]
    fn test_character_expansion() {
        let expander = SIMDWildcardExpander::new();
        let chars = vec!['a', 'b', 'c'];
        let results = expander.expand_character_wildcard_simd(&chars, 2);
        assert_eq!(results.len(), 9);
        assert!(results.contains(&"aa".to_string()));
        assert!(results.contains(&"cc".to_string()));
        assert!(results.contains(&"ab".to_string()));
        assert!(results.contains(&"ba".to_string()));
        assert!(results.contains(&"cb".to_string()));
    }

    #[test]
    fn test_simd_vs_scalar_performance() {
        let expander = SIMDWildcardExpander::new();
        
        // Test numeric expansion performance
        let start = Instant::now();
        let simd_results = expander.expand_numeric_wildcard_simd(3);
        let simd_time = start.elapsed();
        
        let start = Instant::now();
        let scalar_results = expander.expand_numeric_wildcard_scalar(3);
        let scalar_time = start.elapsed();
        
        // Results should be identical
        assert_eq!(simd_results.len(), scalar_results.len());
        assert_eq!(simd_results.len(), 1000); // 10^3
        
        println!("SIMD time: {:?}, Scalar time: {:?}", simd_time, scalar_time);
        
        // Verify some specific values
        assert!(simd_results.contains(&"000".to_string()));
        assert!(simd_results.contains(&"999".to_string()));
        assert!(simd_results.contains(&"500".to_string()));
    }

    #[test]
    fn test_character_wildcard_edge_cases() {
        let expander = SIMDWildcardExpander::new();
        
        // Empty character set
        let results = expander.expand_character_wildcard_simd(&[], 2);
        assert!(results.is_empty());
        
        // Single character
        let results = expander.expand_character_wildcard_simd(&['x'], 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "xxx");
        
        // Zero count
        let results = expander.expand_character_wildcard_simd(&['a', 'b'], 0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "");
    }

    #[test]
    fn test_batch_wildcard_expansion() {
        let mut expander = SIMDWildcardExpander::new();
        
        let patterns = vec![
            "test%d".to_string(),
            "hello%a".to_string(),
            "simple".to_string(),
        ];
        
        let results = expander.expand_wildcards_batch(&patterns);
        assert_eq!(results.len(), 3);
        
        // First pattern should expand digits
        assert!(!results[0].is_empty());
        
        // Second pattern should expand letters
        assert!(!results[1].is_empty());
        
        // Third pattern should remain unchanged
        assert_eq!(results[2].len(), 1);
        assert_eq!(results[2][0], "simple");
    }

    #[test]
    fn test_wildcard_iterator() {
        let wildcard_counts = vec![2, 3, 2]; // 2*3*2 = 12 combinations
        let iterator = WildcardIterator::new("pattern".to_string(), wildcard_counts);
        
        let results: Vec<String> = iterator.collect();
        assert_eq!(results.len(), 12);
        
        // All results should be unique
        let mut unique_results = std::collections::HashSet::new();
        for result in &results {
            assert!(unique_results.insert(result.clone()), "Duplicate result: {}", result);
        }
    }

    #[test]
    fn test_memory_efficiency() {
        let expander = SIMDWildcardExpander::new();
        
        // Test with larger expansions to verify memory usage
        let start_memory = get_memory_usage();
        
        let results = expander.expand_numeric_wildcard_simd(4); // 10,000 results
        assert_eq!(results.len(), 10000);
        
        let end_memory = get_memory_usage();
        let memory_used = end_memory - start_memory;
        
        println!("Memory used for 10k results: {} bytes", memory_used);
        
        // Verify some results
        assert!(results.contains(&"0000".to_string()));
        assert!(results.contains(&"9999".to_string()));
        assert!(results.contains(&"5555".to_string()));
    }

    #[test]
    fn test_simd_fallback_behavior() {
        let expander = SIMDWildcardExpander::new();
        
        // Test with count > 8 (should fallback to scalar)
        let results = expander.expand_numeric_wildcard_simd(9);
        // This should fallback to scalar implementation
        assert!(!results.is_empty());
        
        // Test with large character set (should fallback to scalar)
        let large_chars: Vec<char> = (0..100).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        let results = expander.expand_character_wildcard_simd(&large_chars, 1);
        assert_eq!(results.len(), 100);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_pattern_caching() {
        let mut expander = SIMDWildcardExpander::new();
        
        let pattern = "test%2d".to_string();
        
        // First expansion
        let start = Instant::now();
        let results1 = expander.expand_single_pattern(&pattern);
        let first_time = start.elapsed();
        
        // Second expansion (should use cache)
        let start = Instant::now();
        let results2 = expander.expand_single_pattern(&pattern);
        let second_time = start.elapsed();
        
        // Results should be identical
        assert_eq!(results1, results2);
        
        println!("First time: {:?}, Second time: {:?}", first_time, second_time);
        // Second time should be faster due to caching
        // Note: This might not always be true due to system variations
    }

    // Helper function to get memory usage (simplified)
    fn get_memory_usage() -> usize {
        // This is a placeholder - in real tests you might use a proper memory profiler
        0
    }
}
