use std::path::Path;
use thiserror::Error;

pub mod tokenlist;
pub mod wildcards;

pub use tokenlist::{TokenList, TokenLine, Token, AnchoredToken, AnchorType, TokenListConfig, TokenListParser};
pub use wildcards::WildcardExpander;

#[derive(Debug, Clone)]
pub struct PasswordGeneratorOptions {
    pub min_tokens: usize,
    pub max_tokens: usize,
    pub keep_tokens_order: bool,
    pub truncate_length: Option<usize>,
    pub password_repeats: Option<usize>,
    pub delimiter: Option<String>,
}

impl Default for PasswordGeneratorOptions {
    fn default() -> Self {
        Self {
            min_tokens: 1,
            max_tokens: 10,
            keep_tokens_order: false,
            truncate_length: None,
            password_repeats: None,
            delimiter: None,
        }
    }
}

#[derive(Error, Debug)]
pub enum TokenlistError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Wildcard error: {0}")]
    Wildcard(String),
    #[error("Generic error: {0}")]
    Generic(#[from] Box<dyn std::error::Error>),
}

pub struct BtcRecoverTokenList {
    parser: TokenListParser,
    wildcard_expander: WildcardExpander,
}

impl BtcRecoverTokenList {
    pub fn new(config: TokenListConfig) -> Self {
        Self {
            parser: TokenListParser::new(config),
            wildcard_expander: WildcardExpander::new(),
        }
    }

    pub fn generate_passwords_from_file_iter<P: AsRef<Path>>(&self, path: P) -> Result<PasswordGenerator, TokenlistError> {
        let token_list = self.parser.parse_file(path)?;
        Ok(PasswordGenerator::new(token_list, TokenListConfig::default()))
    }

    pub fn with_custom_wildcards(mut self, wildcards: Vec<(char, Vec<String>)>) -> Self {
        for (wildcard_char, values) in wildcards {
            self.wildcard_expander.add_custom_wildcard(wildcard_char, values);
        }
        self
    }

    pub fn generate_passwords_from_file<P: AsRef<Path>>(
        &self, 
        path: P
    ) -> Result<Vec<String>, TokenlistError> {
        let token_list = self.parser.parse_file(path)?;
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        Ok(generator.generate_passwords().collect())
    }

    pub fn generate_passwords_from_string(
        &self, 
        content: &str
    ) -> Result<Vec<String>, TokenlistError> {
        use std::io::Cursor;
        let cursor = Cursor::new(content);
        let token_list = self.parser.parse_reader(cursor)?;
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        Ok(generator.generate_passwords().collect())
    }

    pub fn expand_wildcards(&self, text: &str) -> Vec<String> {
        self.wildcard_expander.expand_wildcards(text)
    }
}

pub struct PasswordGenerator {
    token_list: TokenList,
    config: TokenListConfig,
}

impl PasswordGenerator {
    pub fn new(token_list: TokenList, config: TokenListConfig) -> Self {
        Self { token_list, config }
    }

    pub fn generate_passwords(self) -> impl Iterator<Item = String> {
        self.token_list.generate_passwords_with_config(self.config).into_iter()
    }
}

// Remove duplicate exports - already exported above

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::io::Cursor;

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
            .generate_passwords_from_string(token_content)
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
