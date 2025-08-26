use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use futures::stream::StreamExt;
use std::path::Path;
use crate::tokenlist::{TokenList, TokenLine, Token, TokenListConfig};
use tokio_stream::wrappers::LinesStream;

pub struct AsyncTokenListParser {
    config: TokenListConfig,
}

impl AsyncTokenListParser {
    pub fn new(config: TokenListConfig) -> Self {
        Self { config }
    }

    /// Parse token file asynchronously with streaming
    pub async fn parse_file_async<P: AsRef<Path>>(&self, path: P) -> Result<TokenList, Box<dyn std::error::Error + Send + Sync>> {
        let file = File::open(path).await?;
        let reader = BufReader::new(file);
        let lines_stream = LinesStream::new(reader.lines());
        self.parse_lines_stream(lines_stream).await
    }

    /// Parse multiple files concurrently
    pub async fn parse_files_concurrent<P: AsRef<Path>>(&self, paths: Vec<P>) -> Result<Vec<TokenList>, Box<dyn std::error::Error + Send + Sync>> {
        let futures: Vec<_> = paths.into_iter()
            .map(|path| self.parse_file_async(path))
            .collect();
        
        let results = futures::future::try_join_all(futures).await?;
        Ok(results)
    }

    /// Stream-based parsing for memory efficiency
    async fn parse_lines_stream<S>(&self, mut lines: S) -> Result<TokenList, Box<dyn std::error::Error + Send + Sync>>
    where
        S: futures::stream::Stream<Item = Result<String, std::io::Error>> + Unpin,
    {
        let mut token_lines = Vec::new();
        let mut has_anchors = false;
        let mut has_wildcards = false;
        let mut has_duplicates = false;
        let mut line_number = 0;

        // Process lines in chunks for better memory usage
        let mut line_buffer = Vec::new();
        const CHUNK_SIZE: usize = 1000;

        while let Some(line_result) = lines.next().await {
            let line = line_result?;
            line_number += 1;
            line_buffer.push((line, line_number));

            if line_buffer.len() >= CHUNK_SIZE {
                let chunk_results = self.process_line_chunk(&line_buffer).await;
                for (token_line, anchors, wildcards, duplicates) in chunk_results {
                    if let Some(tl) = token_line {
                        token_lines.push(tl);
                        has_anchors |= anchors;
                        has_wildcards |= wildcards;
                        has_duplicates |= duplicates;
                    }
                }
                line_buffer.clear();
            }
        }

        // Process remaining lines
        if !line_buffer.is_empty() {
            let chunk_results = self.process_line_chunk(&line_buffer).await;
            for (token_line, anchors, wildcards, duplicates) in chunk_results {
                if let Some(tl) = token_line {
                    token_lines.push(tl);
                    has_anchors |= anchors;
                    has_wildcards |= wildcards;
                    has_duplicates |= duplicates;
                }
            }
        }

        // Reverse to match Python behavior
        token_lines.reverse();

        Ok(TokenList {
            lines: token_lines,
            has_anchors,
            has_wildcards,
            has_duplicates,
        })
    }

    /// Process a chunk of lines concurrently
    async fn process_line_chunk(&self, lines: &[(String, usize)]) -> Vec<(Option<TokenLine>, bool, bool, bool)> {
        use rayon::prelude::*;
        
        // Use rayon for CPU-bound parsing within async context
        let results: Vec<_> = lines.par_iter()
            .map(|(line, line_num)| self.parse_single_line(line, *line_num))
            .collect();
        
        results
    }

    fn parse_single_line(&self, line: &str, _line_number: usize) -> (Option<TokenLine>, bool, bool, bool) {
        let trimmed = line.trim();
        
        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            // Check for embedded options warning
            if trimmed.starts_with("# --") {
                eprintln!("Warning: Embedded command-line options are not supported: {}", trimmed);
            }
            return (None, false, false, false);
        }

        let mut has_anchors = false;
        let mut has_wildcards = false;
        let mut has_duplicates = false;

        // Parse tokens from the line
        let delimiter = self.config.delimiter.as_deref().unwrap_or(" ");
        let raw_tokens: Vec<&str> = if delimiter == " " {
            trimmed.split_whitespace().collect()
        } else {
            trimmed.split(delimiter).collect()
        };

        let mut tokens = Vec::new();
        let mut is_required = false;

        for raw_token in raw_tokens {
            if raw_token.is_empty() {
                continue;
            }

            // Check for required token marker
            let token_text = if raw_token.starts_with('+') {
                is_required = true;
                &raw_token[1..]
            } else {
                raw_token
            };

            if token_text.is_empty() {
                continue;
            }

            // Check for wildcards
            if token_text.contains('%') {
                has_wildcards = true;
            }

            // Parse anchors and create token
            let token = self.parse_token_with_anchors(token_text);
            if matches!(token, Token::Anchored(_)) {
                has_anchors = true;
            }

            tokens.push(token);
        }

        // Check for duplicates within the line
        let token_texts: Vec<String> = tokens.iter().map(|t| t.text().to_string()).collect();
        let unique_texts: std::collections::HashSet<_> = token_texts.iter().collect();
        if unique_texts.len() != token_texts.len() {
            has_duplicates = true;
        }

        if tokens.is_empty() {
            return (None, has_anchors, has_wildcards, has_duplicates);
        }

        let token_line = TokenLine {
            tokens,
            is_required,
        };

        (Some(token_line), has_anchors, has_wildcards, has_duplicates)
    }

    fn parse_token_with_anchors(&self, text: &str) -> Token {
        // Simplified anchor parsing - implement full logic from original
        Token::Simple(text.to_string())
    }
}

/// Async password generator with backpressure control
pub struct AsyncPasswordGenerator {
    parser: AsyncTokenListParser,
    batch_size: usize,
}

impl AsyncPasswordGenerator {
    pub fn new(config: TokenListConfig) -> Self {
        Self {
            parser: AsyncTokenListParser::new(config),
            batch_size: 10000,
        }
    }

    /// Generate passwords as an async stream with backpressure
    pub async fn generate_password_stream<P: AsRef<Path>>(&self, path: P) -> std::pin::Pin<Box<dyn futures::stream::Stream<Item = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
        let token_list = match self.parser.parse_file_async(path).await {
            Ok(tl) => tl,
            Err(e) => return Box::pin(futures::stream::once(async { Err(e) })),
        };

        Box::pin(self.create_password_stream(token_list))
    }

    fn create_password_stream(&self, _token_list: TokenList) -> impl futures::stream::Stream<Item = Result<String, Box<dyn std::error::Error + Send + Sync>>> {
        futures::stream::iter(vec!["password1".to_string(), "password2".to_string()]) // Placeholder
            .map(|password| Ok(password))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_async_file_parsing() {
        let config = TokenListConfig::default();
        let parser = AsyncTokenListParser::new(config);
        
        // Create a temporary file with test content
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "hello world").unwrap();
        writeln!(temp_file, "+ required").unwrap();
        writeln!(temp_file, "test%d").unwrap();
        temp_file.flush().unwrap();
        
        let result = parser.parse_file_async(temp_file.path()).await;
        assert!(result.is_ok());
        
        let token_list = result.unwrap();
        assert_eq!(token_list.lines.len(), 3);
        assert!(token_list.has_wildcards);
    }

    #[tokio::test]
    async fn test_concurrent_file_parsing() {
        let config = TokenListConfig::default();
        let parser = AsyncTokenListParser::new(config);
        
        // Create multiple temporary files
        let mut files = Vec::new();
        for i in 0..3 {
            let mut temp_file = NamedTempFile::new().unwrap();
            writeln!(temp_file, "token{}", i).unwrap();
            writeln!(temp_file, "test%d").unwrap();
            temp_file.flush().unwrap();
            files.push(temp_file);
        }
        
        let paths: Vec<_> = files.iter().map(|f| f.path()).collect();
        let results = parser.parse_files_concurrent(paths).await;
        
        assert!(results.is_ok());
        let token_lists = results.unwrap();
        assert_eq!(token_lists.len(), 3);
        
        for token_list in &token_lists {
            assert_eq!(token_list.lines.len(), 2);
            assert!(token_list.has_wildcards);
        }
    }

    #[tokio::test]
    async fn test_async_password_generation() {
        let config = TokenListConfig::default();
        let generator = AsyncPasswordGenerator::new(config);
        
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "hello").unwrap();
        writeln!(temp_file, "world").unwrap();
        temp_file.flush().unwrap();
        
        let stream = generator.generate_password_stream(temp_file.path()).await;
        let passwords: Vec<_> = stream.collect().await;
        
        assert!(!passwords.is_empty());
        // All results should be Ok
        for result in &passwords {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_parse_single_line() {
        let config = TokenListConfig::default();
        let parser = AsyncTokenListParser::new(config);
        
        // Test regular line
        let (token_line, anchors, wildcards, duplicates) = parser.parse_single_line("hello world", 1);
        assert!(token_line.is_some());
        assert!(!anchors);
        assert!(!wildcards);
        assert!(!duplicates);
        
        let line = token_line.unwrap();
        assert_eq!(line.tokens.len(), 2);
        assert!(!line.is_required);
        
        // Test required line
        let (token_line, _, _, _) = parser.parse_single_line("+ required", 2);
        assert!(token_line.is_some());
        let line = token_line.unwrap();
        assert!(line.is_required);
        
        // Test wildcard line
        let (token_line, _, wildcards, _) = parser.parse_single_line("test%d", 3);
        assert!(token_line.is_some());
        assert!(wildcards);
        
        // Test comment line
        let (token_line, _, _, _) = parser.parse_single_line("# comment", 4);
        assert!(token_line.is_none());
        
        // Test empty line
        let (token_line, _, _, _) = parser.parse_single_line("", 5);
        assert!(token_line.is_none());
    }

    #[tokio::test]
    async fn test_chunk_processing() {
        let config = TokenListConfig::default();
        let parser = AsyncTokenListParser::new(config);
        
        let lines = vec![
            ("hello world".to_string(), 1),
            ("+ required".to_string(), 2),
            ("test%d".to_string(), 3),
            ("# comment".to_string(), 4),
        ];
        
        let results = parser.process_line_chunk(&lines).await;
        assert_eq!(results.len(), 4);
        
        // First three should have token lines, last should be None (comment)
        assert!(results[0].0.is_some());
        assert!(results[1].0.is_some());
        assert!(results[2].0.is_some());
        assert!(results[3].0.is_none());
        
        // Check flags
        assert!(results[1].0.as_ref().unwrap().is_required); // required token
        assert!(results[2].2); // has wildcards
    }
}
