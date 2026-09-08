mod cli;
mod commands;
mod config;
mod content;
mod credential;
mod skill;

use std::process::ExitCode;

use clap::Parser;
use serde_json::json;

use crate::{cli::Cli, commands::CommandOutput};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let pretty = cli.output.is_pretty();
    match commands::run(&cli).await {
        Ok(CommandOutput::Json(value)) => {
            let result = json!({"ok": true, "data": value});
            let rendered = if pretty {
                serde_json::to_string_pretty(&result)
            } else {
                serde_json::to_string(&result)
            };
            match rendered {
                Ok(rendered) => {
                    println!("{rendered}");
                    ExitCode::SUCCESS
                }
                Err(error) => print_error(&error),
            }
        }
        Ok(CommandOutput::Plain(value)) => {
            print!("{value}");
            if !value.ends_with('\n') {
                println!();
            }
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error.as_ref()),
    }
}

fn print_error(error: &dyn std::error::Error) -> ExitCode {
    let response = json!({
        "ok": false,
        "error": {
            "code": "command_failed",
            "message": error.to_string(),
        }
    });
    eprintln!(
        "{}",
        serde_json::to_string(&response).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"code\":\"serialization_failed\"}}".into()
        })
    );
    ExitCode::FAILURE
}
