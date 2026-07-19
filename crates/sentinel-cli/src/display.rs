use colored::*;

pub fn print_banner() {
    println!();
    println!("{}", "╔══════════════════════════════════════╗".green());
    println!("{}", "║        Sentinel Agent v0.1.0         ║".green().bold());
    println!("{}", "╚══════════════════════════════════════╝".green());
    println!();
}

pub fn print_divider() {
    println!("{}", "────────────────────────────────────────────".dimmed());
}
