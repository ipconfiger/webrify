//! `webrify sitekey` subcommand — manage `allowed_origins` in a TOML config.
//!
//! Usage:
//!   `webrify sitekey list [--config <path>]`
//!   `webrify sitekey add <origin> [--config <path>]`
//!   `webrify sitekey remove <origin> [--config <path>]`
//!
//! Reads/modifies the TOML as a generic `toml::Value` so all other fields are
//! preserved. Changes take effect on the next server restart.

use std::fs;
use std::path::PathBuf;

/// Run the `sitekey` subcommand. `args` is everything after `sitekey` in argv.
pub fn sitekey(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = extract_config_path(args).unwrap_or_else(|| PathBuf::from("webrify.toml"));
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let action = positional
        .first()
        .map(|s| s.as_str())
        .ok_or("missing action: use `add <origin>`, `remove <origin>`, or `list`")?;

    match action {
        "list" => list(&config_path),
        "add" => {
            let origin = positional
                .get(1)
                .map(|s| s.as_str())
                .ok_or("missing origin to add")?;
            modify(&config_path, |origins| {
                if !origins.iter().any(|o| o == origin) {
                    origins.push(origin.to_string());
                }
            })
        }
        "remove" => {
            let origin = positional
                .get(1)
                .map(|s| s.as_str())
                .ok_or("missing origin to remove")?;
            modify(&config_path, |origins| origins.retain(|o| o != origin))
        }
        other => Err(format!("unknown action `{other}`: use add, remove, or list").into()),
    }
}

fn extract_config_path(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            return iter.next().map(PathBuf::from);
        }
    }
    None
}

fn list(config_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let val = read_toml(config_path)?;
    let origins = extract_origins(&val);
    if origins.is_empty() {
        println!("(no allowed_origins in {})", config_path.display());
    } else {
        for origin in &origins {
            println!("{origin}");
        }
    }
    Ok(())
}

fn modify<F: FnOnce(&mut Vec<String>)>(
    config_path: &PathBuf,
    f: F,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut val = read_toml(config_path)?;
    let mut origins = extract_origins(&val);
    f(&mut origins);
    let arr: Vec<toml::Value> = origins
        .iter()
        .map(|s| toml::Value::String(s.clone()))
        .collect();
    if let Some(table) = val.as_table_mut() {
        table.insert("allowed_origins".into(), toml::Value::Array(arr));
    }
    fs::write(config_path, toml::to_string_pretty(&val)?)?;
    println!(
        "updated {} — {} allowed_origins",
        config_path.display(),
        origins.len()
    );
    Ok(())
}

fn read_toml(path: &PathBuf) -> Result<toml::Value, Box<dyn std::error::Error>> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    Ok(toml::from_str(&text)?)
}

fn extract_origins(val: &toml::Value) -> Vec<String> {
    val.get("allowed_origins")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
