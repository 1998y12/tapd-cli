//! A small, organization-neutral example of composing `tapd-client` into a
//! domain workflow.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tapd_client::{Params, TapdClient, record};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DemoError {
    #[error("invalid demo request configuration: {0}")]
    Config(String),
    #[error("invalid demo request input: {0}")]
    Input(String),
    #[error(transparent)]
    Client(#[from] tapd_client::Error),
}

pub type Result<T> = std::result::Result<T, DemoError>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DemoConfig {
    pub request: DemoRequestConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DemoRequestConfig {
    pub profile: Option<String>,
    pub workspace_id: Option<String>,
    pub workitem_type_id: Option<String>,
    pub category_id: Option<String>,
    pub owner: Option<String>,
    pub title_template: String,
}

impl Default for DemoRequestConfig {
    fn default() -> Self {
        Self {
            profile: None,
            workspace_id: None,
            workitem_type_id: None,
            category_id: None,
            owner: None,
            title_template: "[{subject}] {summary}".into(),
        }
    }
}

impl DemoRequestConfig {
    /// Validate the example's fixed domain configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the title template is empty or does not contain
    /// the required `{subject}` and `{summary}` placeholders.
    pub fn validate(&self) -> Result<()> {
        if self.title_template.trim().is_empty() {
            return Err(DemoError::Config("title_template must not be empty".into()));
        }
        for placeholder in ["{subject}", "{summary}"] {
            if !self.title_template.contains(placeholder) {
                return Err(DemoError::Config(format!(
                    "title_template must contain {placeholder}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DemoRequestInput {
    pub subject: String,
    pub summary: String,
    pub description: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequestPlan {
    pub organization: String,
    pub domain: String,
    pub workspace_id: String,
    pub method: String,
    pub endpoint: String,
    pub params: Params,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequestResult {
    pub plan: DemoRequestPlan,
    pub story: Value,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct DemoRequestService {
    client: TapdClient,
    workspace_id: String,
    config: DemoRequestConfig,
}

impl DemoRequestService {
    /// Construct the example domain service.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace or demo configuration is invalid.
    pub fn new(
        client: TapdClient,
        workspace_id: impl Into<String>,
        config: DemoRequestConfig,
    ) -> Result<Self> {
        config.validate()?;
        let workspace_id = workspace_id.into();
        if workspace_id.trim().is_empty() {
            return Err(DemoError::Config("workspace_id must not be empty".into()));
        }
        Ok(Self {
            client,
            workspace_id,
            config,
        })
    }

    /// Resolve a domain request into generic TAPD story parameters.
    ///
    /// # Errors
    ///
    /// Returns an error when required business input is empty.
    pub fn plan(&self, input: &DemoRequestInput) -> Result<DemoRequestPlan> {
        let subject = input.subject.trim();
        let summary = input.summary.trim();
        if subject.is_empty() || summary.is_empty() {
            return Err(DemoError::Input(
                "subject and summary must both be non-empty".into(),
            ));
        }

        let title = self
            .config
            .title_template
            .replace("{subject}", subject)
            .replace("{summary}", summary);
        let description = input
            .description
            .clone()
            .unwrap_or_else(|| format!("<p>{}</p>", escape_html(summary)));
        let mut params = vec![
            ("workspace_id".into(), self.workspace_id.clone()),
            ("name".into(), title),
            ("description".into(), description),
        ];
        push_optional(
            &mut params,
            "workitem_type_id",
            self.config.workitem_type_id.as_deref(),
        );
        push_optional(
            &mut params,
            "category_id",
            self.config.category_id.as_deref(),
        );
        push_optional(
            &mut params,
            "owner",
            input.owner.as_deref().or(self.config.owner.as_deref()),
        );

        Ok(DemoRequestPlan {
            organization: "demo".into(),
            domain: "request".into(),
            workspace_id: self.workspace_id.clone(),
            method: "POST".into(),
            endpoint: "/stories".into(),
            params,
        })
    }

    /// Submit the planned example request as a TAPD story.
    ///
    /// # Errors
    ///
    /// Returns an error when planning or the TAPD request fails.
    pub async fn submit(&self, input: &DemoRequestInput) -> Result<DemoRequestResult> {
        let plan = self.plan(input)?;
        let story = record(
            &self.client.post_form(&plan.endpoint, &plan.params).await?,
            "Story",
        )?;
        let id = tapd_client::field_string(&story, "id")
            .ok_or_else(|| DemoError::Input("created story has no id".into()))?;
        let url = self.client.story_url(&self.workspace_id, &id)?.to_string();
        Ok(DemoRequestResult { plan, story, url })
    }
}

fn push_optional(params: &mut Params, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        params.push((name.into(), value.into()));
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_combines_fixed_configuration_and_business_input() {
        let service = DemoRequestService::new(
            TapdClient::builder().build().expect("build client"),
            "123456",
            DemoRequestConfig {
                workitem_type_id: Some("42".into()),
                category_id: Some("7".into()),
                owner: Some("configured-owner".into()),
                ..DemoRequestConfig::default()
            },
        )
        .expect("build service");
        let plan = service
            .plan(&DemoRequestInput {
                subject: "Reporting".into(),
                summary: "Add a monthly view".into(),
                description: None,
                owner: None,
            })
            .expect("plan request");

        assert_eq!(plan.organization, "demo");
        assert_eq!(plan.domain, "request");
        assert!(
            plan.params
                .contains(&("workitem_type_id".into(), "42".into()))
        );
        assert!(plan.params.contains(&("category_id".into(), "7".into())));
        assert!(
            plan.params
                .contains(&("owner".into(), "configured-owner".into()))
        );
        assert!(
            plan.params
                .contains(&("name".into(), "[Reporting] Add a monthly view".into()))
        );
    }
}
