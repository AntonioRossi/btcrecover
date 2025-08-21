use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use regex::Regex;
use itertools::Itertools;
use crate::wildcards::WildcardExpander;

#[derive(Debug)]
pub struct DuplicateChecker {
    seen_once: HashMap<String, usize>,
    duplicates: HashMap<String, usize>,
    run_number: usize,
    tracking: bool,
}

impl DuplicateChecker {
    const EXCLUDE: usize = usize::MAX;

    pub fn new() -> Self {
        Self {
            seen_once: HashMap::new(),
            duplicates: HashMap::new(),
            run_number: 0,
            tracking: true,
        }
    }

    pub fn is_duplicate(&mut self, item: &str) -> bool {
        if self.run_number == 0 {
            if self.duplicates.contains_key(item) {
                return true;
            }
            if self.seen_once.contains_key(item) {
                let count = self.seen_once.remove(item).unwrap_or(1);
                self.duplicates.insert(item.to_string(), count);
                return true;
            }
            if self.tracking {
                self.seen_once.insert(item.to_string(), 1);
            }
            return false;
        }

        if let Some(&duplicate_count) = self.duplicates.get(item) {
            if duplicate_count == Self::EXCLUDE {
                return true;
            }
            if duplicate_count <= self.run_number {
                self.duplicates.insert(item.to_string(), self.run_number + 1);
                return false;
            } else {
                return true;
            }
        }
        false
    }

    pub fn exclude(&mut self, item: &str) {
        self.duplicates.insert(item.to_string(), Self::EXCLUDE);
    }

    pub fn disable_duplicate_tracking(&mut self) {
        self.tracking = false;
    }

    pub fn run_finished(&mut self) {
        if self.run_number == 0 {
            self.seen_once.clear();
        }
        self.run_number += 1;
    }
    
    pub fn clear(&mut self) {
        self.seen_once.clear();
        self.duplicates.clear();
        self.run_number = 0;
    }
    
    pub fn get_duplicate_count(&self) -> usize {
        self.duplicates.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnchorType {
    Positional(usize),    // ^2^token or token$
    Relative(usize),      // ^r1^token
    Middle { begin: Option<usize>, end: Option<usize> }, // ^2,4^token
    End,                  // token$
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchoredToken {
    pub text: String,
    pub anchor_type: AnchorType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    Simple(String),
    Anchored(AnchoredToken),
}

impl Token {
    pub fn text(&self) -> &str {
        match self {
            Token::Simple(text) => text,
            Token::Anchored(anchored) => &anchored.text,
        }
    }

    pub fn is_anchored(&self) -> bool {
        matches!(self, Token::Anchored(_))
    }
}

#[derive(Debug, Clone)]
pub struct TokenLine {
    pub tokens: Vec<Token>,
    pub is_required: bool,  // true if line started with '+'
}

#[derive(Debug, Clone)]
pub struct TokenList {
    pub lines: Vec<TokenLine>,
    pub has_wildcards: bool,
    pub has_anchors: bool,
    pub has_duplicates: bool,
}

#[derive(Debug, Clone)]
pub struct TokenListConfig {
    pub delimiter: Option<String>,
    pub min_tokens: usize,
    pub max_tokens: usize,
    pub keep_tokens_order: bool,
    pub no_dupchecks: u8,
    pub truncate_length: Option<usize>,
    pub password_repeats: Option<usize>,
    pub mnemonic_length: Option<usize>,
    pub seedgenerator: bool,
    pub custom_wild_chars: Option<String>,
    pub seed_transform_wordswaps: Option<usize>,
    pub wildcard_custom_lists: Option<HashMap<char, String>>,
    pub length_min: Option<usize>,
    pub length_max: Option<usize>,
    pub regex_only: Option<String>,
    pub regex_never: Option<String>,
}

impl Default for TokenListConfig {
    fn default() -> Self {
        Self {
            delimiter: None, // Use whitespace by default
            min_tokens: 1,
            max_tokens: usize::MAX,
            keep_tokens_order: false,
            no_dupchecks: 0,
            truncate_length: None,
            password_repeats: None,
            mnemonic_length: None,
            seedgenerator: false,
            custom_wild_chars: None,
            seed_transform_wordswaps: None,
            wildcard_custom_lists: None,
            length_min: None,
            length_max: None,
            regex_only: None,
            regex_never: None,
        }
    }
}

pub struct TokenListParser {
    config: TokenListConfig,
}

impl TokenListParser {
    pub fn new(config: TokenListConfig) -> Self {
        Self { config }
    }

    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<TokenList, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        self.parse_reader(reader)
    }

    pub fn parse_reader<R: BufRead>(&self, reader: R) -> Result<TokenList, Box<dyn std::error::Error>> {
        let mut lines = Vec::new();
        let mut has_wildcards = false;
        let mut has_anchors = false;
        let mut has_duplicates = false;
        let mut token_set = HashSet::new();

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = line_result?;
            let line_num = line_num + 1;

            // Skip comments
            if line.trim_start().starts_with('#') {
                // Check for embedded options (warning only)
                if line.trim_start().starts_with("# --") && line_num > 1 {
                    eprintln!("Warning: all options must be on the first line, ignoring options on line {}", line_num);
                }
                continue;
            }

            // Skip empty lines
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Parse the line
            let token_line = self.parse_line(&line, line_num)?;
            
            // Check for wildcards and anchors
            for token in &token_line.tokens {
                if self.has_wildcards(token.text()) {
                    has_wildcards = true;
                }
                if token.is_anchored() {
                    has_anchors = true;
                }

                // Check for duplicates
                if self.config.no_dupchecks < 3 {
                    if token_set.contains(token) {
                        has_duplicates = true;
                    } else {
                        token_set.insert(token.clone());
                    }
                }
            }

            if !token_line.tokens.is_empty() {
                lines.push(token_line);
            }
        }

        // Reverse the list so tokens at the beginning of file get tried first
        lines.reverse();

        Ok(TokenList {
            lines,
            has_wildcards,
            has_anchors,
            has_duplicates,
        })
    }

    fn parse_line(&self, line: &str, line_num: usize) -> Result<TokenLine, Box<dyn std::error::Error>> {
        let delimiter = self.config.delimiter.as_deref().unwrap_or(" ");
        
        // Split on delimiter (whitespace by default)
        let mut raw_tokens: Vec<&str> = if delimiter == " " {
            line.trim().split_whitespace().collect()
        } else {
            line.trim().split(delimiter).collect()
        };

        // Handle required tokens (lines starting with '+')
        let is_required = if !raw_tokens.is_empty() && raw_tokens[0] == "+" {
            raw_tokens.remove(0);
            true
        } else {
            false
        };

        // Handle custom wildcard spaces - merge tokens that are part of %[...] wildcards
        let raw_tokens = self.merge_custom_wildcard_tokens(raw_tokens);

        // Parse tokens and handle anchors
        let mut tokens = Vec::new();
        for token_str in raw_tokens {
            if token_str.is_empty() {
                continue;
            }

            let token = self.parse_token(&token_str, line_num)?;
            tokens.push(token);
        }

        Ok(TokenLine { tokens, is_required })
    }

    fn parse_token(&self, token_str: &str, line_num: usize) -> Result<Token, Box<dyn std::error::Error>> {
        // Check for anchored tokens
        if token_str.starts_with('^') || token_str.ends_with('$') {
            let anchored_token = self.parse_anchored_token(token_str, line_num)?;
            Ok(Token::Anchored(anchored_token))
        } else {
            Ok(Token::Simple(token_str.to_string()))
        }
    }

    fn parse_anchored_token(&self, token_str: &str, line_num: usize) -> Result<AnchoredToken, Box<dyn std::error::Error>> {
        // Regex patterns for different anchor types - matches Python implementation
        let positional_re = Regex::new(r"^\^(\d+)\^(.*)$")?;
        let relative_re = Regex::new(r"^\^[rR](\d+)\^(.*)$")?;
        let middle_re = Regex::new(r"^\^(\d+)?,(\d+)?\^(.*)$")?;
        let end_re = Regex::new(r"^(.+)\$$")?;
        let beginning_re = Regex::new(r"^\^(.+)$")?;
        
        // Validate that token doesn't have both ^ and $ anchors (Python validation)
        if token_str.starts_with('^') && token_str.ends_with('$') {
            return Err(format!("Token on line {} is anchored with both ^ at the beginning and $ at the end", line_num).into());
        }

        // Try to match different anchor patterns
        if let Some(caps) = positional_re.captures(token_str) {
            let pos: usize = caps[1].parse()?;
            let text = caps[2].to_string();
            
            // Validate positional anchor (Python validation)
            if pos < 1 {
                return Err(format!("Anchor position of token on line {} must be 1 or greater", line_num).into());
            }
            
            Ok(AnchoredToken {
                text,
                anchor_type: AnchorType::Positional(pos - 1), // Convert to 0-based indexing
            })
        } else if let Some(caps) = relative_re.captures(token_str) {
            let rel: usize = caps[1].parse()?;
            let text = caps[2].to_string();
            Ok(AnchoredToken {
                text,
                anchor_type: AnchorType::Relative(rel),
            })
        } else if let Some(caps) = middle_re.captures(token_str) {
            let begin = caps.get(1).and_then(|m| m.as_str().parse().ok());
            let end = caps.get(2).and_then(|m| m.as_str().parse().ok());
            let text = caps[3].to_string();
            
            // Validate middle anchor constraints (Python validation)
            let begin_pos = begin.unwrap_or(2);
            let end_pos = end.unwrap_or(usize::MAX);
            
            if begin_pos > end_pos {
                return Err(format!("Anchor range of token on line {} is invalid (begin > end)", line_num).into());
            }
            if begin_pos < 2 {
                return Err(format!("Anchor range of token on line {} must begin with 2 or greater", line_num).into());
            }
            
            Ok(AnchoredToken {
                text,
                anchor_type: AnchorType::Middle { begin, end },
            })
        } else if let Some(caps) = end_re.captures(token_str) {
            let text = caps[1].to_string();
            
            // Warn about empty tokens
            if text.is_empty() {
                eprintln!("Warning: token on line {} contains only an anchor (and zero password characters)", line_num);
            }
            
            Ok(AnchoredToken {
                text,
                anchor_type: AnchorType::End,
            })
        } else if let Some(caps) = beginning_re.captures(token_str) {
            let text = caps[1].to_string();
            
            // Provide warnings for tokens that look like they might be other anchor types
            if text.len() > 0 && text.chars().next().unwrap().is_ascii_digit() {
                eprintln!("Warning: token on line {} looks like it might be a positional anchor, but it can't be parsed correctly, so it's assumed to be a simple beginning anchor instead", line_num);
            }
            if text.len() > 1 && text.chars().next().unwrap().to_ascii_lowercase() == 'r' && text.chars().nth(1).unwrap().is_ascii_digit() {
                eprintln!("Warning: token on line {} looks like it might be a relative anchor, but it can't be parsed correctly, so it's assumed to be a simple beginning anchor instead", line_num);
            }
            
            // Warn about empty tokens
            if text.is_empty() {
                eprintln!("Warning: token on line {} contains only an anchor (and zero password characters)", line_num);
            }
            
            Ok(AnchoredToken {
                text,
                anchor_type: AnchorType::Positional(0), // Beginning position
            })
        } else {
            Err(format!("Invalid anchor syntax in token '{}' on line {}", token_str, line_num).into())
        }
    }

    fn has_wildcards(&self, text: &str) -> bool {
        // Simple check for wildcard characters
        text.contains('%')
    }

    fn merge_custom_wildcard_tokens(&self, tokens: Vec<&str>) -> Vec<String> {
        let mut merged_tokens = Vec::new();
        let mut temp_token: Option<String> = None;
        let delimiter = self.config.delimiter.as_deref().unwrap_or(" ");
        
        for token in tokens {
            // Check if this token starts a custom wildcard that might span multiple tokens
            if token.len() >= 5 && token.contains('%') && token.contains('[') && !token.contains(']') {
                temp_token = Some(token.to_string());
                continue;
            }
            
            // If we're building a multi-token wildcard, continue building it
            if let Some(ref mut building_token) = temp_token {
                building_token.push_str(delimiter);
                building_token.push_str(token);
                
                // If this token completes the wildcard, add it and reset
                if token.contains(']') {
                    merged_tokens.push(building_token.clone());
                    temp_token = None;
                }
                continue;
            }
            
            // Regular token
            merged_tokens.push(token.to_string());
        }
        
        // If we have an incomplete wildcard, add it as-is
        if let Some(incomplete) = temp_token {
            merged_tokens.push(incomplete);
        }
        
        merged_tokens
    }
}

pub struct PasswordGenerator {
    token_list: TokenList,
    config: TokenListConfig,
    wildcard_expander: WildcardExpander,
    duplicate_checker: Option<DuplicateChecker>,
}

impl PasswordGenerator {
    pub fn new(token_list: TokenList, config: TokenListConfig) -> Self {
        let duplicate_checker = if config.no_dupchecks < 2 && token_list.has_duplicates {
            Some(DuplicateChecker::new())
        } else {
            None
        };
        
        Self { 
            token_list, 
            config,
            wildcard_expander: WildcardExpander::new(),
            duplicate_checker,
        }
    }

    pub fn with_wildcard_expander(mut self, expander: WildcardExpander) -> Self {
        self.wildcard_expander = expander;
        self
    }
    
    pub fn with_duplicate_checker(mut self, checker: Option<DuplicateChecker>) -> Self {
        self.duplicate_checker = checker;
        self
    }

    pub fn generate_passwords(&self) -> impl Iterator<Item = String> + '_ {
        PasswordIterator::new(self)
    }

    fn generate_token_combinations(&self) -> Vec<Vec<&Token>> {
        let mut combinations = Vec::new();
        
        // Generate all possible combinations respecting the "at most one token per line" rule
        let line_choices: Vec<Vec<Option<&Token>>> = self.token_list.lines
            .iter()
            .map(|line| {
                let mut choices = vec![None]; // None means no token from this line
                if line.is_required {
                    choices.clear(); // Required lines must have a token
                }
                for token in &line.tokens {
                    choices.push(Some(token));
                }
                choices
            })
            .collect();

        // Generate cartesian product of all line choices
        let products = line_choices.iter()
            .multi_cartesian_product()
            .map(|combination| {
                combination.into_iter()
                    .filter_map(|&opt_token| opt_token)
                    .collect::<Vec<&Token>>()
            })
            .filter(|combo| {
                // Filter empty combinations and check token count constraints
                if combo.is_empty() {
                    return false;
                }
                
                // Check if all required lines have tokens
                let mut required_lines_satisfied = true;
                for line in &self.token_list.lines {
                    if line.is_required {
                        let has_token_from_line = combo.iter().any(|&token| {
                            line.tokens.iter().any(|line_token| {
                                std::ptr::eq(token, line_token)
                            })
                        });
                        if !has_token_from_line {
                            required_lines_satisfied = false;
                            break;
                        }
                    }
                }
                
                if !required_lines_satisfied {
                    return false;
                }
                
                let len = combo.len();
                len >= self.config.min_tokens && len <= self.config.max_tokens
            });

        combinations.extend(products);
        
        // Apply seed generator logic if enabled
        if self.config.seedgenerator {
            combinations = self.filter_combinations_for_seed_generator(combinations);
        }
        
        combinations
    }
    
    fn filter_combinations_for_seed_generator<'a>(&self, combinations: Vec<Vec<&'a Token>>) -> Vec<Vec<&'a Token>> {
        let mut filtered = Vec::new();
        
        for combo in combinations {
            // For seed generator, split tokens on commas and validate mnemonic length
            let mut expanded_tokens = Vec::new();
            for &token in &combo {
                let token_text = token.text();
                if token_text.contains(',') {
                    for part in token_text.split(',') {
                        if !part.trim().is_empty() {
                            expanded_tokens.push(part.trim());
                        }
                    }
                } else {
                    expanded_tokens.push(token_text);
                }
            }
            
            // Check mnemonic length constraint if specified
            if let Some(expected_length) = self.config.mnemonic_length {
                if expanded_tokens.len() != expected_length {
                    continue; // Skip combinations that don't match expected mnemonic length
                }
            }
            
            filtered.push(combo);
        }
        
        filtered
    }

    fn arrange_tokens(&self, tokens: &[&Token]) -> Vec<Vec<String>> {
        if self.config.keep_tokens_order {
            // Keep original order and expand wildcards
            return self.expand_tokens_in_order(tokens);
        }

        let mut arrangements = Vec::new();

        // Handle anchored tokens
        if self.token_list.has_anchors {
            arrangements.extend(self.arrange_anchored_tokens(tokens));
        } else {
            // Generate all permutations for non-anchored tokens
            for perm in tokens.iter().permutations(tokens.len()) {
                let perm_slice: Vec<&Token> = perm.into_iter().copied().collect();
                arrangements.extend(self.expand_tokens_in_order(&perm_slice));
            }
        }

        arrangements
    }
    
    fn expand_tokens_in_order(&self, tokens: &[&Token]) -> Vec<Vec<String>> {
        let mut arrangements = vec![Vec::new()];
        
        for &token in tokens {
            let expansions = self.wildcard_expander.expand_wildcards(token.text());
            let mut new_arrangements = Vec::new();
            
            for arrangement in arrangements {
                for expansion in &expansions {
                    let mut new_arrangement = arrangement.clone();
                    new_arrangement.push(expansion.clone());
                    new_arrangements.push(new_arrangement);
                }
            }
            arrangements = new_arrangements;
        }
        
        arrangements
    }

    fn arrange_anchored_tokens(&self, tokens: &[&Token]) -> Vec<Vec<String>> {
        let mut arrangements: Vec<Vec<String>> = Vec::new();
        
        // Separate anchored and non-anchored tokens
        let mut positional_tokens = HashMap::new();
        let mut relative_tokens = Vec::new();
        let mut middle_tokens = Vec::new();
        let mut non_anchored_tokens = Vec::new();
        let mut end_tokens = Vec::new();
        let mut beginning_tokens = Vec::new();
        
        // Check for invalid anchor combinations early
        let tokens_len = tokens.len();
        let mut has_conflicts = false;

        for &token in tokens {
            match token {
                Token::Anchored(anchored) => {
                    let expanded_tokens = self.wildcard_expander.expand_wildcards(&anchored.text);
                    match &anchored.anchor_type {
                        AnchorType::Positional(0) => {
                            // Beginning anchor (^token)
                            beginning_tokens.extend(expanded_tokens);
                        }
                        AnchorType::Positional(pos) => {
                            // Specific position anchor (^2^token) - validate position
                            if *pos >= tokens_len {
                                has_conflicts = true;
                                break;
                            }
                            // Check for conflicting positional anchors
                            if positional_tokens.contains_key(pos) {
                                has_conflicts = true;
                                break;
                            }
                            for expanded in expanded_tokens {
                                positional_tokens.insert(*pos, expanded);
                            }
                        }
                        AnchorType::Relative(rel) => {
                            // Relative anchor (^r1^token)
                            for expanded in expanded_tokens {
                                relative_tokens.push((*rel, expanded));
                            }
                        }
                        AnchorType::Middle { begin, end } => {
                            // Middle anchor (^2,4^token) - validate range
                            let begin_pos = begin.unwrap_or(2);
                            if begin_pos >= tokens_len {
                                has_conflicts = true;
                                break;
                            }
                            for expanded in expanded_tokens {
                                middle_tokens.push((*begin, *end, expanded));
                            }
                        }
                        AnchorType::End => {
                            // End anchor (token$)
                            end_tokens.extend(expanded_tokens);
                        }
                    }
                }
                Token::Simple(text) => {
                    let expanded_tokens = self.wildcard_expander.expand_wildcards(text);
                    non_anchored_tokens.extend(expanded_tokens);
                }
            }
        }
        
        // Return empty if there are anchor conflicts
        if has_conflicts {
            return arrangements;
        }

        // Sort relative tokens by their relative position
        relative_tokens.sort_by_key(|(rel, _)| *rel);

        // Calculate maximum possible length for proper positioning
        let total_tokens = tokens.len();
        
        // Generate arrangements respecting anchor constraints
        self.generate_valid_anchor_arrangements(
            total_tokens,
            positional_tokens,
            relative_tokens,
            middle_tokens,
            non_anchored_tokens,
            beginning_tokens,
            end_tokens,
        )
    }
    
    fn generate_valid_anchor_arrangements(
        &self,
        total_length: usize,
        positional_tokens: HashMap<usize, String>,
        relative_tokens: Vec<(usize, String)>,
        middle_tokens: Vec<(Option<usize>, Option<usize>, String)>,
        non_anchored_tokens: Vec<String>,
        beginning_tokens: Vec<String>,
        end_tokens: Vec<String>,
    ) -> Vec<Vec<String>> {
        let mut arrangements: Vec<Vec<String>> = Vec::new();
        
        // Validate positional anchors against total length
        for (&pos, _) in &positional_tokens {
            if pos >= total_length {
                return arrangements; // Invalid anchor position
            }
        }
        
        // Generate arrangements with proper anchor constraints
        let max_arrangements = if total_length <= 6 { 1000 } else { 100 };
        
        for begin_opt in if beginning_tokens.is_empty() { vec![None] } else { beginning_tokens.iter().map(Some).collect() } {
            for end_opt in if end_tokens.is_empty() { vec![None] } else { end_tokens.iter().map(Some).collect() } {
                if arrangements.len() >= max_arrangements {
                    break;
                }
                
                let arrangement = self.build_arrangement_with_constraints(
                    total_length,
                    &positional_tokens,
                    &relative_tokens,
                    &middle_tokens,
                    &non_anchored_tokens,
                    begin_opt,
                    end_opt,
                );
                
                if let Some(arr) = arrangement {
                    arrangements.push(arr);
                }
            }
        }
        
        arrangements
    }
    
    fn build_arrangement_with_constraints(
        &self,
        total_length: usize,
        positional_tokens: &HashMap<usize, String>,
        relative_tokens: &[(usize, String)],
        middle_tokens: &[(Option<usize>, Option<usize>, String)],
        non_anchored_tokens: &[String],
        begin_token: Option<&String>,
        end_token: Option<&String>,
    ) -> Option<Vec<String>> {
        let mut arrangement = vec![String::new(); total_length];
        let mut used_positions = std::collections::HashSet::new();
        
        // Place beginning token
        if let Some(token) = begin_token {
            arrangement[0] = token.clone();
            used_positions.insert(0);
        }
        
        // Place end token
        if let Some(token) = end_token {
            if total_length > 0 {
                arrangement[total_length - 1] = token.clone();
                used_positions.insert(total_length - 1);
            }
        }
        
        // Place positional tokens
        for (&pos, token) in positional_tokens {
            if pos < total_length && !used_positions.contains(&pos) {
                arrangement[pos] = token.clone();
                used_positions.insert(pos);
            } else {
                return None; // Conflict
            }
        }
        
        // Place relative tokens in order
        let mut available_positions: Vec<usize> = (0..total_length)
            .filter(|&pos| !used_positions.contains(&pos))
            .collect();
            
        for (_, token) in relative_tokens {
            if let Some(pos) = available_positions.first().copied() {
                arrangement[pos] = token.clone();
                used_positions.insert(pos);
                available_positions.remove(0);
            } else {
                return None; // No space
            }
        }
        
        // Place middle tokens (simplified - place in available middle positions)
        for (begin_opt, end_opt, token) in middle_tokens {
            let begin_pos = begin_opt.unwrap_or(1);
            let end_pos = end_opt.unwrap_or(total_length.saturating_sub(1));
            
            let valid_positions: Vec<usize> = available_positions
                .iter()
                .copied()
                .filter(|&pos| pos >= begin_pos && pos < end_pos && pos > 0 && pos < total_length - 1)
                .collect();
                
            if let Some(&pos) = valid_positions.first() {
                arrangement[pos] = token.clone();
                used_positions.insert(pos);
                available_positions.retain(|&p| p != pos);
            } else {
                return None; // No valid middle position
            }
        }
        
        // Place remaining non-anchored tokens
        let mut non_anchored_iter = non_anchored_tokens.iter();
        for pos in available_positions {
            if let Some(token) = non_anchored_iter.next() {
                arrangement[pos] = token.clone();
            }
        }
        
        // Filter out empty strings and return
        let result: Vec<String> = arrangement.into_iter().filter(|s| !s.is_empty()).collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }
}

struct PasswordIterator<'a> {
    generator: &'a PasswordGenerator,
    combinations: Vec<Vec<&'a Token>>,
    current_combination: usize,
    current_arrangements: Vec<Vec<String>>,
    current_arrangement: usize,
    duplicate_checker: Option<DuplicateChecker>,
}

impl<'a> PasswordIterator<'a> {
    fn new(generator: &'a PasswordGenerator) -> Self {
        let combinations = generator.generate_token_combinations();
        let duplicate_checker = if generator.config.no_dupchecks < 3 && generator.token_list.has_duplicates {
            Some(DuplicateChecker::new())
        } else {
            None
        };
        
        Self {
            generator,
            combinations,
            current_combination: 0,
            current_arrangements: Vec::new(),
            current_arrangement: 0,
            duplicate_checker,
        }
    }
}

impl<'a> Iterator for PasswordIterator<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we have arrangements for the current combination, return the next one
            if self.current_arrangement < self.current_arrangements.len() {
                let arrangement = &self.current_arrangements[self.current_arrangement];
                self.current_arrangement += 1;
                let password = arrangement.join("");
                
                // Apply truncation if configured
                if let Some(max_len) = self.generator.config.truncate_length {
                    if password.len() > max_len {
                        continue; // Skip passwords that are too long
                    }
                }
                
                // Handle seed generator mode - split tokens on commas
                let final_password = if self.generator.config.seedgenerator {
                    let parts: Vec<&str> = password.split(',').collect();
                    if let Some(expected_length) = self.generator.config.mnemonic_length {
                        if parts.len() != expected_length {
                            continue; // Skip if doesn't match expected mnemonic length
                        }
                    }
                    parts.join(" ") // Join with spaces for seed phrases
                } else {
                    password
                };
                
                // Check for duplicates if enabled
                if let Some(ref mut checker) = self.duplicate_checker {
                    if checker.is_duplicate(&final_password) {
                        continue; // Skip duplicate passwords
                    }
                }
                
                return Some(final_password);
            }

            // Move to the next combination
            if self.current_combination >= self.combinations.len() {
                // Finish the duplicate checking run if we have one
                if let Some(ref mut checker) = self.duplicate_checker {
                    checker.run_finished();
                }
                return None;
            }

            let combination = &self.combinations[self.current_combination];
            self.current_arrangements = self.generator.arrange_tokens(combination);
            self.current_combination += 1;
            self.current_arrangement = 0;

            // Continue to the next iteration to return the first arrangement
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_simple_token_parsing() {
        let input = "token1 token2 token3\ntoken4 token5";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        assert_eq!(token_list.lines.len(), 2);
        assert_eq!(token_list.lines[1].tokens.len(), 3); // Reversed order
        assert_eq!(token_list.lines[0].tokens.len(), 2);
    }

    #[test]
    fn test_required_tokens_parsing() {
        let input = "+ required1 required2\noptional1 optional2";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        assert_eq!(token_list.lines.len(), 2);
        assert!(token_list.lines[1].is_required); // Reversed order
        assert!(!token_list.lines[0].is_required);
    }

    #[test]
    fn test_anchored_tokens() {
        let input = "^beginning middle end$\n^2^second ^r1^relative";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        assert!(token_list.has_anchors);
        
        // Check specific anchor types
        let first_line = &token_list.lines[1]; // Reversed order
        if let Token::Anchored(anchored) = &first_line.tokens[0] {
            assert!(matches!(anchored.anchor_type, AnchorType::Positional(0)));
        }
    }

    #[test]
    fn test_wildcard_detection() {
        let input = "test%d\nhello%2a\nworld%[0-9]";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        assert!(token_list.has_wildcards);
    }

    #[test]
    fn test_duplicate_detection() {
        let input = "duplicate\nduplicate\nunique";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        assert!(token_list.has_duplicates);
    }

    #[test]
    fn test_middle_anchors() {
        let input = "^2,4^middle_token";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        let line = &token_list.lines[0];
        if let Token::Anchored(anchored) = &line.tokens[0] {
            if let AnchorType::Middle { begin, end } = &anchored.anchor_type {
                assert_eq!(*begin, Some(2));
                assert_eq!(*end, Some(4));
            } else {
                panic!("Expected middle anchor");
            }
        }
    }

    #[test]
    fn test_relative_anchors() {
        let input = "^r1^first ^r2^second";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        let line = &token_list.lines[0];
        if let Token::Anchored(anchored) = &line.tokens[0] {
            assert!(matches!(anchored.anchor_type, AnchorType::Relative(1)));
        }
    }

    #[test]
    fn test_comment_handling() {
        let input = "# This is a comment\nvalid_token\n# Another comment";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();

        assert_eq!(token_list.lines.len(), 1);
        assert_eq!(token_list.lines[0].tokens[0].text(), "valid_token");
    }

    #[test]
    fn test_custom_delimiter() {
        let input = "token1|token2|token3";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            delimiter: Some("|".to_string()),
            ..Default::default()
        };
        let parser = TokenListParser::new(config);
        let token_list = parser.parse_reader(cursor).unwrap();

        assert_eq!(token_list.lines[0].tokens.len(), 3);
    }

    #[test]
    fn test_password_generation_basic() {
        let input = "hello\nworld";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(10).collect();
        assert!(!passwords.is_empty());
        
        // Should contain individual tokens and combinations
        assert!(passwords.iter().any(|p| p.contains("hello")));
        assert!(passwords.iter().any(|p| p.contains("world")));
    }

    #[test]
    fn test_required_tokens() {
        let input = "+ required\noptional";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        // Required line should be marked as required
        assert!(token_list.lines.iter().any(|line| line.is_required));
    }

    #[test]
    fn test_duplicate_checker() {
        let mut checker = DuplicateChecker::new();
        
        // First occurrence should not be duplicate
        assert!(!checker.is_duplicate("test"));
        
        // Second occurrence should be duplicate
        assert!(checker.is_duplicate("test"));
        
        // Different string should not be duplicate
        assert!(!checker.is_duplicate("different"));
    }

    #[test]
    fn test_token_length_constraints() {
        let input = "a\nb\nc\nd";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            min_tokens: 2,
            max_tokens: 3,
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // All passwords should have between 2-3 tokens
        // This is a simplified test - full implementation would verify actual token counts
        assert!(!passwords.is_empty());
    }

    #[test]
    fn test_keep_tokens_order_basic() {
        let input = "first\nsecond\nthird";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            keep_tokens_order: true,
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(5).collect();
        assert!(!passwords.is_empty());
    }

    #[test]
    fn test_empty_lines_ignored() {
        let input = "token1\n\n\ntoken2\n\n";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        assert_eq!(token_list.lines.len(), 2);
    }

    #[test]
    fn test_mutual_exclusion() {
        let input = "option1 option2 option3";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        // All tokens on same line should be mutually exclusive
        assert_eq!(token_list.lines[0].tokens.len(), 3);
    }

    #[test]
    fn test_custom_wildcard_spaces() {
        let input = "test%[a b c]end";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        // Should be parsed as a single token with spaces in the wildcard
        assert_eq!(token_list.lines[0].tokens.len(), 1);
        assert_eq!(token_list.lines[0].tokens[0].text(), "test%[a b c]end");
    }

    #[test]
    fn test_complex_tokenlist_example() {
        let input = r#"# This is a comment
Cairo cairo Katmai katmai
+ Beetlejuice beetlejuice Betelgeuse betelgeuse
Hotel_california hotel_california"#;
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        assert_eq!(token_list.lines.len(), 3);
        
        // Check required line (reversed order due to parsing)
        assert!(token_list.lines[1].is_required);
        assert_eq!(token_list.lines[1].tokens.len(), 4);
        
        // Generate passwords and verify required token is always present
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        let passwords: Vec<String> = generator.generate_passwords().take(50).collect();
        
        // All passwords should contain at least one Beetlejuice variant
        for password in &passwords {
            assert!(
                password.contains("Beetlejuice") || 
                password.contains("beetlejuice") || 
                password.contains("Betelgeuse") || 
                password.contains("betelgeuse"),
                "Password '{}' doesn't contain required token", password
            );
        }
    }
    
    #[test]
    fn test_backreference_wildcards() {
        let input = "test%d%b";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(20).collect();
        
        // Should generate patterns like test00, test11, test22, etc.
        assert!(passwords.iter().any(|p| p.contains("test")));
        assert!(!passwords.is_empty());
    }
    
    #[test]
    fn test_contracting_wildcards() {
        let input = "Start%0,2-End";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should include variations with characters removed
        assert!(!passwords.is_empty());
        assert!(passwords.iter().any(|p| p.contains("Start") && p.contains("End")));
    }
    
    #[test]
    fn test_character_set_wildcards() {
        let input = "test%[abc]end";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should generate testaend, testbend, testcend
        assert!(passwords.contains(&"testaend".to_string()));
        assert!(passwords.contains(&"testbend".to_string()));
        assert!(passwords.contains(&"testcend".to_string()));
        assert_eq!(passwords.len(), 3);
    }
    
    #[test]
    fn test_range_character_set_wildcards() {
        let input = "test%[0-2]";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should generate test0, test1, test2
        assert!(passwords.contains(&"test0".to_string()));
        assert!(passwords.contains(&"test1".to_string()));
        assert!(passwords.contains(&"test2".to_string()));
        assert_eq!(passwords.len(), 3);
    }
    
    #[test]
    fn test_multiple_wildcards() {
        let input = "test%d%a";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(50).collect();
        
        // Should generate combinations like test0a, test1b, etc.
        assert!(passwords.iter().any(|p| p.starts_with("test") && p.len() == 6));
        assert_eq!(passwords.len(), 50); // Limited to 50
    }
    
    #[test]
    fn test_case_insensitive_wildcards() {
        let input = "test%ia";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(60).collect();
        
        // Should generate both lowercase and uppercase letters
        assert!(passwords.iter().any(|p| p.contains("testa")));
        assert!(passwords.iter().any(|p| p.contains("testA")));
        assert!(passwords.iter().any(|p| p.contains("testz")));
        assert!(passwords.iter().any(|p| p.contains("testZ")));
    }
    
    #[test]
    fn test_escape_wildcards() {
        let input = "test%%end";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should generate test%end (literal %)
        assert_eq!(passwords.len(), 1);
        assert_eq!(passwords[0], "test%end");
    }
    
    #[test]
    fn test_duplicate_checking_integration() {
        let input = "duplicate\nduplicate\nunique";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            no_dupchecks: 0, // Enable duplicate checking
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let has_duplicates = token_list.has_duplicates;
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should detect and handle duplicates
        assert!(has_duplicates);
        assert!(!passwords.is_empty());
    }
    
    #[test]
    fn test_complex_anchor_positioning() {
        let input = "^beginning ^2^second middle ^r1^relative end$";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        assert!(token_list.has_anchors);
        
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should generate passwords respecting anchor constraints
        assert!(!passwords.is_empty());
        
        for password in &passwords {
            if password.contains("beginning") && password.contains("end") {
                assert!(password.starts_with("beginning"));
                assert!(password.ends_with("end"));
            }
        }
    }
    
    #[test]
    fn test_middle_anchor_constraints() {
        let input = "start ^2,4^middle end";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        let line = &token_list.lines[0];
        if let Token::Anchored(anchored) = &line.tokens[1] {
            if let AnchorType::Middle { begin, end } = &anchored.anchor_type {
                assert_eq!(*begin, Some(2));
                assert_eq!(*end, Some(4));
                assert_eq!(anchored.text, "middle");
            } else {
                panic!("Expected middle anchor");
            }
        }
    }
    
    #[test]
    fn test_comprehensive_wildcard_types() {
        let test_cases = vec![
            ("test%d", 10),     // digits 0-9
            ("test%a", 26),     // lowercase a-z
            ("test%A", 26),     // uppercase A-Z
            ("test%s", 1),      // space
            ("test%H", 16),     // hex 0-9A-F
            ("test%B", 58),     // Base58 characters
        ];
        
        for (input, expected_count) in test_cases {
            let cursor = Cursor::new(input);
            let parser = TokenListParser::new(TokenListConfig::default());
            let token_list = parser.parse_reader(cursor).unwrap();
            let config = TokenListConfig::default();
            let generator = PasswordGenerator::new(token_list, config);
            
            let passwords: Vec<String> = generator.generate_passwords().collect();
            assert_eq!(passwords.len(), expected_count, "Failed for input: {}", input);
        }
    }
    
    #[test]
    fn test_seed_generator_mode() {
        let input = "word1,word2,word3\ntest1,test2,test3";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            seedgenerator: true,
            mnemonic_length: Some(3),
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(10).collect();
        
        // Should generate space-separated seed phrases
        assert!(!passwords.is_empty());
        for password in &passwords {
            assert!(password.contains(" ")); // Should have spaces between words
            let word_count = password.split_whitespace().count();
            assert_eq!(word_count, 3); // Should have exactly 3 words
        }
    }
    
    #[test]
    fn test_advanced_anchor_validation() {
        // Test conflicting positional anchors
        let input = "^2^conflict1 ^2^conflict2";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should handle conflicts gracefully (may be empty or limited)
        // This tests the conflict detection logic
        assert!(passwords.len() <= 1);
    }
    
    #[test]
    fn test_middle_anchor_never_at_ends() {
        let input = "start ^2,4^middle end";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Middle anchors should never appear at the beginning or end
        for password in &passwords {
            if password.contains("middle") {
                assert!(!password.starts_with("middle"));
                assert!(!password.ends_with("middle"));
            }
        }
    }
    
    #[test]
    fn test_token_order_preservation() {
        let input = "first\nsecond\nthird";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            keep_tokens_order: true,
            min_tokens: 3,
            max_tokens: 3,
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should maintain token order (note: lines are reversed during parsing)
        assert!(passwords.iter().any(|p| p == "thirdsecondfirst"));
    }
    
    #[test]
    fn test_python_compatibility_example() {
        // Test the exact example from Python documentation
        let input = r#"# This is a comment
Cairo cairo Katmai katmai
+ Beetlejuice beetlejuice Betelgeuse betelgeuse
Hotel_california hotel_california"#;
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        // Verify structure matches Python expectations
        assert_eq!(token_list.lines.len(), 3);
        
        // Required line (Beetlejuice variations)
        let required_line = &token_list.lines[1]; // Reversed order
        assert!(required_line.is_required);
        assert_eq!(required_line.tokens.len(), 4);
        
        // Generate passwords and verify required token is always present
        let config = TokenListConfig {
            min_tokens: 1,
            max_tokens: 3,
            ..Default::default()
        };
        let generator = PasswordGenerator::new(token_list, config);
        let passwords: Vec<String> = generator.generate_passwords().take(50).collect();
        
        // All passwords should contain at least one Beetlejuice variant
        for password in &passwords {
            assert!(
                password.contains("Beetlejuice") || 
                password.contains("beetlejuice") || 
                password.contains("Betelgeuse") || 
                password.contains("betelgeuse"),
                "Password '{}' doesn't contain required token", password
            );
        }
    }

    #[test]
    fn test_password_generation_with_required_tokens() {
        let input = "+ required\noptional1 optional2";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            min_tokens: 1,
            max_tokens: 2,
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(10).collect();
        
        // All passwords should contain the required token
        assert!(passwords.iter().all(|p| p.contains("required")));
        assert!(!passwords.is_empty());
    }

    #[test]
    fn test_wildcard_expansion_integration() {
        let input = "test%2d\nhello%a";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(200).collect();
        
        // Should have passwords with 2-digit expansions
        assert!(passwords.iter().any(|p| p.contains("test00")));
        assert!(passwords.iter().any(|p| p.contains("test99")));
        
        // Should have passwords with letter expansions
        assert!(passwords.iter().any(|p| p.contains("helloa")));
        assert!(passwords.iter().any(|p| p.contains("helloz")));
    }

    #[test]
    fn test_anchor_combinations() {
        let input = "^beginning middle end$";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        assert!(token_list.has_anchors);
        
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(token_list, config);
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should generate passwords with proper anchor ordering
        assert!(!passwords.is_empty());
        
        // Check that beginning and end anchors are respected
        for password in &passwords {
            if password.contains("beginning") && password.contains("end") {
                assert!(password.starts_with("beginning"));
                assert!(password.ends_with("end"));
            }
        }
    }

    #[test]
    fn test_token_count_constraints() {
        let input = "a\nb\nc\nd\ne";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            min_tokens: 2,
            max_tokens: 3,
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(100).collect();
        
        // All passwords should have between 2-3 characters (tokens)
        for password in &passwords {
            assert!(password.len() >= 2 && password.len() <= 3);
        }
    }

    #[test]
    fn test_truncation_length() {
        let input = "verylongtoken\nshort";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            truncate_length: Some(5),
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // All passwords should be 5 characters or less
        for password in &passwords {
            assert!(password.len() <= 5);
        }
    }

    #[test]
    fn test_keep_tokens_order() {
        let input = "first\nsecond\nthird";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            keep_tokens_order: true,
            min_tokens: 3,
            max_tokens: 3,
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should maintain original order (note: lines are reversed during parsing)
        assert!(passwords.iter().any(|p| p == "thirdsecondfirst"));
    }

    #[test]
    fn test_embedded_options_warning() {
        let input = "token1\n# --some-option value\ntoken2";
        let cursor = Cursor::new(input);
        let parser = TokenListParser::new(TokenListConfig::default());
        let token_list = parser.parse_reader(cursor).unwrap();
        
        // Should parse successfully and ignore embedded options with warning
        assert_eq!(token_list.lines.len(), 2);
        assert_eq!(token_list.lines[1].tokens[0].text(), "token1");
        assert_eq!(token_list.lines[0].tokens[0].text(), "token2");
    }
    
    #[test]
    fn test_performance_with_large_tokenlist() {
        // Create a larger tokenlist to test performance
        let mut input = String::new();
        for i in 0..20 {
            input.push_str(&format!("token{}\n", i));
        }
        
        let cursor = Cursor::new(&input);
        let config = TokenListConfig {
            max_tokens: 3, // Limit to prevent exponential explosion
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().take(1000).collect();
        
        // Should generate passwords efficiently
        assert!(!passwords.is_empty());
        assert!(passwords.len() <= 1000);
    }

    #[test]
    fn test_token_combination_duplicate_checking() {
        let input = "duplicate\nduplicate\nunique";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            no_dupchecks: 0, // Enable all duplicate checking
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let has_duplicates = token_list.has_duplicates;
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // Should detect duplicates and handle them appropriately
        assert!(has_duplicates);
        
        // Check that we don't have obvious duplicates in the output
        let mut unique_passwords = std::collections::HashSet::new();
        for password in &passwords {
            assert!(unique_passwords.insert(password.clone()), 
                   "Found duplicate password: {}", password);
        }
    }
    
    #[test]
    fn test_truncation_functionality() {
        let input = "verylongtoken\nshort";
        let cursor = Cursor::new(input);
        let config = TokenListConfig {
            truncate_length: Some(8),
            ..Default::default()
        };
        let parser = TokenListParser::new(config.clone());
        let token_list = parser.parse_reader(cursor).unwrap();
        let generator = PasswordGenerator::new(token_list, config);
        
        let passwords: Vec<String> = generator.generate_passwords().collect();
        
        // All passwords should be 8 characters or less
        for password in &passwords {
            assert!(password.len() <= 8, "Password '{}' exceeds truncation limit", password);
        }
    }

}

impl TokenList {
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let parser = TokenListParser::new(TokenListConfig::default());
        parser.parse_file(path)
    }

    pub fn generate_passwords(&self) -> Vec<String> {
        let config = TokenListConfig::default();
        let generator = PasswordGenerator::new(self.clone(), config);
        generator.generate_passwords().collect()
    }

    pub fn generate_passwords_with_config(self, config: TokenListConfig) -> Vec<String> {
        let generator = PasswordGenerator::new(self, config);
        generator.generate_passwords().collect()
    }
    
    /// Check if the token list has any syntax errors or warnings
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        
        // Check for empty tokens
        for (line_idx, line) in self.lines.iter().enumerate() {
            for token in &line.tokens {
                if token.text().is_empty() {
                    warnings.push(format!("Line {}: Token contains only anchor (no password characters)", line_idx + 1));
                }
            }
        }
        
        // Check for potential anchor conflicts
        if self.has_anchors {
            warnings.push("Token list contains anchors - ensure proper positioning".to_string());
        }
        
        // Check for wildcards that might generate many combinations
        if self.has_wildcards {
            warnings.push("Token list contains wildcards - password generation may be extensive".to_string());
        }
        
        warnings
    }
}
