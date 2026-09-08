#[cfg(feature = "demo")]
mod demo;

use std::{collections::BTreeMap, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use tapd_client::{AttachmentUpload, ImageUpload, Params, TapdClient, record, records};
use zeroize::Zeroizing;

use crate::{
    cli::{
        AttachmentCommand, AuthCommand, Cli, CommentCommand, ConfigCommand, DescriptionArgs,
        ImageCommand, MetadataCommand, PaginationArgs, RootCommand, SkillCommand, StoryCommand,
        StoryCreateArgs, StoryFilterArgs, StoryListArgs, StoryTransitionArgs, StoryUpdateArgs,
        UrlCommand, WorkflowCommand, WorkflowQueryArgs, WorkspaceCommand,
    },
    config::{EXAMPLE_CONFIG, LoadedConfig, ResolvedProfile, discover_path, write_example},
    content::{
        format_content, parse_inline_image_specs, read_content, read_raw_content,
        replace_inline_image_placeholder, validate_inline_image_file,
        validate_inline_image_placeholders, validate_readable_file,
    },
    credential::{self, KEYCHAIN_SERVICE},
    skill::{self, SkillOutput},
};

#[derive(Debug)]
pub enum CommandOutput {
    Json(Value),
    Plain(String),
}

#[derive(Debug)]
struct ApiContext {
    client: TapdClient,
    workspace_id: Option<String>,
}

pub async fn run(cli: &Cli) -> Result<CommandOutput> {
    match &cli.command {
        RootCommand::Config(args) => return run_config(cli, &args.command),
        RootCommand::Auth(args) => return run_auth(cli, &args.command).await,
        RootCommand::Url(args) => return run_url(&args.command),
        RootCommand::Skill(args) => return run_skill(&args.command),
        #[cfg(feature = "demo")]
        RootCommand::Demo(args) => return demo::run(cli, args).await,
        _ => {}
    }

    let requires_workspace = !matches!(
        &cli.command,
        RootCommand::Workspace(crate::cli::WorkspaceArgs {
            command: WorkspaceCommand::List { .. }
        })
    );
    let requires_token = !(cli.dry_run && supports_offline_dry_run(&cli.command));
    let context = api_context(cli, requires_workspace, requires_token)?;

    match &cli.command {
        RootCommand::Workspace(args) => run_workspace(&context, &args.command).await,
        RootCommand::Story(args) => run_story(cli, &context, &args.command).await,
        RootCommand::Comment(args) => run_comment(cli, &context, &args.command).await,
        RootCommand::Attachment(args) => run_attachment(cli, &context, &args.command).await,
        RootCommand::Image(args) => run_image(cli, &context, &args.command).await,
        RootCommand::Workflow(args) => run_workflow(&context, &args.command).await,
        RootCommand::Metadata(args) => run_metadata(&context, &args.command).await,
        RootCommand::Config(_)
        | RootCommand::Auth(_)
        | RootCommand::Url(_)
        | RootCommand::Skill(_) => unreachable!(),
        #[cfg(feature = "demo")]
        RootCommand::Demo(_) => unreachable!(),
    }
}

fn run_config(cli: &Cli, command: &ConfigCommand) -> Result<CommandOutput> {
    match command {
        ConfigCommand::Path => Ok(CommandOutput::Json(json!({
            "path": discover_path(cli.config.as_ref())
        }))),
        ConfigCommand::Show => {
            let loaded = LoadedConfig::load(cli.config.as_ref())?;
            Ok(CommandOutput::Json(json!({
                "path": loaded.path,
                "config": loaded.file,
            })))
        }
        ConfigCommand::Validate => {
            let loaded = LoadedConfig::load(cli.config.as_ref())?;
            let profile = loaded.resolve_profile(
                cli.profile.as_deref(),
                None,
                cli.workspace_id.as_deref(),
                false,
            )?;
            #[cfg(feature = "demo")]
            if let Some(value) = loaded.file.organizations.get("demo") {
                let config: tapd_domain_demo::DemoConfig = value
                    .clone()
                    .try_into()
                    .context("invalid organizations.demo configuration")?;
                config
                    .request
                    .validate()
                    .context("invalid organizations.demo.request configuration")?;
            }
            let credential_status = credential::status(&profile.name, &profile.config.token_env)?;
            let token_available = credential_status.effective_source.is_some();
            Ok(CommandOutput::Json(json!({
                "valid": true,
                "path": loaded.path,
                "profile": profile.name,
                "workspace_id": profile.workspace_id,
                "token_env": profile.config.token_env,
                "token_available": token_available,
                "credential": credential_status,
            })))
        }
        ConfigCommand::Init {
            stdout,
            write,
            force,
        } => {
            if let Some(path) = write {
                let path = write_example(path.clone(), *force)?;
                Ok(CommandOutput::Json(json!({"written": path})))
            } else {
                let _ = stdout;
                Ok(CommandOutput::Plain(EXAMPLE_CONFIG.into()))
            }
        }
    }
}

fn run_url(command: &UrlCommand) -> Result<CommandOutput> {
    match command {
        UrlCommand::Parse { url } => Ok(CommandOutput::Json(serde_json::to_value(
            tapd_client::TapdUrl::parse(url)?,
        )?)),
    }
}

fn run_skill(command: &SkillCommand) -> Result<CommandOutput> {
    let output = match command {
        SkillCommand::List => skill::list(),
        SkillCommand::Read { path } => skill::read(path)?,
        SkillCommand::Install { target, force } => skill::install(target, *force)?,
        SkillCommand::Status { target } => skill::status(target)?,
        SkillCommand::Update { target, force } => skill::update(target, *force)?,
        SkillCommand::Uninstall { target } => skill::uninstall(target)?,
    };
    Ok(match output {
        SkillOutput::Json(value) => CommandOutput::Json(value),
        SkillOutput::Plain(value) => CommandOutput::Plain(value),
    })
}

async fn run_auth(cli: &Cli, command: &AuthCommand) -> Result<CommandOutput> {
    match command {
        AuthCommand::Login { from_env } => {
            let profile = selected_profile(cli, false)?;
            let environment_token = credential::token_from_environment(&profile.config.token_env);
            let environment_override_active = environment_token.is_some();
            let token = if *from_env {
                environment_token.ok_or_else(|| {
                    anyhow!(
                        "environment variable {} is not set or is empty",
                        profile.config.token_env
                    )
                })?
            } else {
                drop(environment_token);
                prompt_token()?
            };
            if token.trim().is_empty() {
                bail!("TAPD access token cannot be empty");
            }

            let client = client_for_profile(&profile, Some(token.as_str()))?;
            let _verification = client.get("/quickstart/testauth", &Vec::new()).await?;
            credential::save(&profile.name, token.as_str())?;

            Ok(CommandOutput::Json(json!({
                "profile": profile.name,
                "verified": true,
                "stored": true,
                "keychain_service": KEYCHAIN_SERVICE,
                "keychain_account": profile.name,
                "environment_override_active": environment_override_active,
            })))
        }
        AuthCommand::Status => {
            let profile = selected_profile(cli, false)?;
            let status = credential::status(&profile.name, &profile.config.token_env)?;
            Ok(CommandOutput::Json(json!({
                "profile": profile.name,
                "credential_available": status.effective_source.is_some(),
                "credential": status,
            })))
        }
        AuthCommand::Logout => {
            let profile = selected_profile(cli, false)?;
            let removed = credential::delete(&profile.name)?;
            let status = credential::status(&profile.name, &profile.config.token_env)?;
            Ok(CommandOutput::Json(json!({
                "profile": profile.name,
                "keychain_credential_removed": removed,
                "credential_available": status.effective_source.is_some(),
                "credential": status,
            })))
        }
        AuthCommand::Test => {
            let context = api_context(cli, false, true)?;
            Ok(CommandOutput::Json(
                context
                    .client
                    .get("/quickstart/testauth", &Vec::new())
                    .await?,
            ))
        }
        AuthCommand::Whoami => {
            let context = api_context(cli, false, true)?;
            Ok(CommandOutput::Json(
                context.client.get("/users/info", &Vec::new()).await?,
            ))
        }
    }
}

fn prompt_token() -> Result<Zeroizing<String>> {
    rpassword::prompt_password("TAPD access token: ")
        .map(Zeroizing::new)
        .context("failed to read TAPD access token from the terminal")
}

async fn run_workspace(context: &ApiContext, command: &WorkspaceCommand) -> Result<CommandOutput> {
    let data = match command {
        WorkspaceCommand::Get { workspace_id } => {
            let workspace_id = workspace_id
                .as_deref()
                .or(context.workspace_id.as_deref())
                .ok_or_else(|| anyhow!("workspace id is required"))?;
            context
                .client
                .get(
                    "/workspaces/get_workspace_info",
                    &vec![("workspace_id".into(), workspace_id.into())],
                )
                .await?
        }
        WorkspaceCommand::List { company_id, nick } => {
            let nick = match nick {
                Some(nick) => nick.clone(),
                None => current_nick(&context.client).await?,
            };
            context
                .client
                .get(
                    "/workspaces/user_participant_projects",
                    &vec![
                        ("company_id".into(), company_id.clone()),
                        ("nick".into(), nick),
                    ],
                )
                .await?
        }
        WorkspaceCommand::Members { user, fields } => {
            let mut params = workspace_params(context)?;
            push_opt(&mut params, "user", user.as_deref());
            push_opt(&mut params, "fields", fields.as_deref());
            context.client.get("/workspaces/users", &params).await?
        }
    };
    Ok(CommandOutput::Json(data))
}

async fn run_story(
    cli: &Cli,
    context: &ApiContext,
    command: &StoryCommand,
) -> Result<CommandOutput> {
    match command {
        StoryCommand::List(args) => {
            let params = story_list_params(context, args)?;
            Ok(CommandOutput::Json(
                context.client.get("/stories", &params).await?,
            ))
        }
        StoryCommand::Get { story_id } => {
            let mut params = workspace_params(context)?;
            params.extend([
                ("id".into(), story_id.clone()),
                ("fields".into(), "*".into()),
            ]);
            let data = context.client.get("/stories", &params).await?;
            let story = exactly_one(&data, "Story")?;
            Ok(CommandOutput::Json(story))
        }
        StoryCommand::Create(args) => {
            let prepared = prepare_story_content(cli, context, &args.content, &args.images).await?;
            let params = story_create_params(context, args, prepared.description)?;
            if cli.dry_run {
                return Ok(dry_run_with_extra(
                    "POST",
                    "/stories",
                    &params,
                    &json!({"inline_images": prepared.images}),
                ));
            }
            let data = context.client.post_form("/stories", &params).await?;
            let mut story = record(&data, "Story")?;
            if let Some(id) = tapd_client::field_string(&story, "id") {
                let url = context
                    .client
                    .story_url(workspace_id(context)?, &id)?
                    .to_string();
                story
                    .as_object_mut()
                    .expect("story record must be an object")
                    .insert("url".into(), Value::String(url));
            }
            append_inline_images(&mut story, prepared.images);
            Ok(CommandOutput::Json(story))
        }
        StoryCommand::Update(args) => {
            let prepared = prepare_story_content(cli, context, &args.content, &args.images).await?;
            let params = story_update_params(context, args, prepared.description)?;
            if params.len() <= 2 {
                bail!("story update has no changed fields");
            }
            if cli.dry_run {
                return Ok(dry_run_with_extra(
                    "POST",
                    "/stories",
                    &params,
                    &json!({"inline_images": prepared.images}),
                ));
            }
            let mut story = record(
                &context.client.post_form("/stories", &params).await?,
                "Story",
            )?;
            append_inline_images(&mut story, prepared.images);
            Ok(CommandOutput::Json(story))
        }
        StoryCommand::Transition(args) => run_story_transition(cli, context, args).await,
        StoryCommand::Count(filters) => {
            let mut params = workspace_params(context)?;
            add_story_filters(&mut params, filters)?;
            Ok(CommandOutput::Json(
                context.client.get("/stories/count", &params).await?,
            ))
        }
    }
}

async fn run_story_transition(
    cli: &Cli,
    context: &ApiContext,
    args: &StoryTransitionArgs,
) -> Result<CommandOutput> {
    let mut story_params = workspace_params(context)?;
    story_params.extend([
        ("id".into(), args.story_id.clone()),
        ("fields".into(), "*".into()),
    ]);
    let story = exactly_one(
        &context.client.get("/stories", &story_params).await?,
        "Story",
    )?;
    let current = tapd_client::field_string(&story, "status")
        .ok_or_else(|| anyhow!("story has no status"))?;
    let workitem_type_id = args
        .workitem_type_id
        .clone()
        .or_else(|| tapd_client::field_string(&story, "workitem_type_id"))
        .ok_or_else(|| anyhow!("story has no workitem_type_id; pass --workitem-type-id"))?;
    let provided_fields = parse_pairs(&args.fields)?;
    let workflow = WorkflowQueryArgs {
        system: "story".into(),
        workitem_type_id,
    };
    let workflow_params = workflow_params(context, &workflow)?;
    let status_map = context
        .client
        .get("/workflows/status_map", &workflow_params)
        .await?;
    let transitions = context
        .client
        .get("/workflows/all_transitions", &workflow_params)
        .await?;
    let plan = tapd_client::plan_transition(
        &status_map,
        &transitions,
        &current,
        &args.to,
        &story,
        &provided_fields,
    )?;
    if cli.dry_run {
        return Ok(CommandOutput::Json(json!({
            "dry_run": true,
            "story": story,
            "plan": plan,
        })));
    }

    let mut final_story = story;
    for transition in &plan.transitions {
        let mut params = workspace_params(context)?;
        params.extend([
            ("id".into(), args.story_id.clone()),
            ("status".into(), transition.next.clone()),
        ]);
        params.extend(provided_fields.clone());
        let _ = record(
            &context.client.post_form("/stories", &params).await?,
            "Story",
        )?;
        final_story = exactly_one(
            &context.client.get("/stories", &story_params).await?,
            "Story",
        )?;
        let observed = tapd_client::field_string(&final_story, "status")
            .ok_or_else(|| anyhow!("story has no status after transition"))?;
        if observed != transition.next {
            bail!(
                "workflow verification failed: expected status {:?}, got {observed:?}",
                transition.next
            );
        }
    }
    Ok(CommandOutput::Json(json!({
        "story": final_story,
        "plan": plan,
    })))
}

async fn run_comment(
    cli: &Cli,
    context: &ApiContext,
    command: &CommentCommand,
) -> Result<CommandOutput> {
    match command {
        CommentCommand::List {
            entry_type,
            entry_id,
            author,
            pagination,
        } => {
            let mut params = workspace_params(context)?;
            params.push(("entry_type".into(), entry_type.clone()));
            push_opt(&mut params, "entry_id", entry_id.as_deref());
            push_opt(&mut params, "author", author.as_deref());
            add_pagination(&mut params, pagination);
            Ok(CommandOutput::Json(
                context.client.get("/comments", &params).await?,
            ))
        }
        CommentCommand::Add {
            entry_type,
            entry_id,
            content,
            reply_id,
        } => {
            let description = read_content(content, true)
                .await?
                .expect("required content");
            let mut params = workspace_params(context)?;
            params.extend([
                ("description".into(), description),
                ("entry_type".into(), entry_type.clone()),
                ("entry_id".into(), entry_id.clone()),
            ]);
            push_opt(&mut params, "reply_id", reply_id.as_deref());
            if cli.dry_run {
                return Ok(dry_run_with_extra(
                    "POST",
                    "/comments",
                    &params,
                    &json!({"author_source": "authenticated_user"}),
                ));
            }
            params.push(("author".into(), current_nick(&context.client).await?));
            Ok(CommandOutput::Json(record(
                &context.client.post_form("/comments", &params).await?,
                "Comment",
            )?))
        }
        CommentCommand::Update {
            comment_id,
            content,
        } => {
            let description = read_content(content, true)
                .await?
                .expect("required content");
            let mut params = workspace_params(context)?;
            params.extend([
                ("id".into(), comment_id.clone()),
                ("description".into(), description),
            ]);
            if cli.dry_run {
                return Ok(dry_run_with_extra(
                    "POST",
                    "/comments",
                    &params,
                    &json!({"change_creator_source": "authenticated_user"}),
                ));
            }
            params.push((
                "change_creator".into(),
                current_nick(&context.client).await?,
            ));
            Ok(CommandOutput::Json(record(
                &context.client.post_form("/comments", &params).await?,
                "Comment",
            )?))
        }
    }
}

async fn run_attachment(
    cli: &Cli,
    context: &ApiContext,
    command: &AttachmentCommand,
) -> Result<CommandOutput> {
    match command {
        AttachmentCommand::List {
            entry_id,
            attachment_type,
            pagination,
        } => {
            let mut params = workspace_params(context)?;
            push_opt(&mut params, "entry_id", entry_id.as_deref());
            push_opt(&mut params, "type", attachment_type.as_deref());
            add_pagination(&mut params, pagination);
            Ok(CommandOutput::Json(
                context.client.get("/attachments", &params).await?,
            ))
        }
        AttachmentCommand::DownloadUrl { id } => {
            let mut params = workspace_params(context)?;
            params.push(("id".into(), id.clone()));
            Ok(CommandOutput::Json(
                context.client.get("/attachments/down", &params).await?,
            ))
        }
        AttachmentCommand::Upload {
            entry_id,
            attachment_type,
            custom_field,
            owner,
            file,
        } => {
            if attachment_type == "story_custom_field"
                && custom_field.as_deref().is_none_or(str::is_empty)
            {
                bail!("--custom-field is required for type=story_custom_field");
            }
            validate_readable_file(file).await?;
            if cli.dry_run {
                return Ok(CommandOutput::Json(json!({
                    "dry_run": true,
                    "method": "POST multipart",
                    "endpoint": "/files/upload_attachment",
                    "workspace_id": workspace_id(context)?,
                    "entry_id": entry_id,
                    "type": attachment_type,
                    "custom_field": custom_field,
                    "owner": owner,
                    "file": file,
                    "post_upload_verification": true,
                })));
            }
            let result = context
                .client
                .upload_attachment(&AttachmentUpload {
                    workspace_id: workspace_id(context)?.into(),
                    entry_id: entry_id.clone(),
                    attachment_type: attachment_type.clone(),
                    custom_field: custom_field.clone(),
                    owner: owner.clone(),
                    file: file.clone(),
                })
                .await?;
            Ok(CommandOutput::Json(serde_json::to_value(result)?))
        }
    }
}

async fn run_image(
    cli: &Cli,
    context: &ApiContext,
    command: &ImageCommand,
) -> Result<CommandOutput> {
    match command {
        ImageCommand::Upload { file } => {
            validate_inline_image_file(file).await?;
            if cli.dry_run {
                return Ok(CommandOutput::Json(json!({
                    "dry_run": true,
                    "method": "POST multipart",
                    "endpoint": "/files/upload_image",
                    "workspace_id": workspace_id(context)?,
                    "file": file,
                    "post_upload_verification": {
                        "method": "GET",
                        "endpoint": "/files/get_image",
                    },
                })));
            }
            let result = context
                .client
                .upload_image(&ImageUpload {
                    workspace_id: workspace_id(context)?.into(),
                    file: file.clone(),
                })
                .await?;
            Ok(CommandOutput::Json(json!({
                "path": result.path,
                "verified": result.verified,
                "image": result.image,
                "markdown": format!("![]({})", result.path),
                "html": format!("<img src=\"{}\" alt=\"\">", result.path),
            })))
        }
        ImageCommand::DownloadUrl { path } => {
            let data = context
                .client
                .get(
                    "/files/get_image",
                    &vec![
                        ("workspace_id".into(), workspace_id(context)?.into()),
                        ("image_path".into(), path.clone()),
                    ],
                )
                .await?;
            Ok(CommandOutput::Json(data))
        }
    }
}

async fn run_workflow(context: &ApiContext, command: &WorkflowCommand) -> Result<CommandOutput> {
    let (path, params) = match command {
        WorkflowCommand::StatusMap(args) => {
            ("/workflows/status_map", workflow_params(context, args)?)
        }
        WorkflowCommand::Transitions(args) => (
            "/workflows/all_transitions",
            workflow_params(context, args)?,
        ),
        WorkflowCommand::FirstStep(args) => {
            ("/workflows/first_step", workflow_params(context, args)?)
        }
        WorkflowCommand::LastSteps(args) => {
            ("/workflows/last_steps", workflow_params(context, args)?)
        }
        WorkflowCommand::List { system } => {
            let mut params = workspace_params(context)?;
            params.push(("system_name".into(), system.clone()));
            ("/workflows", params)
        }
    };
    Ok(CommandOutput::Json(
        context.client.get(path, &params).await?,
    ))
}

async fn run_metadata(context: &ApiContext, command: &MetadataCommand) -> Result<CommandOutput> {
    match command {
        MetadataCommand::WorkitemTypes { name } => {
            let mut params = workspace_params(context)?;
            push_opt(&mut params, "name", name.as_deref());
            params.push(("limit".into(), "200".into()));
            Ok(CommandOutput::Json(
                context.client.get("/workitem_types", &params).await?,
            ))
        }
        MetadataCommand::CustomFields { entity_type } => {
            if !matches!(
                entity_type.as_str(),
                "stories" | "tasks" | "iterations" | "tcases"
            ) {
                bail!("entity-type must be stories, tasks, iterations, or tcases");
            }
            Ok(CommandOutput::Json(
                context
                    .client
                    .get(
                        &format!("/{entity_type}/custom_fields_settings"),
                        &workspace_params(context)?,
                    )
                    .await?,
            ))
        }
        MetadataCommand::Categories => Ok(CommandOutput::Json(
            context
                .client
                .get("/story_categories", &workspace_params(context)?)
                .await?,
        )),
        MetadataCommand::Templates { workitem_type_id } => {
            let mut params = workspace_params(context)?;
            push_opt(&mut params, "workitem_type_id", workitem_type_id.as_deref());
            Ok(CommandOutput::Json(
                context
                    .client
                    .get("/stories/template_list", &params)
                    .await?,
            ))
        }
        MetadataCommand::StoryFields => {
            let params = workspace_params(context)?;
            let labels = context
                .client
                .get("/stories/get_fields_lable", &params)
                .await?;
            let fields = context
                .client
                .get("/stories/get_fields_info", &params)
                .await?;
            Ok(CommandOutput::Json(
                json!({"labels": labels, "fields": fields}),
            ))
        }
    }
}

fn api_context(cli: &Cli, require_workspace: bool, require_token: bool) -> Result<ApiContext> {
    let profile = selected_profile(cli, require_token)?;
    if require_workspace && profile.workspace_id.as_deref().is_none_or(str::is_empty) {
        bail!("workspace id is required; configure it in the profile or pass --workspace-id");
    }
    let client = client_for_profile(
        &profile,
        profile.access_token.as_ref().map(|token| token.as_str()),
    )?;
    Ok(ApiContext {
        client,
        workspace_id: profile.workspace_id.clone(),
    })
}

fn selected_profile(cli: &Cli, require_token: bool) -> Result<ResolvedProfile> {
    LoadedConfig::load(cli.config.as_ref())?.resolve_profile(
        cli.profile.as_deref(),
        None,
        cli.workspace_id.as_deref(),
        require_token,
    )
}

fn client_for_profile(profile: &ResolvedProfile, token: Option<&str>) -> Result<TapdClient> {
    let mut builder = TapdClient::builder()
        .api_base_url(profile.config.api_base_url.clone())
        .web_base_url(profile.config.web_base_url.clone())
        .timeout(Duration::from_secs(profile.config.timeout_seconds));
    if let Some(token) = token {
        builder = builder.access_token(token.to_owned());
    }
    Ok(builder.build()?)
}

fn supports_offline_dry_run(command: &RootCommand) -> bool {
    match command {
        RootCommand::Story(args) => matches!(
            args.command,
            StoryCommand::Create(_) | StoryCommand::Update(_)
        ),
        RootCommand::Comment(args) => matches!(
            args.command,
            CommentCommand::Add { .. } | CommentCommand::Update { .. }
        ),
        RootCommand::Attachment(args) => {
            matches!(args.command, AttachmentCommand::Upload { .. })
        }
        RootCommand::Image(args) => matches!(args.command, ImageCommand::Upload { .. }),
        _ => false,
    }
}

fn workspace_params(context: &ApiContext) -> Result<Params> {
    Ok(vec![("workspace_id".into(), workspace_id(context)?.into())])
}

fn workspace_id(context: &ApiContext) -> Result<&str> {
    context
        .workspace_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("workspace id is required"))
}

fn story_list_params(context: &ApiContext, args: &StoryListArgs) -> Result<Params> {
    let mut params = workspace_params(context)?;
    add_story_filters(&mut params, &args.filters)?;
    add_pagination(&mut params, &args.pagination);
    params.push(("fields".into(), args.fields.clone()));
    Ok(params)
}

#[derive(Debug)]
struct PreparedStoryContent {
    description: Option<String>,
    images: Vec<Value>,
}

async fn prepare_story_content(
    cli: &Cli,
    context: &ApiContext,
    description_args: &DescriptionArgs,
    image_values: &[String],
) -> Result<PreparedStoryContent> {
    let mut raw = read_raw_content(description_args, false).await?;
    let specs = parse_inline_image_specs(image_values)?;
    validate_inline_image_placeholders(
        raw.as_deref(),
        description_args.description_format,
        &specs,
    )?;
    for spec in &specs {
        validate_inline_image_file(&spec.file).await?;
    }

    let mut images = Vec::with_capacity(specs.len());
    if let Some(raw) = &mut raw {
        for spec in specs {
            if cli.dry_run {
                let planned_path = format!("tapd-image-pending://{}", spec.name);
                replace_inline_image_placeholder(raw, &spec.name, &planned_path);
                images.push(json!({
                    "name": spec.name,
                    "file": spec.file,
                    "upload": {
                        "method": "POST multipart",
                        "endpoint": "/files/upload_image",
                        "workspace_id": workspace_id(context)?,
                    },
                    "read_back": {
                        "method": "GET",
                        "endpoint": "/files/get_image",
                    },
                }));
                continue;
            }

            let uploaded = context
                .client
                .upload_image(&ImageUpload {
                    workspace_id: workspace_id(context)?.into(),
                    file: spec.file.clone(),
                })
                .await?;
            replace_inline_image_placeholder(raw, &spec.name, &uploaded.path);
            images.push(json!({
                "name": spec.name,
                "file": spec.file,
                "path": uploaded.path,
                "verified": uploaded.verified,
                "image": uploaded.image,
            }));
        }
    }

    Ok(PreparedStoryContent {
        description: raw.map(|value| format_content(&value, description_args.description_format)),
        images,
    })
}

fn append_inline_images(story: &mut Value, images: Vec<Value>) {
    if images.is_empty() {
        return;
    }
    story
        .as_object_mut()
        .expect("story record must be an object")
        .insert("inline_images".into(), Value::Array(images));
}

fn add_story_filters(params: &mut Params, filters: &StoryFilterArgs) -> Result<()> {
    push_opt(params, "name", filters.name.as_deref());
    push_opt(params, "status", filters.status.as_deref());
    push_opt(params, "v_status", filters.v_status.as_deref());
    push_opt(params, "owner", filters.owner.as_deref());
    push_opt(params, "category_id", filters.category_id.as_deref());
    push_opt(
        params,
        "workitem_type_id",
        filters.workitem_type_id.as_deref(),
    );
    push_opt(params, "label", filters.label.as_deref());
    params.extend(parse_pairs(&filters.filters)?);
    Ok(())
}

fn story_create_params(
    context: &ApiContext,
    args: &StoryCreateArgs,
    description: Option<String>,
) -> Result<Params> {
    let mut params = workspace_params(context)?;
    params.push(("name".into(), args.name.clone()));
    if let Some(description) = description {
        params.push(("description".into(), description));
    }
    push_opt(&mut params, "owner", args.owner.as_deref());
    push_joined(&mut params, "cc", &args.cc);
    push_opt(&mut params, "developer", args.developer.as_deref());
    push_opt(&mut params, "priority_label", args.priority.as_deref());
    push_opt(&mut params, "iteration_id", args.iteration_id.as_deref());
    push_opt(&mut params, "parent_id", args.parent_id.as_deref());
    push_opt(&mut params, "category_id", args.category_id.as_deref());
    push_opt(
        &mut params,
        "workitem_type_id",
        args.workitem_type_id.as_deref(),
    );
    push_opt(&mut params, "label", args.label.as_deref());
    push_opt(&mut params, "begin", args.begin.as_deref());
    push_opt(&mut params, "due", args.due.as_deref());
    if let Some(template_id) = &args.template_id {
        params.extend([
            ("templated_id".into(), template_id.clone()),
            ("apply_template".into(), "1".into()),
            ("is_apply_template_default_value".into(), "1".into()),
        ]);
    }
    params.extend(parse_pairs(&args.custom_fields)?);
    Ok(params)
}

fn story_update_params(
    context: &ApiContext,
    args: &StoryUpdateArgs,
    description: Option<String>,
) -> Result<Params> {
    let mut params = workspace_params(context)?;
    params.push(("id".into(), args.story_id.clone()));
    push_opt(&mut params, "name", args.name.as_deref());
    if let Some(description) = description {
        params.push(("description".into(), description));
    }
    push_opt(&mut params, "status", args.status.as_deref());
    push_opt(&mut params, "v_status", args.v_status.as_deref());
    push_opt(&mut params, "owner", args.owner.as_deref());
    push_joined(&mut params, "cc", &args.cc);
    push_opt(&mut params, "developer", args.developer.as_deref());
    push_opt(&mut params, "priority_label", args.priority.as_deref());
    push_opt(&mut params, "iteration_id", args.iteration_id.as_deref());
    push_opt(&mut params, "category_id", args.category_id.as_deref());
    push_opt(&mut params, "label", args.label.as_deref());
    push_opt(&mut params, "begin", args.begin.as_deref());
    push_opt(&mut params, "due", args.due.as_deref());
    params.extend(parse_pairs(&args.custom_fields)?);
    Ok(params)
}

fn workflow_params(context: &ApiContext, args: &WorkflowQueryArgs) -> Result<Params> {
    let mut params = workspace_params(context)?;
    params.extend([
        ("system".into(), args.system.clone()),
        ("workitem_type_id".into(), args.workitem_type_id.clone()),
    ]);
    Ok(params)
}

fn add_pagination(params: &mut Params, pagination: &PaginationArgs) {
    params.extend([
        ("limit".into(), pagination.limit.to_string()),
        ("page".into(), pagination.page.to_string()),
    ]);
    push_opt(params, "order", pagination.order.as_deref());
}

fn push_opt(params: &mut Params, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        params.push((key.into(), value.into()));
    }
}

fn push_joined(params: &mut Params, key: &str, values: &[String]) {
    if !values.is_empty() {
        params.push((key.into(), values.join(";")));
    }
}

pub(super) fn parse_pairs(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut output = BTreeMap::new();
    for value in values {
        let Some((key, value)) = value.split_once('=') else {
            bail!("expected FIELD=VALUE, got {value:?}");
        };
        if key.is_empty() {
            bail!("field name must not be empty in {value:?}");
        }
        output.insert(key.into(), value.into());
    }
    Ok(output)
}

async fn current_nick(client: &TapdClient) -> Result<String> {
    let data = client.get("/users/info", &Vec::new()).await?;
    tapd_client::field_string(&data, "nick")
        .or_else(|| tapd_client::field_string(&data, "name"))
        .ok_or_else(|| anyhow!("authenticated TAPD user has no nick"))
}

fn exactly_one(data: &Value, wrapper: &str) -> Result<Value> {
    let values = records(data, wrapper)?;
    match values.as_slice() {
        [value] => Ok(value.clone()),
        [] => bail!("{wrapper} was not found"),
        _ => bail!("expected one {wrapper}, got {}", values.len()),
    }
}

fn dry_run_with_extra(
    method: &str,
    endpoint: &str,
    params: &Params,
    extra: &Value,
) -> CommandOutput {
    CommandOutput::Json(json!({
        "dry_run": true,
        "method": method,
        "endpoint": endpoint,
        "params": params,
        "extra": extra,
    }))
}
