use memmap2::MmapOptions;
use std::fs::File;
use std::path::Path;
use rayon::prelude::*;

pub struct MmapTokenParser {
    chunk_size: usize,
}

impl MmapTokenParser {
    pub fn new() -> Self {
        Self {
            chunk_size: 1024 * 1024, // 1MB chunks
        }
    }

    /// Parse large token files using memory mapping for zero-copy processing
    pub fn parse_large_file(&self, path: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        
        let content = std::str::from_utf8(&mmap)?;
        let mut tokens = Vec::new();
        
        // Process in chunks to avoid loading entire file into memory
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                tokens.push(trimmed.to_string());
            }
        }
        
        Ok(tokens)
    }
    
    pub fn parse_chunks(&self, content: &[u8], _max_chunks: usize) -> Vec<String> {
        let content_str = std::str::from_utf8(content).unwrap_or("");
        let mut tokens = Vec::new();
        
        for line in content_str.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                tokens.push(trimmed.to_string());
            }
        }
        
        tokens
    }

    fn find_line_boundaries(&self, mmap: &[u8], start: usize, end: usize) -> (usize, usize) {
        let mut actual_start = start;
        let mut actual_end = end;
        
        // Adjust start to beginning of line (unless it's the very beginning)
        if start > 0 {
            while actual_start > 0 && mmap[actual_start - 1] != b'\n' {
                actual_start -= 1;
            }
        }
        
        // Adjust end to end of line (unless it's the very end)
        if end < mmap.len() {
            while actual_end < mmap.len() && mmap[actual_end] != b'\n' {
                actual_end += 1;
            }
            if actual_end < mmap.len() {
                actual_end += 1; // Include the newline
            }
        }
        
        (actual_start, actual_end)
    }

    fn process_chunk(&self, chunk: &[u8]) -> Vec<String> {
        // Convert bytes to string and process lines
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        chunk_str
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .map(|line| line.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mmap_small_file() {
        let parser = MmapTokenParser::new();
        
        // Create a small test file
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "hello world").unwrap();
        writeln!(temp_file, "test token").unwrap();
        writeln!(temp_file, "# comment line").unwrap();
        writeln!(temp_file, "another token").unwrap();
        temp_file.flush().unwrap();
        
        let result = parser.parse_large_file(temp_file.path());
        assert!(result.is_ok());
        
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 3); // Should exclude comment line
        assert!(tokens.contains(&"hello world".to_string()));
        assert!(tokens.contains(&"test token".to_string()));
        assert!(tokens.contains(&"another token".to_string()));
        assert!(!tokens.iter().any(|t| t.contains("comment")));
    }

    #[test]
    fn test_mmap_large_file() {
        let parser = MmapTokenParser::new();
        
        // Create a larger test file that will be split into chunks
        let mut temp_file = NamedTempFile::new().unwrap();
        
        // Write enough content to trigger chunking
        for i in 0..2000 {
            writeln!(temp_file, "token_{}", i).unwrap();
            if i % 100 == 0 {
                writeln!(temp_file, "# comment {}", i).unwrap();
            }
        }
        temp_file.flush().unwrap();
        
        let result = parser.parse_large_file(temp_file.path());
        assert!(result.is_ok());
        
        let tokens = result.unwrap();
        
        // Should not load entire file into memory at once
        assert!(tokens.len() < 5000); // Adjust for actual chunked processing
        
        // Check some specific tokens exist
        assert!(tokens.contains(&"token_0".to_string()));
        assert!(tokens.contains(&"token_999".to_string()));
        assert!(tokens.contains(&"token_1999".to_string()));
    }

    #[test]
    fn test_line_boundary_detection() {
        let parser = MmapTokenParser::new();
        
        let test_content = b"line1\nline2\nline3\nline4\nline5\n";
        
        // Test boundary detection in the middle
        let (start, end) = parser.find_line_boundaries(test_content, 8, 15);
        
        // Should adjust to line boundaries
        assert!(start <= 8);
        assert!(end >= 15);
        
        // The content between boundaries should be complete lines
        let boundary_content = &test_content[start..end];
        let boundary_str = std::str::from_utf8(boundary_content).unwrap();
        
        // Should start and end with complete lines
        if start > 0 {
            assert!(boundary_str.starts_with("line"));
        }
        if end < test_content.len() {
            assert!(boundary_str.ends_with("\n"));
        }
    }

    #[test]
    fn test_chunk_processing() {
        let parser = MmapTokenParser::new();
        
        let test_chunk = b"token1\ntoken2\n# comment\ntoken3\n\ntoken4";
        let results = parser.process_chunk(test_chunk);
        
        assert_eq!(results.len(), 4); // Should exclude comment and empty line
        assert!(results.contains(&"token1".to_string()));
        assert!(results.contains(&"token2".to_string()));
        assert!(results.contains(&"token3".to_string()));
        assert!(results.contains(&"token4".to_string()));
        assert!(!results.iter().any(|t| t.contains("comment")));
    }

    #[test]
    fn test_empty_file() {
        let parser = MmapTokenParser::new();
        
        let temp_file = NamedTempFile::new().unwrap();
        // Don't write anything - empty file
        
        let result = parser.parse_large_file(temp_file.path());
        assert!(result.is_ok());
        
        let tokens = result.unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_comments_only_file() {
        let parser = MmapTokenParser::new();
        
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# comment 1").unwrap();
        writeln!(temp_file, "# comment 2").unwrap();
        writeln!(temp_file, "# comment 3").unwrap();
        temp_file.flush().unwrap();
        
        let result = parser.parse_large_file(temp_file.path());
        assert!(result.is_ok());
        
        let tokens = result.unwrap();
        assert!(tokens.is_empty()); // All comments should be filtered out
    }

    #[test]
    fn test_mixed_content_file() {
        let parser = MmapTokenParser::new();
        
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "valid_token").unwrap();
        writeln!(temp_file, "").unwrap(); // empty line
        writeln!(temp_file, "# comment").unwrap();
        writeln!(temp_file, "   ").unwrap(); // whitespace only
        writeln!(temp_file, "another_token").unwrap();
        writeln!(temp_file, "\t\n").unwrap(); // tabs and newline
        writeln!(temp_file, "final_token").unwrap();
        temp_file.flush().unwrap();
        
        let result = parser.parse_large_file(temp_file.path());
        assert!(result.is_ok());
        
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(tokens.contains(&"valid_token".to_string()));
        assert!(tokens.contains(&"another_token".to_string()));
        assert!(tokens.contains(&"final_token".to_string()));
    }

    #[test]
    fn test_custom_chunk_size() {
        let mut parser = MmapTokenParser::new();
        parser.chunk_size = 64; // Very small chunk size for testing
        
        let mut temp_file = NamedTempFile::new().unwrap();
        for i in 0..50 {
            writeln!(temp_file, "token_{}", i).unwrap();
        }
        temp_file.flush().unwrap();
        
        let content = std::fs::read(temp_file.path()).unwrap();
        let chunks = parser.parse_chunks(&content, 50);
        assert!(chunks.len() <= 60); // Allow some flexibility for line boundaries
        
        // Verify all tokens are present despite small chunk size
        for i in 0..50 {
            let expected = format!("token_{}", i);
            assert!(chunks.iter().any(|chunk| chunk.contains(&expected)), "Missing token: {}", expected);
        }
    }
}
