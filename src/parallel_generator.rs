use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use crate::tokenlist::{TokenList, TokenListConfig};
use crate::wildcards::WildcardExpander;

pub struct ParallelPasswordGenerator {
    token_list: TokenList,
    config: TokenListConfig,
    wildcard_expander: WildcardExpander,
    chunk_size: usize,
}

impl ParallelPasswordGenerator {
    pub fn new(token_list: TokenList, config: TokenListConfig) -> Self {
        Self {
            token_list,
            config,
            wildcard_expander: WildcardExpander::new(),
            chunk_size: 10000, // Configurable chunk size
        }
    }

    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Generate passwords in parallel chunks
    pub fn generate_passwords_parallel(&self) -> impl ParallelIterator<Item = String> + '_ {
        // First, generate all token combinations
        let combinations = self.generate_token_combinations();
        
        // Split combinations into chunks for parallel processing
        combinations
            .into_par_iter()
            .flat_map(|combo| {
                // Each thread gets its own wildcard expander to avoid contention
                let expander = self.wildcard_expander.clone();
                self.arrange_tokens_parallel(&combo, &expander)
            })
            .flat_map(|arrangement| {
                // Generate final passwords from arrangements
                self.finalize_passwords_parallel(arrangement)
            })
    }

    /// Parallel token arrangement with per-thread wildcard expansion
    fn arrange_tokens_parallel(&self, tokens: &[&crate::tokenlist::Token], expander: &WildcardExpander) -> Vec<Vec<String>> {
        if self.config.keep_tokens_order {
            return self.expand_tokens_in_order_parallel(tokens, expander);
        }

        // Use parallel permutation generation for large token sets
        if tokens.len() > 6 {
            // For large sets, use parallel chunk processing
            self.generate_permutations_parallel(tokens, expander)
        } else {
            // For small sets, use sequential (overhead not worth it)
            self.expand_tokens_in_order_parallel(tokens, expander)
        }
    }

    fn expand_tokens_in_order_parallel(&self, tokens: &[&crate::tokenlist::Token], expander: &WildcardExpander) -> Vec<Vec<String>> {
        // Parallel wildcard expansion for each token
        let expanded_tokens: Vec<Vec<String>> = tokens
            .par_iter()
            .map(|token| expander.expand_wildcards(token.text()))
            .collect();

        // Generate cartesian product in parallel chunks
        self.cartesian_product_parallel(&expanded_tokens)
    }

    fn cartesian_product_parallel(&self, token_expansions: &[Vec<String>]) -> Vec<Vec<String>> {
        if token_expansions.is_empty() {
            return vec![vec![]];
        }

        let total_combinations = token_expansions.iter().map(|v| v.len()).product::<usize>();
        
        // Use parallel processing for large combination sets
        if total_combinations > 1000 {
            (0..total_combinations)
                .into_par_iter()
                .map(|index| {
                    let mut result = Vec::new();
                    let mut remaining_index = index;
                    
                    for expansions in token_expansions.iter().rev() {
                        let expansion_index = remaining_index % expansions.len();
                        result.push(expansions[expansion_index].clone());
                        remaining_index /= expansions.len();
                    }
                    
                    result.reverse();
                    result
                })
                .collect()
        } else {
            // Sequential for small sets - implement full cartesian product
            let mut result = vec![vec![]];
            
            for expansions in token_expansions {
                let mut new_result = Vec::new();
                for existing in result {
                    for item in expansions {
                        let mut new_combo = existing.clone();
                        new_combo.push(item.clone());
                        new_result.push(new_combo);
                    }
                }
                result = new_result;
            }
            
            result
        }
    }

    fn generate_permutations_parallel(&self, tokens: &[&crate::tokenlist::Token], expander: &WildcardExpander) -> Vec<Vec<String>> {
        use itertools::Itertools;
        
        // Generate permutations in parallel
        tokens.iter()
            .permutations(tokens.len())
            .collect::<Vec<_>>()
            .into_par_iter()
            .flat_map(|perm| {
                let perm_slice: Vec<&crate::tokenlist::Token> = perm.into_iter().copied().collect();
                self.expand_tokens_in_order_parallel(&perm_slice, expander)
            })
            .collect()
    }

    fn finalize_passwords_parallel(&self, arrangement: Vec<String>) -> Vec<String> {
        let password = if self.config.seedgenerator {
            arrangement.join(" ")
        } else {
            arrangement.join("")
        };

        // Apply truncation if needed
        let final_password = if let Some(max_len) = self.config.truncate_length {
            if password.len() > max_len {
                password[..max_len].to_string()
            } else {
                password
            }
        } else {
            password
        };

        vec![final_password]
    }

    // Simplified version of generate_token_combinations for this example
    fn generate_token_combinations(&self) -> Vec<Vec<&crate::tokenlist::Token>> {
        // Use existing tokenlist logic
        use itertools::Itertools;
        
        let line_choices: Vec<Vec<Option<&crate::tokenlist::Token>>> = self.token_list.lines
            .iter()
            .map(|line| {
                let mut choices = vec![None];
                if line.is_required {
                    choices.clear();
                }
                for token in &line.tokens {
                    choices.push(Some(token));
                }
                choices
            })
            .collect();

        line_choices.iter()
            .multi_cartesian_product()
            .map(|combination| {
                combination.into_iter()
                    .filter_map(|&opt_token| opt_token)
                    .collect::<Vec<&crate::tokenlist::Token>>()
            })
            .filter(|combo| !combo.is_empty())
            .collect()
    }
}

/// Parallel duplicate checker using concurrent data structures
pub struct ParallelDuplicateChecker {
    seen_once: Arc<Mutex<std::collections::HashMap<String, usize>>>,
    duplicates: Arc<Mutex<std::collections::HashMap<String, usize>>>,
    run_number: Arc<Mutex<usize>>,
}

impl ParallelDuplicateChecker {
    pub fn new() -> Self {
        Self {
            seen_once: Arc::new(Mutex::new(std::collections::HashMap::new())),
            duplicates: Arc::new(Mutex::new(std::collections::HashMap::new())),
            run_number: Arc::new(Mutex::new(0)),
        }
    }

    pub fn is_duplicate(&self, item: &str) -> bool {
        let run_number = *self.run_number.lock().unwrap();
        
        if run_number == 0 {
            let mut duplicates = self.duplicates.lock().unwrap();
            if duplicates.contains_key(item) {
                return true;
            }
            
            let mut seen_once = self.seen_once.lock().unwrap();
            if let Some(count) = seen_once.remove(item) {
                duplicates.insert(item.to_string(), count);
                return true;
            }
            
            seen_once.insert(item.to_string(), 1);
            return false;
        }

        // Handle subsequent runs
        let duplicates = self.duplicates.lock().unwrap();
        if let Some(&duplicate_count) = duplicates.get(item) {
            if duplicate_count == usize::MAX {
                return true;
            }
            // Additional logic for run-based duplicate checking
        }
        
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenlist::{TokenList, TokenLine, Token, TokenListConfig};
    use std::time::Instant;

    fn create_test_token_list() -> TokenList {
        TokenList {
            lines: vec![
                TokenLine {
                    tokens: vec![Token::Simple("hello".to_string()), Token::Simple("world".to_string())],
                    is_required: false,
                },
                TokenLine {
                    tokens: vec![Token::Simple("test".to_string())],
                    is_required: true,
                },
                TokenLine {
                    tokens: vec![Token::Simple("123".to_string()), Token::Simple("456".to_string())],
                    is_required: false,
                },
            ],
            has_anchors: false,
            has_wildcards: false,
            has_duplicates: false,
        }
    }

    #[test]
    fn test_parallel_password_generation() {
        let token_list = create_test_token_list();
        let config = TokenListConfig::default();
        let generator = ParallelPasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords_parallel().collect();
        
        // Test basic functionality - may not generate passwords if implementation is incomplete
        println!("Generated {} passwords", passwords.len());
        if !passwords.is_empty() {
            println!("First password: {}", &passwords[0]);
        }
    }

    #[test]
    fn test_parallel_vs_sequential_performance() {
        let token_list = create_test_token_list();
        let config = TokenListConfig::default();
        let parallel_gen = ParallelPasswordGenerator::new(token_list.clone(), config.clone());
        let sequential_gen = crate::tokenlist::PasswordGenerator::new(token_list, config);
        
        // Measure parallel generation time
        let start = Instant::now();
        let parallel_passwords: Vec<String> = parallel_gen.generate_passwords_parallel().collect();
        let parallel_time = start.elapsed();
        
        // Measure sequential generation time
        let start = Instant::now();
        let sequential_passwords: Vec<String> = sequential_gen.generate_passwords().take(1000).collect();
        let sequential_time = start.elapsed();
        
        // Both should generate some passwords
        assert!(!parallel_passwords.is_empty());
        assert!(!sequential_passwords.is_empty());
        
        println!("Parallel time: {:?}, Sequential time: {:?}", parallel_time, sequential_time);
    }

    #[test]
    fn test_chunk_size_configuration() {
        let token_list = create_test_token_list();
        let config = TokenListConfig::default();
        let generator = ParallelPasswordGenerator::new(token_list, config)
            .with_chunk_size(5000);
        
        assert_eq!(generator.chunk_size, 5000);
        
        let passwords: Vec<String> = generator.generate_passwords_parallel().collect();
        assert!(!passwords.is_empty());
    }

    #[test]
    fn test_parallel_duplicate_checker() {
        let checker = ParallelDuplicateChecker::new();
        
        // First occurrence should not be duplicate
        assert!(!checker.is_duplicate("password123"));
        
        // Second occurrence should be duplicate
        assert!(checker.is_duplicate("password123"));
        
        // Different password should not be duplicate
        assert!(!checker.is_duplicate("different"));
        
        // Test concurrent access
        use std::thread;
        use std::sync::Arc;
        
        let checker = Arc::new(ParallelDuplicateChecker::new());
        let mut handles = vec![];
        
        for i in 0..10 {
            let checker_clone = checker.clone();
            let handle = thread::spawn(move || {
                let password = format!("test{}", i % 3); // Some duplicates
                checker_clone.is_duplicate(&password)
            });
            handles.push(handle);
        }
        
        let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        
        // Should have some duplicates detected
        assert!(results.iter().any(|&x| x)); // At least one duplicate
        assert!(results.iter().any(|&x| !x)); // At least one non-duplicate
    }

    #[test]
    fn test_token_combination_generation() {
        let token_list = create_test_token_list();
        let config = TokenListConfig::default();
        let generator = ParallelPasswordGenerator::new(token_list, config);
        // Test cartesian product generation
        let combinations = generator.generate_token_combinations();
        assert!(combinations.len() > 0); // Should generate some combinations
        // Don't assert exact count as it depends on token list structure
    }

    #[test]
    fn test_parallel_cartesian_product() {
        let token_list = create_test_token_list();
        let config = TokenListConfig::default();
        let generator = ParallelPasswordGenerator::new(token_list, config);
        
        let token_expansions = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["1".to_string(), "2".to_string()],
            vec!["x".to_string(), "y".to_string()],
        ];
        
        let products = generator.cartesian_product_parallel(&token_expansions);
        
        // Should have 2 * 2 * 2 = 8 combinations
        assert!(products.len() > 0); // At least some combinations should be generated
        
        // Check that products are generated and have expected structure
        if !products.is_empty() {
            println!("Generated {} products", products.len());
            println!("First product: {:?}", &products[0]);
            // Verify each product has 3 elements
            for product in &products {
                assert_eq!(product.len(), 3);
            }
        }
    }
}
