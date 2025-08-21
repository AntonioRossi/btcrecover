use std::collections::HashMap;
use std::fs;
use std::path::Path;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct WildcardExpander {
    custom_wildcards: HashMap<char, Vec<String>>,
    case_insensitive: bool,
    backreference_maps: HashMap<String, HashMap<char, Vec<char>>>,
}

impl WildcardExpander {
    pub fn new() -> Self {
        Self {
            custom_wildcards: HashMap::new(),
            case_insensitive: false,
            backreference_maps: HashMap::new(),
        }
    }

    pub fn with_case_insensitive(mut self, case_insensitive: bool) -> Self {
        self.case_insensitive = case_insensitive;
        self
    }

    pub fn add_custom_wildcard(&mut self, wildcard_char: char, values: Vec<String>) {
        self.custom_wildcards.insert(wildcard_char, values);
    }

    pub fn load_backreference_map<P: AsRef<Path>>(&mut self, name: String, path: P) -> Result<(), std::io::Error> {
        let content = fs::read_to_string(path)?;
        let mut map = HashMap::new();
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let (Some(from_char), Some(to_str)) = (parts[0].chars().next(), parts.get(1)) {
                    let to_chars: Vec<char> = to_str.chars().collect();
                    map.insert(from_char, to_chars);
                }
            }
        }
        
        self.backreference_maps.insert(name, map);
        Ok(())
    }

    pub fn expand_wildcards(&self, text: &str) -> Vec<String> {
        if !text.contains('%') {
            return vec![text.to_string()];
        }

        // Handle backreference wildcards first
        if text.contains("%b") {
            return self.expand_backreference_wildcards(text);
        }

        // Handle contracting wildcards
        if text.contains("%-") || text.contains("%<") || text.contains("%>") {
            return self.expand_contracting_wildcards(text);
        }

        // Handle custom character sets like %[chars]
        if text.contains("%[") {
            return self.expand_character_set_wildcards(text);
        }

        let mut results = vec![text.to_string()];
        let wildcard_regex = Regex::new(r"%(\d*,?\d*)(i?[a-zA-Z%\^\[\]S<>-]|\[[^\]]+\])").unwrap();

        for captures in wildcard_regex.captures_iter(text) {
            let full_match = captures.get(0).unwrap().as_str();
            let count_spec = captures.get(1).unwrap().as_str();
            let wildcard_spec = captures.get(2).unwrap().as_str();

            let (min_count, max_count) = self.parse_count_spec(count_spec);
            let case_insensitive = wildcard_spec.starts_with('i');
            let wildcard_char = if case_insensitive {
                wildcard_spec.chars().nth(1).unwrap_or('d')
            } else {
                wildcard_spec.chars().next().unwrap_or('d')
            };

            let expansions = self.get_wildcard_expansions_with_case(wildcard_char, min_count, max_count, case_insensitive);

            let mut new_results = Vec::new();
            for result in results {
                for expansion in &expansions {
                    new_results.push(result.replace(full_match, expansion));
                }
            }
            results = new_results;
        }

        results
    }

    fn parse_count_spec(&self, spec: &str) -> (usize, usize) {
        if spec.is_empty() {
            return (1, 1);
        }

        if spec.contains(',') {
            let parts: Vec<&str> = spec.split(',').collect();
            let min = if parts[0].is_empty() { 0 } else { parts[0].parse().unwrap_or(1) };
            let max = if parts.len() > 1 && !parts[1].is_empty() { 
                parts[1].parse().unwrap_or(min) 
            } else { 
                usize::MAX 
            };
            (min, max)
        } else {
            let count = spec.parse().unwrap_or(1);
            (count, count)
        }
    }

    fn get_wildcard_expansions_with_case(&self, wildcard_type: char, min_count: usize, max_count: usize, case_insensitive: bool) -> Vec<String> {
        let expansions = self.get_wildcard_expansions(wildcard_type, min_count, max_count);
        
        if case_insensitive {
            let mut case_variants = Vec::new();
            for expansion in expansions {
                case_variants.push(expansion.to_lowercase());
                case_variants.push(expansion.to_uppercase());
            }
            case_variants.sort();
            case_variants.dedup();
            return case_variants;
        }
        
        expansions
    }

    fn get_wildcard_expansions(&self, wildcard_type: char, min_count: usize, max_count: usize) -> Vec<String> {
        // Handle escape wildcards first
        if wildcard_type == '%' || wildcard_type == '^' || wildcard_type == 'S' {
            let escape_char = match wildcard_type {
                '%' => '%',
                '^' => '^', 
                'S' => '$',
                _ => wildcard_type,
            };
            return vec![escape_char.to_string()];
        }

        // Handle custom wildcards
        if self.custom_wildcards.contains_key(&wildcard_type) {
            return self.custom_wildcards[&wildcard_type].clone();
        }

        let base_chars = self.get_base_characters(wildcard_type);
        let mut expansions = Vec::new();

        let actual_max = if max_count == usize::MAX { 
            std::cmp::min(4, std::cmp::max(min_count, 1)) // Reasonable limit
        } else { 
            max_count 
        };

        for length in min_count..=actual_max {
            if length == 0 {
                expansions.push(String::new());
            } else {
                expansions.extend(self.generate_combinations(&base_chars, length));
            }
        }

        expansions
    }

    fn get_base_characters(&self, wildcard_type: char) -> Vec<String> {
        match wildcard_type {
            'd' => ('0'..='9').map(|c| c.to_string()).collect(),
            'a' => ('a'..='z').map(|c| c.to_string()).collect(),
            'A' => ('A'..='Z').map(|c| c.to_string()).collect(),
            'n' => {
                let mut chars = ('0'..='9').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend(('a'..='z').map(|c| c.to_string()));
                chars
            }
            'N' => {
                let mut chars = ('0'..='9').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend(('A'..='Z').map(|c| c.to_string()));
                chars
            }
            's' => vec![" ".to_string()], // space
            'l' => vec!["\n".to_string()], // line feed
            'r' => vec!["\r".to_string()], // carriage return
            'R' => vec!["\n".to_string(), "\r".to_string()], // line feed or carriage return
            't' => vec!["\t".to_string()], // tab
            'T' => vec![" ".to_string(), "\t".to_string()], // space or tab
            'w' => vec![" ".to_string(), "\n".to_string(), "\r".to_string()], // whitespace
            'W' => vec![" ".to_string(), "\n".to_string(), "\r".to_string(), "\t".to_string()], // all whitespace
            'y' => {
                // ASCII symbols
                let mut chars = Vec::new();
                chars.extend((33..=47).map(|i| (i as u8 as char).to_string())); // !"#$%&'()*+,-./
                chars.extend((58..=64).map(|i| (i as u8 as char).to_string())); // :;<=>?@
                chars.extend((91..=96).map(|i| (i as u8 as char).to_string())); // [\]^_`
                chars.extend((123..=126).map(|i| (i as u8 as char).to_string())); // {|}~
                chars
            }
            'Y' => {
                let mut chars = ('0'..='9').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend((33..=47).map(|i| (i as u8 as char).to_string()));
                chars.extend((58..=64).map(|i| (i as u8 as char).to_string()));
                chars.extend((91..=96).map(|i| (i as u8 as char).to_string()));
                chars.extend((123..=126).map(|i| (i as u8 as char).to_string()));
                chars
            }
            'p' => {
                let mut chars = ('a'..='z').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend(('A'..='Z').map(|c| c.to_string()));
                chars.extend(('0'..='9').map(|c| c.to_string()));
                chars.extend((33..=47).map(|i| (i as u8 as char).to_string()));
                chars.extend((58..=64).map(|i| (i as u8 as char).to_string()));
                chars.extend((91..=96).map(|i| (i as u8 as char).to_string()));
                chars.extend((123..=126).map(|i| (i as u8 as char).to_string()));
                chars
            }
            'P' => {
                let mut chars = ('a'..='z').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend(('A'..='Z').map(|c| c.to_string()));
                chars.extend(('0'..='9').map(|c| c.to_string()));
                chars.extend((33..=47).map(|i| (i as u8 as char).to_string()));
                chars.extend((58..=64).map(|i| (i as u8 as char).to_string()));
                chars.extend((91..=96).map(|i| (i as u8 as char).to_string()));
                chars.extend((123..=126).map(|i| (i as u8 as char).to_string()));
                chars.extend([" ".to_string(), "\n".to_string(), "\r".to_string(), "\t".to_string()]);
                chars
            }
            'q' => {
                // BIP39 passphrase characters (letters, digits, symbols, space)
                let mut chars = ('a'..='z').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend(('A'..='Z').map(|c| c.to_string()));
                chars.extend(('0'..='9').map(|c| c.to_string()));
                chars.extend((33..=47).map(|i| (i as u8 as char).to_string()));
                chars.extend((58..=64).map(|i| (i as u8 as char).to_string()));
                chars.extend((91..=96).map(|i| (i as u8 as char).to_string()));
                chars.extend((123..=126).map(|i| (i as u8 as char).to_string()));
                chars.push(" ".to_string());
                chars
            }
            'H' => {
                // Hexadecimal characters
                let mut chars = ('0'..='9').map(|c| c.to_string()).collect::<Vec<_>>();
                chars.extend(('A'..='F').map(|c| c.to_string()));
                chars
            }
            'B' => {
                // Base58 characters (Bitcoin Base58)
                "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
                    .chars().map(|c| c.to_string()).collect()
            }
            'U' => {
                // Unicode characters (simplified - just ASCII for now)
                (32..=126).map(|i| (i as u8 as char).to_string()).collect()
            }
            'c' => {
                // Custom wildcard - should be set via add_custom_wildcard
                self.custom_wildcards.get(&'c').cloned().unwrap_or_default()
            }
            'C' => {
                // Uppercase version of custom wildcard
                let custom_chars = self.custom_wildcards.get(&'c').cloned().unwrap_or_default();
                custom_chars.iter().map(|s| s.to_uppercase()).collect()
            }
            'e' | 'f' | 'j' | 'k' => {
                // Custom string wildcards - should be loaded from external configuration
                // For now, return empty - these need to be configured externally
                self.custom_wildcards.get(&wildcard_type).cloned().unwrap_or_default()
            }
            _ => vec![], // Unknown wildcard type
        }
    }

    fn generate_combinations(&self, chars: &[String], length: usize) -> Vec<String> {
        if length == 0 {
            return vec![String::new()];
        }
        if length == 1 {
            return chars.to_vec();
        }

        let mut combinations = Vec::new();
        let base = chars.len();
        let total_combinations = base.pow(length as u32);

        for i in 0..total_combinations {
            let mut combination = String::with_capacity(length);
            let mut num = i;
            
            for _ in 0..length {
                let index = num % base;
                combination.push_str(&chars[index]);
                num /= base;
            }
            
            combinations.push(combination);
        }

        combinations
    }

    pub fn has_wildcards(&self, text: &str) -> bool {
        text.contains('%')
    }

    pub fn count_wildcards(&self, text: &str) -> usize {
        let wildcard_regex = Regex::new(r"%(\d*,?\d*)[a-zA-Z\[\]]").unwrap();
        wildcard_regex.find_iter(text).count()
    }

    fn expand_backreference_wildcards(&self, text: &str) -> Vec<String> {
        // Enhanced backreference implementation with map file support
        let backreference_regex = Regex::new(r"%(\d*)(;(.+?);(\d+))?b").unwrap();
        
        if let Some(captures) = backreference_regex.captures(text) {
            let copy_length = captures.get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(1);
            let map_file = captures.get(3).map(|m| m.as_str().to_string());
            let start_offset = captures.get(4)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(1);
            
            let _backref = BackreferenceWildcard::new(copy_length, start_offset, map_file);
            
            // For now, return a simple expansion
            // Real implementation would need to track position in password generation
            vec![text.replace(&captures[0], "XX")]
        } else {
            vec![text.to_string()]
        }
    }

    fn expand_contracting_wildcards(&self, text: &str) -> Vec<String> {
        let contracting_regex = Regex::new(r"%(\d+),?(\d+)?([<>-])").unwrap();
        
        if let Some(captures) = contracting_regex.captures(text) {
            let min_remove = captures[1].parse().unwrap_or(0);
            let max_remove = captures.get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(min_remove);
            
            let direction = match &captures[3] {
                "<" => ContractDirection::Left,
                ">" => ContractDirection::Right,
                "-" => ContractDirection::Both,
                _ => ContractDirection::Both,
            };
            
            let contracting = ContractingWildcard::new(min_remove, max_remove, direction);
            
            // For now, return simplified results
            let position = text.find(&captures[0]).unwrap_or(0);
            contracting.apply(text, position)
        } else {
            vec![text.to_string()]
        }
    }

    fn expand_character_set_wildcards(&self, text: &str) -> Vec<String> {
        let charset_regex = Regex::new(r"%(\d*,?\d*)\[([^\]]+)\]").unwrap();
        
        let mut results = vec![text.to_string()];
        
        for captures in charset_regex.captures_iter(text) {
            let full_match = captures.get(0).unwrap().as_str();
            let count_spec = captures.get(1).unwrap().as_str();
            let charset = captures.get(2).unwrap().as_str();
            
            let (min_count, max_count) = self.parse_count_spec(count_spec);
            let chars: Vec<String> = self.expand_character_range(charset);
            
            let mut new_results = Vec::new();
            for result in results {
                for length in min_count..=std::cmp::min(max_count, 4) {
                    if length == 0 {
                        new_results.push(result.replace(full_match, ""));
                    } else {
                        let combinations = self.generate_combinations(&chars, length);
                        for combination in combinations {
                            new_results.push(result.replace(full_match, &combination));
                        }
                    }
                }
            }
            results = new_results;
        }
        
        results
    }

    fn expand_character_range(&self, charset: &str) -> Vec<String> {
        let mut chars = Vec::new();
        let mut i = 0;
        let charset_chars: Vec<char> = charset.chars().collect();
        
        while i < charset_chars.len() {
            if i + 2 < charset_chars.len() && charset_chars[i + 1] == '-' {
                // Handle ranges like 0-9, a-z, A-Z
                let start = charset_chars[i] as u8;
                let end = charset_chars[i + 2] as u8;
                for c in start..=end {
                    chars.push((c as char).to_string());
                }
                i += 3;
            } else {
                chars.push(charset_chars[i].to_string());
                i += 1;
            }
        }
        
        chars
    }
}

#[derive(Debug, Clone)]
pub struct BackreferenceWildcard {
    pub copy_length: usize,
    pub start_offset: usize,
    pub map_file: Option<String>,
}

impl BackreferenceWildcard {
    pub fn new(copy_length: usize, start_offset: usize, map_file: Option<String>) -> Self {
        Self {
            copy_length,
            start_offset,
            map_file,
        }
    }

    pub fn apply(&self, text: &str, position: usize, expander: &WildcardExpander) -> Option<String> {
        if position < self.start_offset {
            return None;
        }

        let start_pos = position - self.start_offset;
        if start_pos >= text.len() {
            return None;
        }

        let end_pos = std::cmp::min(start_pos + self.copy_length, text.len());
        let copied_text = &text[start_pos..end_pos];

        if let Some(map_file) = &self.map_file {
            // Apply map file transformation for keyboard walking
            if let Some(map) = expander.backreference_maps.get(map_file) {
                let mut result = String::new();
                for ch in copied_text.chars() {
                    if let Some(mapped_chars) = map.get(&ch) {
                        // For simplicity, take the first mapped character
                        if let Some(&first_char) = mapped_chars.first() {
                            result.push(first_char);
                        } else {
                            result.push(ch); // No mapping, use original
                        }
                    } else {
                        result.push(ch); // No mapping, use original
                    }
                }
                Some(result)
            } else {
                Some(copied_text.to_string())
            }
        } else {
            Some(copied_text.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContractingWildcard {
    pub min_remove: usize,
    pub max_remove: usize,
    pub direction: ContractDirection,
}

#[derive(Debug, Clone)]
pub enum ContractDirection {
    Both,  // %0,5-
    Left,  // %0,5<
    Right, // %0,5>
}

impl ContractingWildcard {
    pub fn new(min_remove: usize, max_remove: usize, direction: ContractDirection) -> Self {
        Self {
            min_remove,
            max_remove,
            direction,
        }
    }

    pub fn apply(&self, text: &str, position: usize) -> Vec<String> {
        let mut results = Vec::new();
        
        for remove_count in self.min_remove..=self.max_remove {
            match self.direction {
                ContractDirection::Both => {
                    // Remove characters from both sides
                    for left_remove in 0..=remove_count {
                        let right_remove = remove_count - left_remove;
                        
                        let start = std::cmp::min(position, left_remove);
                        let end_pos = position + right_remove;
                        
                        if start < text.len() && end_pos <= text.len() {
                            let mut result = text.to_string();
                            if end_pos < text.len() {
                                result.drain(position..end_pos);
                            }
                            if start > 0 {
                                result.drain((position - start)..position);
                            }
                            results.push(result);
                        }
                    }
                }
                ContractDirection::Left => {
                    // Remove characters only from the left
                    let start = if position >= remove_count { position - remove_count } else { 0 };
                    if start < text.len() {
                        let mut result = text.to_string();
                        result.drain(start..position);
                        results.push(result);
                    }
                }
                ContractDirection::Right => {
                    // Remove characters only from the right
                    let end_pos = std::cmp::min(position + remove_count, text.len());
                    if position < text.len() {
                        let mut result = text.to_string();
                        result.drain(position..end_pos);
                        results.push(result);
                    }
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digit_wildcard() {
        let expander = WildcardExpander::new();
        let results = expander.expand_wildcards("test%d");
        assert_eq!(results.len(), 10);
        assert!(results.contains(&"test0".to_string()));
        assert!(results.contains(&"test9".to_string()));
    }

    #[test]
    fn test_multiple_digit_wildcard() {
        let expander = WildcardExpander::new();
        let results = expander.expand_wildcards("test%2d");
        assert_eq!(results.len(), 100); // 10^2
        assert!(results.contains(&"test00".to_string()));
        assert!(results.contains(&"test99".to_string()));
    }

    #[test]
    fn test_range_wildcard() {
        let expander = WildcardExpander::new();
        let results = expander.expand_wildcards("test%1,3d");
        // Should have 1-digit (10) + 2-digit (100) + 3-digit (1000) = 1110 combinations
        assert!(results.len() > 100);
        assert!(results.contains(&"test5".to_string()));
        assert!(results.contains(&"test55".to_string()));
        assert!(results.contains(&"test555".to_string()));
    }

    #[test]
    fn test_letter_wildcards() {
        let expander = WildcardExpander::new();
        
        let lowercase_results = expander.expand_wildcards("test%a");
        assert_eq!(lowercase_results.len(), 26);
        assert!(lowercase_results.contains(&"testa".to_string()));
        assert!(lowercase_results.contains(&"testz".to_string()));

        let uppercase_results = expander.expand_wildcards("test%A");
        assert_eq!(uppercase_results.len(), 26);
        assert!(uppercase_results.contains(&"testA".to_string()));
        assert!(uppercase_results.contains(&"testZ".to_string()));
    }

    #[test]
    fn test_custom_wildcard() {
        let mut expander = WildcardExpander::new();
        expander.add_custom_wildcard('c', vec!["x".to_string(), "y".to_string(), "z".to_string()]);
        
        let results = expander.expand_wildcards("test%c");
        assert_eq!(results.len(), 3);
        assert!(results.contains(&"testx".to_string()));
        assert!(results.contains(&"testy".to_string()));
        assert!(results.contains(&"testz".to_string()));
    }

    #[test]
    fn test_escape_wildcards() {
        let expander = WildcardExpander::new();
        
        let percent_results = expander.expand_wildcards("test%%");
        assert_eq!(percent_results.len(), 1);
        assert_eq!(percent_results[0], "test%");

        let caret_results = expander.expand_wildcards("test%^");
        assert_eq!(caret_results.len(), 1);
        assert_eq!(caret_results[0], "test^");
    }

    #[test]
    fn test_backreference_wildcard() {
        let backref = BackreferenceWildcard::new(2, 4, None);
        let expander = WildcardExpander::new();
        let result = backref.apply("testABCD", 8, &expander);
        assert_eq!(result, Some("AB".to_string()));
    }

    #[test]
    fn test_contracting_wildcard() {
        let contracting = ContractingWildcard::new(0, 2, ContractDirection::Right);
        let results = contracting.apply("testword", 4);
        assert!(results.contains(&"testord".to_string())); // Remove 1 char
        assert!(results.contains(&"testrd".to_string()));  // Remove 2 chars
    }
}
