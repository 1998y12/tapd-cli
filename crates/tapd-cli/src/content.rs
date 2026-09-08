use std::{collections::BTreeSet, path::Path, path::PathBuf};

use anyhow::{Context, Result, bail};
use pulldown_cmark::{Options, Parser, html};

use crate::cli::{DescriptionArgs, DescriptionFormat};

const IMAGE_PLACEHOLDER_PREFIX: &str = "{{tapd-image:";

#[derive(Debug, Clone)]
pub struct InlineImageSpec {
    pub name: String,
    pub file: PathBuf,
}

pub async fn read_content(args: &DescriptionArgs, required: bool) -> Result<Option<String>> {
    Ok(read_raw_content(args, required)
        .await?
        .map(|value| format_content(&value, args.description_format)))
}

pub async fn read_raw_content(args: &DescriptionArgs, required: bool) -> Result<Option<String>> {
    let raw = match (&args.description, &args.description_file) {
        (Some(value), None) => Some(value.clone()),
        (None, Some(path)) => Some(read_file(path).await?),
        (None, None) if required => bail!("description or description-file is required"),
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("clap enforces conflicting description inputs"),
    };
    Ok(raw)
}

pub fn parse_inline_image_specs(values: &[String]) -> Result<Vec<InlineImageSpec>> {
    if values.len() > 20 {
        bail!("at most 20 inline images may be uploaded in one command");
    }
    let mut names = BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let Some((name, file)) = value.split_once('=') else {
                bail!("expected NAME=PATH for --image, got {value:?}");
            };
            if !valid_image_name(name) {
                bail!(
                    "inline image name {name:?} must contain only ASCII letters, digits, '.', '_', or '-'"
                );
            }
            if file.is_empty() {
                bail!("inline image {name:?} has an empty file path");
            }
            if !names.insert(name.to_owned()) {
                bail!("inline image name {name:?} is configured more than once");
            }
            Ok(InlineImageSpec {
                name: name.to_owned(),
                file: PathBuf::from(file),
            })
        })
        .collect()
}

pub fn validate_inline_image_placeholders(
    raw: Option<&str>,
    format: DescriptionFormat,
    specs: &[InlineImageSpec],
) -> Result<()> {
    let raw = raw.unwrap_or_default();
    let referenced = placeholder_names(raw)?;
    if (!specs.is_empty() || !referenced.is_empty()) && matches!(format, DescriptionFormat::Plain) {
        bail!("inline images require --description-format markdown or html");
    }
    if !specs.is_empty() && raw.is_empty() {
        bail!("--image requires --description or --description-file");
    }

    let configured = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    for name in &referenced {
        if !configured.contains(name.as_str()) {
            bail!("inline image placeholder {name:?} has no matching --image NAME=PATH");
        }
    }
    for name in configured {
        if !referenced.contains(name) {
            bail!(
                "--image name {name:?} is not referenced by a {{{{tapd-image:{name}}}}} placeholder"
            );
        }
    }
    Ok(())
}

pub fn replace_inline_image_placeholder(raw: &mut String, name: &str, image_path: &str) {
    *raw = raw.replace(&format!("{{{{tapd-image:{name}}}}}"), image_path);
}

pub async fn validate_readable_file(path: &Path) -> Result<()> {
    let metadata = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !metadata.is_file() {
        bail!("{} is not a regular file", path.display());
    }
    let _ = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("failed to open {}", path.display()))?;
    Ok(())
}

pub async fn validate_inline_image_file(path: &Path) -> Result<()> {
    validate_readable_file(path).await?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(
        extension.as_deref(),
        Some("png" | "gif" | "jpg" | "jpeg" | "bmp")
    ) {
        bail!(
            "inline image {} must use a png, gif, jpg, jpeg, or bmp extension",
            path.display()
        );
    }
    Ok(())
}

async fn read_file(path: &Path) -> Result<String> {
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))
}

pub fn format_content(value: &str, format: DescriptionFormat) -> String {
    match format {
        DescriptionFormat::Plain => format!("<p>{}</p>", escape_html(value).replace('\n', "<br>")),
        DescriptionFormat::Markdown => {
            let parser = Parser::new_ext(value, Options::all());
            let mut output = String::new();
            html::push_html(&mut output, parser);
            output
        }
        DescriptionFormat::Html => value.to_owned(),
    }
}

fn placeholder_names(value: &str) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let mut remaining = value;
    while let Some(start) = remaining.find(IMAGE_PLACEHOLDER_PREFIX) {
        let after_prefix = &remaining[start + IMAGE_PLACEHOLDER_PREFIX.len()..];
        let Some(end) = after_prefix.find("}}") else {
            bail!("unterminated inline image placeholder");
        };
        let name = &after_prefix[..end];
        if !valid_image_name(name) {
            bail!("invalid inline image placeholder name {name:?}");
        }
        names.insert(name.to_owned());
        remaining = &after_prefix[end + 2..];
    }
    Ok(names)
}

fn valid_image_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
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
    fn validates_and_replaces_inline_image_placeholders() {
        let specs =
            parse_inline_image_specs(&["trend=./trend.png".into(), "route=./route.jpg".into()])
                .expect("parse image mappings");
        let mut raw: String =
            "![趋势]({{tapd-image:trend}})\n<img src=\"{{tapd-image:route}}\">".into();
        validate_inline_image_placeholders(Some(&raw), DescriptionFormat::Markdown, &specs)
            .expect("validate placeholders");
        replace_inline_image_placeholder(&mut raw, "trend", "/tfl/pictures/trend.png");
        replace_inline_image_placeholder(&mut raw, "route", "/tfl/pictures/route.jpg");
        assert!(raw.contains("![趋势](/tfl/pictures/trend.png)"));
        assert!(raw.contains("<img src=\"/tfl/pictures/route.jpg\">"));
    }

    #[test]
    fn rejects_missing_and_unused_inline_image_mappings() {
        let specs =
            parse_inline_image_specs(&["trend=./trend.png".into()]).expect("parse image mappings");
        let missing = validate_inline_image_placeholders(
            Some("![航迹]({{tapd-image:route}})"),
            DescriptionFormat::Markdown,
            &specs,
        )
        .expect_err("unmapped placeholder must fail");
        assert!(missing.to_string().contains("no matching --image"));

        let unused = validate_inline_image_placeholders(
            Some("没有图片"),
            DescriptionFormat::Markdown,
            &specs,
        )
        .expect_err("unused mapping must fail");
        assert!(unused.to_string().contains("is not referenced"));
    }
}
