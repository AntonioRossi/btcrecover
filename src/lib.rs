//! # BTCRecover Rust Library
//!
//! A high-performance Rust implementation of BTCRecover's tokenlist functionality.
//! This library provides password generation from token files with support for
//! wildcards, anchors, and various constraints.
//!
//! ## Quick Start
//!
//! ```rust
//! use btcrecover_rust::{BtcRecoverTokenList, TokenListConfig};
//!
//! let config = TokenListConfig::default();
//! let generator = BtcRecoverTokenList::new(config);
//!
//! let token_content = "hello\nworld\ntest%d";
//! let passwords = generator.generate_from_string(token_content)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::path::Path;
use thiserror::Error;

pub mod tokenlist;
pub mod cli;
#[cfg(test)]
pub mod tests;
pub mod wildcards;

pub use cli::CliApp;

// Re-export core types for library users
pub use tokenlist::{
    TokenList, TokenLine, Token, AnchoredToken, AnchorType, 
    TokenListConfig, TokenListParser, PasswordGenerator
};
pub use wildcards::WildcardExpander;

// Re-export CLI for binary usage

/// Main error type for the BTCRecover library
#[derive(Error, Debug)]
pub enum BtcRecoverError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Wildcard error: {0}")]
    Wildcard(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Generic error: {0}")]
    Generic(#[from] Box<dyn std::error::Error>),
}

/// Type alias for Results using BtcRecoverError
pub type Result<T> = std::result::Result<T, BtcRecoverError>;

/// Main entry point for the BTCRecover tokenlist functionality
/// 
/// This struct provides a high-level API for generating passwords from token files.
/// It encapsulates the parsing, wildcard expansion, and password generation logic.
/// 
/// # Examples
/// 
/// ```rust
/// use btcrecover_rust::{BtcRecoverTokenList, TokenListConfig};
/// 
/// let config = TokenListConfig {
///     min_tokens: 1,
///     max_tokens: 3,
///     ..Default::default()
/// };
/// let generator = BtcRecoverTokenList::new(config);
/// ```
pub struct BtcRecoverTokenList {
    config: TokenListConfig,
    wildcard_expander: WildcardExpander,
}

impl BtcRecoverTokenList {
    /// Create a new BTCRecover tokenlist generator with the given configuration
    pub fn new(config: TokenListConfig) -> Self {
        Self {
            config,
            wildcard_expander: WildcardExpander::new(),
        }
    }

    /// Create a new generator with default configuration
    pub fn default() -> Self {
        Self::new(TokenListConfig::default())
    }

    /// Generate passwords from a file and return an iterator
    /// 
    /// This is the most memory-efficient way to generate passwords as it doesn't
    /// collect all passwords into memory at once.
    pub fn generate_from_file_iter<P: AsRef<Path>>(&self, path: P) -> Result<PasswordGenerator> {
        let parser = TokenListParser::new(self.config.clone());
        let token_list = parser.parse_file(path)?;
        Ok(PasswordGenerator::new(token_list, self.config.clone())
            .with_wildcard_expander(self.wildcard_expander.clone()))
    }

    /// Generate passwords from a string and return an iterator
    pub fn generate_from_string_iter(&self, content: &str) -> Result<PasswordGenerator> {
        use std::io::Cursor;
        let parser = TokenListParser::new(self.config.clone());
        let cursor = Cursor::new(content);
        let token_list = parser.parse_reader(cursor)?;
        Ok(PasswordGenerator::new(token_list, self.config.clone())
            .with_wildcard_expander(self.wildcard_expander.clone()))
    }

    /// Generate all passwords from a file and collect them into a Vec
    /// 
    /// Warning: This can use significant memory for large token lists.
    /// Consider using `generate_from_file_iter` for large datasets.
    pub fn generate_from_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<String>> {
        Ok(self.generate_from_file_iter(path)?.generate_passwords().collect())
    }

    /// Generate all passwords from a string and collect them into a Vec
    pub fn generate_from_string(&self, content: &str) -> Result<Vec<String>> {
        Ok(self.generate_from_string_iter(content)?.generate_passwords().collect())
    }

    /// Add custom wildcard characters
    /// 
    /// # Example
    /// 
    /// ```rust
    /// # use btcrecover_rust::BtcRecoverTokenList;
    /// let mut generator = BtcRecoverTokenList::default();
    /// generator.add_custom_wildcard('x', vec!["alpha".to_string(), "beta".to_string()]);
    /// ```
    pub fn add_custom_wildcard(&mut self, wildcard_char: char, values: Vec<String>) {
        self.wildcard_expander.add_custom_wildcard(wildcard_char, values);
    }

    /// Add multiple custom wildcards at once
    pub fn add_custom_wildcards(&mut self, wildcards: Vec<(char, Vec<String>)>) {
        for (wildcard_char, values) in wildcards {
            self.wildcard_expander.add_custom_wildcard(wildcard_char, values);
        }
    }

    /// Builder method to add custom wildcards
    pub fn with_custom_wildcards(mut self, wildcards: Vec<(char, Vec<String>)>) -> Self {
        self.add_custom_wildcards(wildcards);
        self
    }

    /// Expand wildcards in a given text without generating full passwords
    pub fn expand_wildcards(&self, text: &str) -> Vec<String> {
        self.wildcard_expander.expand_wildcards(text)
    }

    /// Get the current configuration
    pub fn config(&self) -> &TokenListConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: TokenListConfig) {
        self.config = config;
    }

    /// Parse a token file and return the parsed TokenList structure
    /// 
    /// This is useful for inspecting the token structure before generating passwords.
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<TokenList> {
        let parser = TokenListParser::new(self.config.clone());
        Ok(parser.parse_file(path)?)
    }

    /// Parse token content from a string and return the parsed TokenList structure
    pub fn parse_string(&self, content: &str) -> Result<TokenList> {
        use std::io::Cursor;
        let parser = TokenListParser::new(self.config.clone());
        let cursor = Cursor::new(content);
        Ok(parser.parse_reader(cursor)?)
    }
}

// PasswordGenerator is re-exported from tokenlist module
// No need to redefine it here

/// Convenience function to quickly generate passwords from a file with default settings
/// 
/// # Example
/// 
/// ```rust,no_run
/// use btcrecover_rust::generate_passwords_from_file;
/// 
/// let passwords = generate_passwords_from_file("tokens.txt")?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn generate_passwords_from_file<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    BtcRecoverTokenList::default().generate_from_file(path)
}

/// Convenience function to quickly generate passwords from a string with default settings
pub fn generate_passwords_from_string(content: &str) -> Result<Vec<String>> {
    BtcRecoverTokenList::default().generate_from_string(content)
}

/// Convenience function to expand wildcards in text with default settings
pub fn expand_wildcards(text: &str) -> Vec<String> {
    BtcRecoverTokenList::default().expand_wildcards(text)
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_full_integration() {
        let config = TokenListConfig {
            min_tokens: 1,
            max_tokens: 3,
            ..Default::default()
        };
        
        let btc_recover = BtcRecoverTokenList::new(config);
        
        let token_content = r#"
# This is a comment
hello world
+ required_token
test%d
^beginning middle end$
"#;

        let passwords = btc_recover
            .generate_from_string(token_content)
            .unwrap();

        assert!(!passwords.is_empty());
        
        // Should contain various combinations
        println!("Generated {} passwords", passwords.len());
        for (i, password) in passwords.iter().take(10).enumerate() {
            println!("{}: {}", i + 1, password);
        }
    }

    #[test]
    fn test_wildcard_expansion() {
        let btc_recover = BtcRecoverTokenList::new(TokenListConfig::default());
        
        let expansions = btc_recover.expand_wildcards("test%2d");
        assert_eq!(expansions.len(), 100); // 10^2 combinations
        assert!(expansions.contains(&"test00".to_string()));
        assert!(expansions.contains(&"test99".to_string()));
    }

    #[test]
    fn test_custom_wildcards() {
        let custom_wildcards = vec![
            ('x', vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]),
        ];
        
        let btc_recover = BtcRecoverTokenList::new(TokenListConfig::default())
            .with_custom_wildcards(custom_wildcards);
        
        let expansions = btc_recover.expand_wildcards("test%x");
        assert_eq!(expansions.len(), 3);
        assert!(expansions.contains(&"testalpha".to_string()));
        assert!(expansions.contains(&"testbeta".to_string()));
        assert!(expansions.contains(&"testgamma".to_string()));
    }
}
