use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde_json::json;
use tapd_client::TapdClient;
use tapd_domain_demo::{DemoConfig, DemoRequestInput, DemoRequestService};

use crate::{
    cli::{Cli, DemoArgs, DemoCommand, DemoRequestCommand, DemoRequestInputArgs},
    commands::CommandOutput,
    config::LoadedConfig,
    content::read_content,
};

pub async fn run(cli: &Cli, args: &DemoArgs) -> Result<CommandOutput> {
    match &args.command {
        DemoCommand::Request(args) => run_request(cli, &args.command).await,
    }
}

async fn run_request(cli: &Cli, command: &DemoRequestCommand) -> Result<CommandOutput> {
    let loaded = LoadedConfig::load(cli.config.as_ref())?;
    let value = loaded
        .file
        .organizations
        .get("demo")
        .cloned()
        .ok_or_else(|| anyhow!("organizations.demo is not configured"))?;
    let demo_config: DemoConfig = value
        .try_into()
        .context("invalid organizations.demo configuration")?;
    demo_config
        .request
        .validate()
        .context("invalid organizations.demo.request configuration")?;

    let needs_api = matches!(command, DemoRequestCommand::Submit(_)) && !cli.dry_run;
    let profile = loaded.resolve_profile(
        cli.profile.as_deref(),
        demo_config.request.profile.as_deref(),
        cli.workspace_id.as_deref(),
        needs_api,
    )?;
    let workspace_id = cli
        .workspace_id
        .clone()
        .or(profile.config.workspace_id.clone())
        .or(demo_config.request.workspace_id.clone())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("demo.request requires a workspace id"))?;

    let mut builder = TapdClient::builder()
        .api_base_url(profile.config.api_base_url)
        .web_base_url(profile.config.web_base_url)
        .timeout(Duration::from_secs(profile.config.timeout_seconds));
    if let Some(token) = profile.access_token {
        builder = builder.access_token(token.as_str().to_owned());
    }
    let service = DemoRequestService::new(builder.build()?, workspace_id, demo_config.request)?;

    match command {
        DemoRequestCommand::Plan(args) => {
            let input = request_input(args).await?;
            Ok(CommandOutput::Json(serde_json::to_value(
                service.plan(&input)?,
            )?))
        }
        DemoRequestCommand::Submit(args) => {
            let input = request_input(args).await?;
            if cli.dry_run {
                return Ok(CommandOutput::Json(json!({
                    "dry_run": true,
                    "plan": service.plan(&input)?,
                })));
            }
            Ok(CommandOutput::Json(serde_json::to_value(
                service.submit(&input).await?,
            )?))
        }
    }
}

async fn request_input(args: &DemoRequestInputArgs) -> Result<DemoRequestInput> {
    Ok(DemoRequestInput {
        subject: args.subject.clone(),
        summary: args.summary.clone(),
        description: read_content(&args.content, false).await?,
        owner: args.owner.clone(),
    })
}
