use std::io::{self, Write};
use colored::*;

/// Reads one line from stdin after printing `label`. Returns `None` on EOF
/// (closed/redirected stdin) so callers can abort instead of spinning forever
/// re-reading an empty string.
pub fn read_line(label: &str) -> anyhow::Result<Option<String>> {
    print!("{label}");
    io::stdout().flush()?;

    let mut input = String::new();
    let bytes = io::stdin().read_line(&mut input)?;
    if bytes == 0 {
        return Ok(None);
    }
    Ok(Some(input.trim().to_string()))
}

/// Prompts until a valid 1-based index in `[1, len]` is entered.
/// Returns `None` on EOF.
pub fn read_choice(label: &str, len: usize) -> anyhow::Result<Option<usize>> {
    loop {
        match read_line(label)? {
            None => return Ok(None),
            Some(s) => {
                if let Ok(idx) = s.parse::<usize>() {
                    if idx > 0 && idx <= len {
                        return Ok(Some(idx));
                    }
                }
                println!("{}", "Invalid selection. Try again.".red());
            }
        }
    }
}

/// Prompts until a non-empty, plausible-looking API key is entered.
/// Returns `None` on EOF.
pub fn read_api_key(provider_name: &str) -> anyhow::Result<Option<String>> {
    loop {
        match read_line(&format!("API Key for {}: ", provider_name))? {
            None => return Ok(None),
            Some(key) if key.is_empty() => {
                println!("{}", "API key cannot be empty.".red());
            }
            Some(key) if key.len() < 10 => {
                println!("{}", "API key seems too short.".red());
            }
            Some(key) => return Ok(Some(key)),
        }
    }
}

/// Yes/no prompt defaulting to yes on blank input. Returns `None` on EOF.
pub fn read_yes_no(label: &str) -> anyhow::Result<Option<bool>> {
    match read_line(label)? {
        None => Ok(None),
        Some(s) => Ok(Some(s.to_lowercase() != "n")),
    }
}
