use clap::{Arg, Command};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::time::Instant;

use crate::{BtcRecoverTokenList, TokenListConfig, Result};

pub struct CliApp {
    generator: BtcRecoverTokenList,
}

impl CliApp {
    pub fn new() -> Self {
        Self {
            generator: BtcRecoverTokenList::default(),
        }
    }

    pub fn run() -> Result<()> {
        let matches = Self::build_cli().get_matches();
        let mut app = Self::new();
        app.execute_command(matches)
    }

    fn build_cli() -> Command {
        Command::new("btcrecover-tokenlist")
            .version("0.1.0")
            .author("BTCRecover Contributors")
            .about("Rust implementation of BTCRecover token list password generation")
            .arg(
                Arg::new("tokenlist")
                    .short('t')
                    .long("tokenlist")
                    .value_name("FILE")
                    .help("Path to the token list file")
                    .required(true)
            )
            .arg(
                Arg::new("max-tokens")
                    .long("max-tokens")
                    .value_name("COUNT")
                    .help("Maximum number of tokens per password")
                    .value_parser(clap::value_parser!(usize))
                    .default_value("10")
            )
            .arg(
                Arg::new("min-tokens")
                    .long("min-tokens")
                    .value_name("COUNT")
                    .help("Minimum number of tokens per password")
                    .value_parser(clap::value_parser!(usize))
                    .default_value("1")
            )
            .arg(
                Arg::new("delimiter")
                    .short('d')
                    .long("delimiter")
                    .value_name("DELIMITER")
                    .help("Token delimiter (default: whitespace)")
            )
            .arg(
                Arg::new("keep-order")
                    .long("keep-tokens-order")
                    .help("Keep tokens in original order")
                    .action(clap::ArgAction::SetTrue)
            )
            .arg(
                Arg::new("limit")
                    .short('l')
                    .long("limit")
                    .value_name("COUNT")
                    .help("Limit number of passwords to generate")
                    .value_parser(clap::value_parser!(usize))
            )
            .arg(
                Arg::new("output")
                    .short('o')
                    .long("output")
                    .value_name("FILE")
                    .help("Output file (default: stdout)")
            )
            .arg(
                Arg::new("benchmark")
                    .short('b')
                    .long("benchmark")
                    .help("Run benchmark mode")
                    .action(clap::ArgAction::SetTrue)
            )
            .arg(
                Arg::new("custom-wild")
                    .long("custom-wild")
                    .value_name("CHARACTERS")
                    .help("Custom wildcard characters for %c")
            )
            .arg(
                Arg::new("no-dupchecks")
                    .long("no-dupchecks")
                    .help("Disable duplicate checking (can be specified multiple times)")
                    .action(clap::ArgAction::Count)
            )
            .arg(
                Arg::new("mnemonic-length")
                    .long("mnemonic-length")
                    .value_name("LENGTH")
                    .help("Expected mnemonic length for seed recovery")
                    .value_parser(clap::value_parser!(usize))
            )
            .arg(
                Arg::new("seedgenerator")
                    .long("seedgenerator")
                    .help("Enable seed generator mode")
                    .action(clap::ArgAction::SetTrue)
            )
            .arg(
                Arg::new("truncate-length")
                    .long("truncate-length")
                    .value_name("COUNT")
                    .help("Truncate passwords to be this number of characters long")
                    .value_parser(clap::value_parser!(usize))
            )
            .arg(
                Arg::new("password-repeats")
                    .long("password-repeats")
                    .help("Test multiple repetitions of each candidate password")
                    .action(clap::ArgAction::SetTrue)
            )
            .arg(
                Arg::new("max-password-repeats")
                    .long("max-password-repeats")
                    .value_name("COUNT")
                    .help("Max number of additional repetitions of the password to produce")
                    .value_parser(clap::value_parser!(usize))
                    .default_value("2")
            )
            .arg(
                Arg::new("seed-transform-wordswaps")
                    .long("seed-transform-wordswaps")
                    .value_name("COUNT")
                    .help("Try swapped words for seed generation")
                    .value_parser(clap::value_parser!(usize))
            )
            .arg(
                Arg::new("wildcard-custom-list-e")
                    .long("wildcard-custom-list-e")
                    .value_name("FILE")
                    .help("Path to a custom list file for the %e expanding wildcard")
            )
            .arg(
                Arg::new("wildcard-custom-list-f")
                    .long("wildcard-custom-list-f")
                    .value_name("FILE")
                    .help("Path to a custom list file for the %f expanding wildcard")
            )
            .arg(
                Arg::new("wildcard-custom-list-j")
                    .long("wildcard-custom-list-j")
                    .value_name("FILE")
                    .help("Path to a custom list file for the %j expanding wildcard")
            )
            .arg(
                Arg::new("wildcard-custom-list-k")
                    .long("wildcard-custom-list-k")
                    .value_name("FILE")
                    .help("Path to a custom list file for the %k expanding wildcard")
            )
            .arg(
                Arg::new("regex-only")
                    .long("regex-only")
                    .value_name("REGEX")
                    .help("Only try passwords which match the given regular expression")
            )
            .arg(
                Arg::new("regex-never")
                    .long("regex-never")
                    .value_name("REGEX")
                    .help("Never try passwords which match the given regular expression")
            )
            .arg(
                Arg::new("length-min")
                    .long("length-min")
                    .value_name("COUNT")
                    .help("Skip passwords shorter than given length")
                    .value_parser(clap::value_parser!(usize))
                    .default_value("0")
            )
            .arg(
                Arg::new("length-max")
                    .long("length-max")
                    .value_name("COUNT")
                    .help("Skip passwords longer than given length")
                    .value_parser(clap::value_parser!(usize))
                    .default_value("999999")
            )
    }

    fn execute_command(&mut self, matches: clap::ArgMatches) -> Result<()> {
        let config = self.parse_config(&matches)?;
        self.generator.set_config(config.clone());

        let tokenlist_path = matches.get_one::<String>("tokenlist").unwrap();
        let benchmark = matches.get_flag("benchmark");
        let limit = matches.get_one::<usize>("limit").copied();
        let output_file = matches.get_one::<String>("output").cloned();

        if benchmark {
            self.run_benchmark(tokenlist_path)
        } else {
            self.generate_passwords(tokenlist_path, limit, output_file, &config)
        }
    }

    fn parse_config(&self, matches: &clap::ArgMatches) -> Result<TokenListConfig> {
        let max_tokens = *matches.get_one::<usize>("max-tokens").unwrap();
        let min_tokens = *matches.get_one::<usize>("min-tokens").unwrap();
        let delimiter = matches.get_one::<String>("delimiter").cloned();
        let keep_order = matches.get_flag("keep-order");
        let custom_wild = matches.get_one::<String>("custom-wild").cloned();
        let no_dupchecks = matches.get_count("no-dupchecks");
        let mnemonic_length = matches.get_one::<usize>("mnemonic-length").copied();
        let seedgenerator = matches.get_flag("seedgenerator");
        let truncate_length = matches.get_one::<usize>("truncate-length").copied();
        let password_repeats = matches.get_flag("password-repeats");
        let max_password_repeats = matches.get_one::<usize>("max-password-repeats").copied();
        let seed_transform_wordswaps = matches.get_one::<usize>("seed-transform-wordswaps").copied();
        let length_min = *matches.get_one::<usize>("length-min").unwrap();
        let length_max = *matches.get_one::<usize>("length-max").unwrap();
        let regex_only = matches.get_one::<String>("regex-only").cloned();
        let regex_never = matches.get_one::<String>("regex-never").cloned();
        
        // Collect wildcard custom list files
        let mut wildcard_custom_lists = HashMap::new();
        if let Some(file) = matches.get_one::<String>("wildcard-custom-list-e") {
            wildcard_custom_lists.insert('e', file.clone());
        }
        if let Some(file) = matches.get_one::<String>("wildcard-custom-list-f") {
            wildcard_custom_lists.insert('f', file.clone());
        }
        if let Some(file) = matches.get_one::<String>("wildcard-custom-list-j") {
            wildcard_custom_lists.insert('j', file.clone());
        }
        if let Some(file) = matches.get_one::<String>("wildcard-custom-list-k") {
            wildcard_custom_lists.insert('k', file.clone());
        }

        Ok(TokenListConfig {
            delimiter,
            min_tokens,
            max_tokens,
            keep_tokens_order: keep_order,
            no_dupchecks,
            truncate_length,
            password_repeats: if password_repeats { max_password_repeats } else { None },
            mnemonic_length,
            seedgenerator,
            custom_wild_chars: custom_wild,
            seed_transform_wordswaps,
            wildcard_custom_lists: if wildcard_custom_lists.is_empty() { None } else { Some(wildcard_custom_lists) },
            length_min: Some(length_min),
            length_max: Some(length_max),
            regex_only,
            regex_never,
        })
    }

    fn generate_passwords(
        &self,
        tokenlist_path: &str,
        limit: Option<usize>,
        output_file: Option<String>,
        config: &TokenListConfig,
    ) -> Result<()> {
        let generator = self.generator.generate_from_file_iter(tokenlist_path)?;
        let password_iter = generator.generate_passwords();

        // Handle output to file or stdout
        let mut writer: Box<dyn Write> = if let Some(output_path) = output_file {
            Box::new(File::create(output_path)?)
        } else {
            Box::new(io::stdout())
        };

        let mut count = 0;
        let mut skipped = 0;
        
        let password_iter: Box<dyn Iterator<Item = String>> = if let Some(limit) = limit {
            Box::new(password_iter.take(limit))
        } else {
            Box::new(password_iter)
        };
        
        for password in password_iter {
            // Apply length filters
            if let Some(min_len) = config.length_min {
                if password.len() < min_len {
                    skipped += 1;
                    continue;
                }
            }
            if let Some(max_len) = config.length_max {
                if password.len() > max_len {
                    skipped += 1;
                    continue;
                }
            }
            
            // Apply regex filters
            if let Some(regex_only) = &config.regex_only {
                if let Ok(re) = regex::Regex::new(regex_only) {
                    if !re.is_match(&password) {
                        skipped += 1;
                        continue;
                    }
                }
            }
            if let Some(regex_never) = &config.regex_never {
                if let Ok(re) = regex::Regex::new(regex_never) {
                    if re.is_match(&password) {
                        skipped += 1;
                        continue;
                    }
                }
            }
            
            // Apply truncation if specified
            let final_password = if let Some(truncate_len) = config.truncate_length {
                if password.len() > truncate_len {
                    password.chars().take(truncate_len).collect()
                } else {
                    password
                }
            } else {
                password
            };
            
            writeln!(writer, "{}", final_password)?;
            count += 1;
        }
        
        if skipped > 0 {
            eprintln!("Generated {} passwords, skipped {} due to filters", count, skipped);
        } else {
            eprintln!("Generated {} passwords", count);
        }

        Ok(())
    }

    fn run_benchmark(&self, tokenlist_path: &str) -> Result<()> {
        println!("Running benchmark...");
        let start = Instant::now();
        
        let generator = self.generator.generate_from_file_iter(tokenlist_path)?;
        let password_iter = generator.generate_passwords();
        let count = password_iter.take(100_000).count();
        
        let duration = start.elapsed();
        let passwords_per_second = count as f64 / duration.as_secs_f64();
        
        println!("Generated {} passwords in {:?}", count, duration);
        println!("Rate: {:.2} passwords/second", passwords_per_second);
        
        Ok(())
    }
}

impl Default for CliApp {
    fn default() -> Self {
        Self::new()
    }
}
