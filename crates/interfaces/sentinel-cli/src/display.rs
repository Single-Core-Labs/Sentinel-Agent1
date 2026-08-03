use colored::*;

pub fn print_banner() {
    println!();
    println!(
        "{}",
        "  ╭──────────────────────────────────────────╮"
            .bright_white()
            .dimmed()
    );
    println!(
        "  {} {}",
        "│".bright_white().dimmed(),
        "           Sentinel Agent                     "
            .bright_white()
            .bold()
    );
    println!(
        "{}",
        "  ╰──────────────────────────────────────────╯"
            .bright_white()
            .dimmed()
    );
    println!();
}

pub fn print_divider() {
    println!(
        "{}",
        "  ────────────────────────────────────────────".dimmed()
    );
}

pub fn print_error(msg: &str) {
    eprintln!();
    eprintln!(" {} {}", "✖ Error:".red().bold(), msg);
    if msg.contains("API key") || msg.contains("401") || msg.contains("403") {
        eprintln!(
            "   {}",
            "Hint: Set the corresponding env var (see --help for provider list)".yellow()
        );
    } else if msg.contains("timed out") || msg.contains("timeout") {
        eprintln!(
            "   {}",
            "Hint: The request timed out. Try a smaller prompt or check your connection.".yellow()
        );
    } else if msg.contains("404") {
        eprintln!(
            "   {}",
            "Hint: The model may not exist or the base URL is wrong.".yellow()
        );
    }
}
