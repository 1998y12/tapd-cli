use serde::{Deserialize, Serialize};
use url::Url;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapdResourceKind {
    Story,
    Bug,
    Task,
    Wiki,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapdUrl {
    pub workspace_id: String,
    pub resource: TapdResourceKind,
    pub resource_id: Option<String>,
}

impl TapdUrl {
    /// Parse a modern or legacy TAPD resource URL.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid URLs, non-TAPD hosts, or unrecognized
    /// resource paths.
    pub fn parse(value: &str) -> Result<Self> {
        let url = Url::parse(value).map_err(|error| Error::InvalidTapdUrl(error.to_string()))?;
        let host = url.host_str().unwrap_or_default();
        if host != "tapd.cn" && !host.ends_with(".tapd.cn") {
            return Err(Error::InvalidTapdUrl(format!(
                "expected a tapd.cn host, got {host:?}"
            )));
        }

        let segments: Vec<_> = url
            .path_segments()
            .map(Iterator::collect)
            .unwrap_or_default();
        if let Some(parsed) = parse_modern_path(&segments) {
            return Ok(parsed.with_query_fallback(&url));
        }
        if let Some(parsed) = parse_legacy_path(&segments) {
            return Ok(parsed.with_query_fallback(&url));
        }
        Err(Error::InvalidTapdUrl(format!(
            "unrecognized TAPD path {}",
            url.path()
        )))
    }

    fn with_query_fallback(mut self, url: &Url) -> Self {
        if self.resource_id.is_some() {
            return self;
        }
        for (key, value) in url.query_pairs() {
            if !matches!(key.as_ref(), "dialog_preview_id" | "preview_id") {
                continue;
            }
            if let Some((prefix, id)) = value.split_once('_')
                && resource_from_segment(prefix) == Some(self.resource)
                && !id.is_empty()
            {
                self.resource_id = Some(id.to_owned());
            }
        }
        self
    }
}

fn parse_modern_path(segments: &[&str]) -> Option<TapdUrl> {
    let tapd_fe = segments.iter().position(|segment| *segment == "tapd_fe")?;
    let workspace_id = *segments.get(tapd_fe + 1)?;
    let resource = resource_from_segment(segments.get(tapd_fe + 2)?)?;
    let resource_id = match (segments.get(tapd_fe + 3), segments.get(tapd_fe + 4)) {
        (Some(&"detail" | &"view"), Some(id)) => Some((*id).to_owned()),
        _ => None,
    };
    Some(TapdUrl {
        workspace_id: workspace_id.to_owned(),
        resource,
        resource_id,
    })
}

fn parse_legacy_path(segments: &[&str]) -> Option<TapdUrl> {
    let workspace_id = *segments.first()?;
    let resource_index = segments
        .iter()
        .position(|segment| resource_from_segment(segment).is_some())?;
    let resource = resource_from_segment(segments[resource_index])?;
    let resource_id = if matches!(segments.get(resource_index + 1), Some(&"view" | &"show")) {
        segments.get(resource_index + 2).map(|id| (*id).to_owned())
    } else {
        None
    };
    Some(TapdUrl {
        workspace_id: workspace_id.to_owned(),
        resource,
        resource_id,
    })
}

fn resource_from_segment(value: &str) -> Option<TapdResourceKind> {
    match value {
        "story" | "stories" => Some(TapdResourceKind::Story),
        "bug" | "bugs" => Some(TapdResourceKind::Bug),
        "task" | "tasks" => Some(TapdResourceKind::Task),
        "wiki" | "wikis" | "markdown_wikis" => Some(TapdResourceKind::Wiki),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_story_detail_url() {
        let parsed =
            TapdUrl::parse("https://www.tapd.cn/tapd_fe/37916943/story/detail/1137916943001018636")
                .unwrap();
        assert_eq!(parsed.workspace_id, "37916943");
        assert_eq!(parsed.resource, TapdResourceKind::Story);
        assert_eq!(parsed.resource_id.as_deref(), Some("1137916943001018636"));
    }

    #[test]
    fn parses_list_preview_query() {
        let parsed = TapdUrl::parse(
            "https://www.tapd.cn/tapd_fe/37916943/story/list?dialog_preview_id=story_123",
        )
        .unwrap();
        assert_eq!(parsed.resource_id.as_deref(), Some("123"));
    }
}
