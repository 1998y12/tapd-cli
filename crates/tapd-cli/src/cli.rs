use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "tapd-cli",
    version,
    about = "TAPD Open API command-line client",
    long_about = "A stable TAPD Open API command-line client with optional organization-specific domain workflows."
)]
pub struct Cli {
    /// Configuration file. Defaults to ./.tapd-cli.toml or the user config directory.
    #[arg(long, env = "TAPD_CLI_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Profile name from the configuration file.
    #[arg(long, env = "TAPD_PROFILE", global = true)]
    pub profile: Option<String>,

    /// Workspace ID override.
    #[arg(long, env = "TAPD_WORKSPACE_ID", global = true)]
    pub workspace_id: Option<String>,

    /// Output representation.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json, global = true)]
    pub output: OutputFormat,

    /// Resolve and validate a write without sending it to TAPD.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: RootCommand,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Json,
    JsonPretty,
}

impl OutputFormat {
    pub const fn is_pretty(self) -> bool {
        matches!(self, Self::JsonPretty)
    }
}

#[derive(Debug, Subcommand)]
pub enum RootCommand {
    /// Configuration discovery, generation, and validation.
    Config(ConfigArgs),
    /// Authentication checks.
    Auth(AuthArgs),
    /// Workspace and member queries.
    Workspace(WorkspaceArgs),
    /// Story operations.
    Story(Box<StoryArgs>),
    /// Comment operations.
    Comment(CommentArgs),
    /// Attachment operations.
    Attachment(AttachmentArgs),
    /// Rich-text image operations.
    Image(ImageArgs),
    /// Workflow discovery.
    Workflow(WorkflowArgs),
    /// Story metadata discovery.
    Metadata(MetadataArgs),
    /// TAPD web URL parsing.
    Url(UrlArgs),
    /// Install and inspect the bundled Agent Skill.
    Skill(SkillArgs),
    /// Organization-domain extension example.
    #[cfg(feature = "demo")]
    Demo(DemoArgs),
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Show the selected config path.
    Path,
    /// Show the parsed config without any secret values.
    Show,
    /// Validate profile and domain configuration.
    Validate,
    /// Print or write an example configuration.
    Init {
        /// Print the TOML example to stdout.
        #[arg(long, conflicts_with = "write")]
        stdout: bool,
        /// Write the example to this path.
        #[arg(long, value_name = "PATH")]
        write: Option<PathBuf>,
        /// Replace the one selected config file if it already exists.
        #[arg(long, requires = "write")]
        force: bool,
    },
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Verify and save a token in the system credential store.
    Login {
        /// Read the token from the profile's `token_env` instead of prompting.
        #[arg(long)]
        from_env: bool,
    },
    /// Show local credential availability without contacting TAPD.
    Status,
    /// Remove this profile's token from the system credential store.
    Logout,
    /// Verify the configured token.
    Test,
    /// Query the authenticated TAPD user.
    Whoami,
}

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Get workspace details.
    Get { workspace_id: Option<String> },
    /// List workspaces for a company and user.
    List {
        #[arg(long)]
        company_id: String,
        #[arg(long)]
        nick: Option<String>,
    },
    /// List workspace members.
    Members {
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        fields: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct StoryArgs {
    #[command(subcommand)]
    pub command: StoryCommand,
}

#[derive(Debug, Subcommand)]
pub enum StoryCommand {
    /// Query stories.
    List(StoryListArgs),
    /// Get one story.
    Get { story_id: String },
    /// Create one story.
    Create(StoryCreateArgs),
    /// Update one story.
    Update(StoryUpdateArgs),
    /// Plan and execute a legal workflow path to a target status.
    Transition(StoryTransitionArgs),
    /// Count stories using the same filters as list.
    Count(StoryFilterArgs),
}

#[derive(Debug, Clone, Args, Default)]
pub struct PaginationArgs {
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u16).range(1..=200))]
    pub limit: u16,
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub page: u32,
    #[arg(long)]
    pub order: Option<String>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct StoryFilterArgs {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub v_status: Option<String>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long)]
    pub category_id: Option<String>,
    #[arg(long)]
    pub workitem_type_id: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    /// Raw TAPD filter as field=value. May be repeated.
    #[arg(long = "filter", value_name = "FIELD=VALUE")]
    pub filters: Vec<String>,
}

#[derive(Debug, Args)]
pub struct StoryListArgs {
    #[command(flatten)]
    pub filters: StoryFilterArgs,
    #[command(flatten)]
    pub pagination: PaginationArgs,
    #[arg(long, default_value = "id,name,status,owner,modified,workitem_type_id")]
    pub fields: String,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum DescriptionFormat {
    Plain,
    #[default]
    Markdown,
    Html,
}

#[derive(Debug, Clone, Args, Default)]
pub struct DescriptionArgs {
    #[arg(long, conflicts_with = "description_file")]
    pub description: Option<String>,
    #[arg(long, conflicts_with = "description")]
    pub description_file: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = DescriptionFormat::Markdown)]
    pub description_format: DescriptionFormat,
}

#[derive(Debug, Args)]
pub struct StoryCreateArgs {
    #[arg(long)]
    pub name: String,
    #[command(flatten)]
    pub content: DescriptionArgs,
    /// Map a body placeholder to a local image. May be repeated.
    #[arg(long = "image", value_name = "NAME=PATH")]
    pub images: Vec<String>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long = "cc")]
    pub cc: Vec<String>,
    #[arg(long)]
    pub developer: Option<String>,
    #[arg(long)]
    pub priority: Option<String>,
    #[arg(long)]
    pub iteration_id: Option<String>,
    #[arg(long)]
    pub parent_id: Option<String>,
    #[arg(long)]
    pub category_id: Option<String>,
    #[arg(long)]
    pub workitem_type_id: Option<String>,
    #[arg(long)]
    pub template_id: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub begin: Option<String>,
    #[arg(long)]
    pub due: Option<String>,
    #[arg(long = "custom-field", value_name = "FIELD=VALUE")]
    pub custom_fields: Vec<String>,
}

#[derive(Debug, Args)]
pub struct StoryUpdateArgs {
    pub story_id: String,
    #[arg(long)]
    pub name: Option<String>,
    #[command(flatten)]
    pub content: DescriptionArgs,
    /// Map a body placeholder to a local image. May be repeated.
    #[arg(long = "image", value_name = "NAME=PATH")]
    pub images: Vec<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub v_status: Option<String>,
    #[arg(long)]
    pub owner: Option<String>,
    #[arg(long = "cc")]
    pub cc: Vec<String>,
    #[arg(long)]
    pub developer: Option<String>,
    #[arg(long)]
    pub priority: Option<String>,
    #[arg(long)]
    pub iteration_id: Option<String>,
    #[arg(long)]
    pub category_id: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub begin: Option<String>,
    #[arg(long)]
    pub due: Option<String>,
    #[arg(long = "custom-field", value_name = "FIELD=VALUE")]
    pub custom_fields: Vec<String>,
}

#[derive(Debug, Args)]
pub struct StoryTransitionArgs {
    pub story_id: String,
    /// Target status code or display label.
    #[arg(long)]
    pub to: String,
    /// Override the story's work item type for workflow discovery.
    #[arg(long)]
    pub workitem_type_id: Option<String>,
    /// Values available to required workflow append fields.
    #[arg(long = "field", value_name = "FIELD=VALUE")]
    pub fields: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CommentArgs {
    #[command(subcommand)]
    pub command: CommentCommand,
}

#[derive(Debug, Subcommand)]
pub enum CommentCommand {
    List {
        #[arg(long, default_value = "stories")]
        entry_type: String,
        #[arg(long)]
        entry_id: Option<String>,
        #[arg(long)]
        author: Option<String>,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    Add {
        #[arg(long, default_value = "stories")]
        entry_type: String,
        #[arg(long)]
        entry_id: String,
        #[command(flatten)]
        content: DescriptionArgs,
        #[arg(long)]
        reply_id: Option<String>,
    },
    Update {
        comment_id: String,
        #[command(flatten)]
        content: DescriptionArgs,
    },
}

#[derive(Debug, Args)]
pub struct AttachmentArgs {
    #[command(subcommand)]
    pub command: AttachmentCommand,
}

#[derive(Debug, Subcommand)]
pub enum AttachmentCommand {
    List {
        #[arg(long)]
        entry_id: Option<String>,
        #[arg(long = "type")]
        attachment_type: Option<String>,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    Upload {
        #[arg(long)]
        entry_id: String,
        #[arg(long = "type", default_value = "story")]
        attachment_type: String,
        #[arg(long)]
        custom_field: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        file: PathBuf,
    },
    DownloadUrl {
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Args)]
pub struct ImageArgs {
    #[command(subcommand)]
    pub command: ImageCommand,
}

#[derive(Debug, Subcommand)]
pub enum ImageCommand {
    /// Upload an image for embedding in rich-text content.
    Upload {
        #[arg(long)]
        file: PathBuf,
    },
    /// Get a temporary download URL for a TAPD image path.
    DownloadUrl {
        #[arg(long)]
        path: String,
    },
}

#[derive(Debug, Args)]
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub command: WorkflowCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    StatusMap(WorkflowQueryArgs),
    Transitions(WorkflowQueryArgs),
    FirstStep(WorkflowQueryArgs),
    LastSteps(WorkflowQueryArgs),
    List {
        #[arg(long, default_value = "story")]
        system: String,
    },
}

#[derive(Debug, Args)]
pub struct WorkflowQueryArgs {
    #[arg(long, default_value = "story")]
    pub system: String,
    #[arg(long)]
    pub workitem_type_id: String,
}

#[derive(Debug, Args)]
pub struct MetadataArgs {
    #[command(subcommand)]
    pub command: MetadataCommand,
}

#[derive(Debug, Subcommand)]
pub enum MetadataCommand {
    WorkitemTypes {
        #[arg(long)]
        name: Option<String>,
    },
    CustomFields {
        #[arg(long, default_value = "stories")]
        entity_type: String,
    },
    Categories,
    Templates {
        #[arg(long)]
        workitem_type_id: Option<String>,
    },
    StoryFields,
}

#[derive(Debug, Args)]
pub struct UrlArgs {
    #[command(subcommand)]
    pub command: UrlCommand,
}

#[derive(Debug, Subcommand)]
pub enum UrlCommand {
    Parse { url: String },
}

#[derive(Debug, Args)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommand,
}

#[derive(Debug, Subcommand)]
pub enum SkillCommand {
    /// List files embedded in the bundled Skill.
    List,
    /// Read one embedded Skill file.
    Read {
        #[arg(default_value = "SKILL.md")]
        path: String,
    },
    /// Install the bundled Skill for one or more agents.
    Install {
        #[command(flatten)]
        target: SkillTargetArgs,
        /// Overwrite known files in an existing destination.
        #[arg(long)]
        force: bool,
    },
    /// Show bundled and installed Skill versions.
    Status {
        #[command(flatten)]
        target: SkillTargetArgs,
    },
    /// Replace a managed installation with this binary's bundled Skill.
    Update {
        #[command(flatten)]
        target: SkillTargetArgs,
        /// Replace locally modified or incomplete managed files.
        #[arg(long)]
        force: bool,
    },
    /// Remove files created by a managed Skill installation.
    Uninstall {
        #[command(flatten)]
        target: SkillTargetArgs,
    },
}

#[derive(Debug, Clone, Args)]
pub struct SkillTargetArgs {
    /// Agent adapter, or auto-detect installed agents.
    #[arg(long, value_enum, default_value_t = SkillAgent::Auto)]
    pub agent: SkillAgent,
    /// Install for the current user or the current Git project.
    #[arg(long, value_enum, default_value_t = SkillScope::User)]
    pub scope: SkillScope,
    /// Custom skills root. The tapd-cli directory is created below it.
    #[arg(long, value_name = "SKILLS_DIR")]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SkillAgent {
    Auto,
    Codex,
    ClaudeCode,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SkillScope {
    User,
    Project,
}

#[cfg(feature = "demo")]
#[derive(Debug, Args)]
pub struct DemoArgs {
    #[command(subcommand)]
    pub command: DemoCommand,
}

#[cfg(feature = "demo")]
#[derive(Debug, Subcommand)]
pub enum DemoCommand {
    /// Example of a configurable organization request workflow.
    Request(DemoRequestArgs),
}

#[cfg(feature = "demo")]
#[derive(Debug, Args)]
pub struct DemoRequestArgs {
    #[command(subcommand)]
    pub command: DemoRequestCommand,
}

#[cfg(feature = "demo")]
#[derive(Debug, Subcommand)]
pub enum DemoRequestCommand {
    /// Resolve the example domain request without calling TAPD.
    Plan(DemoRequestInputArgs),
    /// Create a story through the example domain service.
    Submit(DemoRequestInputArgs),
}

#[cfg(feature = "demo")]
#[derive(Debug, Args)]
pub struct DemoRequestInputArgs {
    /// Business subject used by the configured title template.
    #[arg(long)]
    pub subject: String,
    /// Concise business summary used by the configured title template.
    #[arg(long)]
    pub summary: String,
    #[command(flatten)]
    pub content: DescriptionArgs,
    #[arg(long)]
    pub owner: Option<String>,
}
