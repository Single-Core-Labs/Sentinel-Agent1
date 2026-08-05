use anyhow::Result;
use sentinel_config::config_json_schema;

/// `sentinel schema` — print the JSON Schema for sentinel.toml.
///
/// Consumers: IDEs and editors use it for validation and autocompletion of
/// configuration files (e.g. via "json.schemas" / redhat.vscode-yaml).
pub fn run(args: &[String]) -> Result<()> {
    let pretty = !args.iter().any(|a| a == "--compact" || a == "-c");
    let json = config_json_schema();
    if pretty {
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", json);
    }
    Ok(())
}
