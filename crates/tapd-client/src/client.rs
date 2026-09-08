use std::{fmt, path::PathBuf, time::Duration};

use reqwest::{
    Client,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
    multipart::{Form, Part},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{Error, Params, Result, field_string, record, records};

const DEFAULT_API_BASE_URL: &str = "https://api.tapd.cn";
const DEFAULT_WEB_BASE_URL: &str = "https://www.tapd.cn";
const MAX_ERROR_BODY_CHARS: usize = 512;

#[derive(Clone)]
pub struct TapdClient {
    http: Client,
    api_base_url: Url,
    web_base_url: Url,
}

impl fmt::Debug for TapdClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TapdClient")
            .field("api_base_url", &self.api_base_url)
            .field("web_base_url", &self.web_base_url)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct TapdClientBuilder {
    api_base_url: String,
    web_base_url: String,
    access_token: Option<String>,
    timeout: Duration,
}

impl Default for TapdClientBuilder {
    fn default() -> Self {
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_owned(),
            web_base_url: DEFAULT_WEB_BASE_URL.to_owned(),
            access_token: None,
            timeout: Duration::from_secs(30),
        }
    }
}

impl TapdClientBuilder {
    #[must_use]
    pub fn api_base_url(mut self, value: impl Into<String>) -> Self {
        self.api_base_url = value.into();
        self
    }

    #[must_use]
    pub fn web_base_url(mut self, value: impl Into<String>) -> Self {
        self.web_base_url = value.into();
        self
    }

    #[must_use]
    pub fn access_token(mut self, value: impl Into<String>) -> Self {
        self.access_token = Some(value.into());
        self
    }

    #[must_use]
    pub const fn timeout(mut self, value: Duration) -> Self {
        self.timeout = value;
        self
    }

    /// Build a configured client.
    ///
    /// # Errors
    ///
    /// Returns an error when a base URL, authorization header, or the
    /// underlying HTTP client configuration is invalid.
    pub fn build(self) -> Result<TapdClient> {
        let api_base_url = normalized_base_url(&self.api_base_url)?;
        let web_base_url = normalized_base_url(&self.web_base_url)?;

        let mut headers = HeaderMap::new();
        if let Some(token) = self.access_token.filter(|value| !value.is_empty()) {
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| Error::InvalidAuthorizationHeader)?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }

        let http = Client::builder()
            .default_headers(headers)
            .timeout(self.timeout)
            .build()
            .map_err(Error::ClientBuild)?;

        Ok(TapdClient {
            http,
            api_base_url,
            web_base_url,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentUpload {
    pub workspace_id: String,
    pub entry_id: String,
    pub attachment_type: String,
    pub custom_field: Option<String>,
    pub owner: Option<String>,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedAttachmentUpload {
    pub attachment: Value,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct ImageUpload {
    pub workspace_id: String,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedImageUpload {
    pub image: Value,
    pub path: String,
    pub verified: bool,
}

impl TapdClient {
    pub fn builder() -> TapdClientBuilder {
        TapdClientBuilder::default()
    }

    /// Send a GET request and return the TAPD envelope's `data` value.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid URLs, transport failures, non-successful
    /// HTTP responses, malformed envelopes, or TAPD API errors.
    pub async fn get(&self, path: &str, params: &Params) -> Result<Value> {
        let url = self.endpoint(path)?;
        let response = self
            .http
            .get(url)
            .query(params)
            .send()
            .await
            .map_err(Error::Transport)?;
        decode_response(response).await
    }

    /// Send a form-encoded POST and return the TAPD envelope's `data` value.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid URLs, transport failures, non-successful
    /// HTTP responses, malformed envelopes, or TAPD API errors.
    pub async fn post_form(&self, path: &str, params: &Params) -> Result<Value> {
        let url = self.endpoint(path)?;
        let response = self
            .http
            .post(url)
            .form(params)
            .send()
            .await
            .map_err(Error::Transport)?;
        decode_response(response).await
    }

    /// Upload an attachment and verify it with an immediate list read-back.
    ///
    /// # Errors
    ///
    /// Returns an error when the local file cannot be read, the upload or
    /// verification request fails, or the uploaded attachment cannot be
    /// matched by ID, entry, type, and filename.
    pub async fn upload_attachment(
        &self,
        upload: &AttachmentUpload,
    ) -> Result<VerifiedAttachmentUpload> {
        let bytes = tokio::fs::read(&upload.file)
            .await
            .map_err(|source| Error::File {
                path: upload.file.display().to_string(),
                source,
            })?;
        let filename = upload
            .file
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::AttachmentVerification("invalid attachment filename".into()))?
            .to_owned();
        let content_type = mime_guess::from_path(&upload.file).first_or_octet_stream();
        let part = Part::bytes(bytes)
            .file_name(filename.clone())
            .mime_str(content_type.as_ref())
            .map_err(|error| Error::Protocol {
                message: format!("invalid attachment MIME type: {error}"),
            })?;

        let mut form = Form::new()
            .text("workspace_id", upload.workspace_id.clone())
            .text("type", upload.attachment_type.clone())
            .text("entry_id", upload.entry_id.clone())
            .part("file", part);
        if let Some(custom_field) = &upload.custom_field {
            form = form.text("custom_field", custom_field.clone());
        }
        if let Some(owner) = &upload.owner {
            form = form.text("owner", owner.clone());
        }

        let response = self
            .http
            .post(self.endpoint("/files/upload_attachment")?)
            .multipart(form)
            .send()
            .await
            .map_err(Error::Transport)?;
        let data = decode_response(response).await?;
        let attachment = record(&data, "Attachment")?;
        let attachment_id = field_string(&attachment, "id").ok_or_else(|| {
            Error::AttachmentVerification("upload response has no attachment id".into())
        })?;

        let listed = self
            .get(
                "/attachments",
                &vec![
                    ("workspace_id".into(), upload.workspace_id.clone()),
                    ("entry_id".into(), upload.entry_id.clone()),
                    ("limit".into(), "200".into()),
                ],
            )
            .await?;
        let verified_attachment = records(&listed, "Attachment")?
            .into_iter()
            .find(|candidate| {
                field_string(candidate, "id").as_deref() == Some(attachment_id.as_str())
                    && field_string(candidate, "entry_id").as_deref()
                        == Some(upload.entry_id.as_str())
                    && field_string(candidate, "type").as_deref()
                        == Some(upload.attachment_type.as_str())
                    && field_string(candidate, "filename").as_deref() == Some(filename.as_str())
            })
            .ok_or_else(|| {
                Error::AttachmentVerification(format!(
                    "attachment {attachment_id} was not present in the post-upload list"
                ))
            })?;

        Ok(VerifiedAttachmentUpload {
            attachment: verified_attachment,
            verified: true,
        })
    }

    /// Upload an image for rich-text embedding and verify its returned path.
    ///
    /// # Errors
    ///
    /// Returns an error when the local file cannot be read, its format is not
    /// supported by TAPD, the upload fails, or `/files/get_image` cannot
    /// confirm that the returned image path belongs to the requested workspace.
    pub async fn upload_image(&self, upload: &ImageUpload) -> Result<VerifiedImageUpload> {
        let content_type = inline_image_content_type(&upload.file)?;
        let bytes = tokio::fs::read(&upload.file)
            .await
            .map_err(|source| Error::File {
                path: upload.file.display().to_string(),
                source,
            })?;
        let filename = upload
            .file
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::ImageVerification("invalid image filename".into()))?
            .to_owned();
        let part = Part::bytes(bytes)
            .file_name(filename)
            .mime_str(content_type)
            .map_err(|error| Error::Protocol {
                message: format!("invalid image MIME type: {error}"),
            })?;
        let form = Form::new()
            .text("workspace_id", upload.workspace_id.clone())
            .part("image", part);

        let response = self
            .http
            .post(self.endpoint("/files/upload_image")?)
            .multipart(form)
            .send()
            .await
            .map_err(Error::Transport)?;
        let data = decode_response(response).await?;
        let path = uploaded_image_path(&data).ok_or_else(|| {
            Error::ImageVerification("upload response has no TAPD image path".into())
        })?;

        let read_back = self
            .get(
                "/files/get_image",
                &vec![
                    ("workspace_id".into(), upload.workspace_id.clone()),
                    ("image_path".into(), path.clone()),
                ],
            )
            .await?;
        let image = record(&read_back, "Attachment")?;
        if field_string(&image, "value").as_deref() != Some(path.as_str()) {
            return Err(Error::ImageVerification(
                "read-back image path does not match the upload response".into(),
            ));
        }
        if field_string(&image, "workspace_id").as_deref() != Some(upload.workspace_id.as_str()) {
            return Err(Error::ImageVerification(
                "read-back image belongs to a different workspace".into(),
            ));
        }

        Ok(VerifiedImageUpload {
            image,
            path,
            verified: true,
        })
    }

    /// Build the interactive TAPD URL for a story.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured TAPD web base URL cannot be joined
    /// with the story path.
    pub fn story_url(&self, workspace_id: &str, story_id: &str) -> Result<Url> {
        self.web_base_url
            .join(&format!("tapd_fe/{workspace_id}/story/detail/{story_id}"))
            .map_err(Error::InvalidBaseUrl)
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.api_base_url
            .join(path.trim_start_matches('/'))
            .map_err(Error::InvalidBaseUrl)
    }
}

fn inline_image_content_type(path: &std::path::Path) -> Result<&'static str> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            Error::ImageVerification(
                "image filename must have a png, gif, jpg, jpeg, or bmp extension".into(),
            )
        })?;
    match extension.as_str() {
        "png" => Ok("image/png"),
        "gif" => Ok("image/gif"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "bmp" => Ok("image/bmp"),
        _ => Err(Error::ImageVerification(format!(
            "unsupported image extension {extension:?}; expected png, gif, jpg, jpeg, or bmp"
        ))),
    }
}

fn uploaded_image_path(value: &Value) -> Option<String> {
    match value {
        Value::String(candidate) => is_tapd_image_path(candidate).then(|| candidate.clone()),
        Value::Object(object) => {
            for key in ["image_path", "path", "value", "url"] {
                if let Some(candidate) = object
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|candidate| is_tapd_image_path(candidate))
                {
                    return Some(candidate.to_owned());
                }
            }
            object.values().find_map(uploaded_image_path)
        }
        Value::Array(values) => values.iter().find_map(uploaded_image_path),
        Value::Null | Value::Bool(_) | Value::Number(_) => None,
    }
}

fn is_tapd_image_path(value: &str) -> bool {
    let path = if value.starts_with("http://") || value.starts_with("https://") {
        Url::parse(value).ok().map(|url| url.path().to_owned())
    } else {
        Some(value.to_owned())
    };
    path.is_some_and(|path| {
        path.starts_with("/tfl/pictures/") || path.starts_with("/tfl/captures/")
    })
}

fn normalized_base_url(value: &str) -> Result<Url> {
    let mut normalized = value.trim_end_matches('/').to_owned();
    normalized.push('/');
    Url::parse(&normalized).map_err(Error::InvalidBaseUrl)
}

async fn decode_response(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body = response.text().await.map_err(Error::Transport)?;
    if !status.is_success() {
        return Err(Error::Http {
            status,
            body: truncate_chars(&body, MAX_ERROR_BODY_CHARS),
        });
    }

    let envelope: Value = serde_json::from_str(&body).map_err(|error| Error::Protocol {
        message: format!("{error}; response={}", truncate_chars(&body, 200)),
    })?;
    let Some(object) = envelope.as_object() else {
        return Err(Error::Protocol {
            message: "response envelope is not an object".into(),
        });
    };
    let api_status = object
        .get("status")
        .and_then(crate::value_as_string)
        .unwrap_or_default();
    if api_status != "1" {
        let info = object
            .get("info")
            .and_then(crate::value_as_string)
            .unwrap_or_else(|| "unknown TAPD error".into());
        return Err(Error::Api {
            status: api_status,
            info,
        });
    }
    Ok(object.get("data").cloned().unwrap_or(Value::Null))
}

fn truncate_chars(value: &str, maximum: usize) -> String {
    let mut chars = value.chars();
    let result: String = chars.by_ref().take(maximum).collect();
    if chars.next().is_some() {
        format!("{result}…")
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_debug_never_contains_token() {
        let client = TapdClient::builder()
            .access_token("a-sensitive-token")
            .build()
            .unwrap();
        let debug = format!("{client:?}");
        assert!(!debug.contains("a-sensitive-token"));
        assert!(!debug.contains("Authorization"));
    }

    #[test]
    fn truncation_is_unicode_safe() {
        assert_eq!(truncate_chars("中文内容", 2), "中文…");
    }
}
