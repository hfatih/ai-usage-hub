//! Request-routing decisions for the local proxy that need no network or database: user model
//! rules, background-task detection, image options and account tier ranking.

use regex::RegexBuilder;
use serde_json::{json, Value};

use crate::models::ModelMapRule;

/// Returns the upstream model the first matching user rule sends `input_model` to.
///
/// A rule's `from` is compared case-insensitively. It may be an exact name, a prefix ending in
/// `*`, or a regular expression written as `re:<pattern>` (searched anywhere in the name; anchor
/// with `^`/`$` for a full match). Rules with an empty side or an invalid pattern never match.
pub fn mapped_model(input_model: &str, rules: &[ModelMapRule]) -> Option<String> {
    let input = input_model.to_ascii_lowercase();
    rules.iter().find_map(|rule| {
        let from = rule.from.trim();
        let to = rule.to.trim();
        if from.is_empty() || to.is_empty() {
            return None;
        }
        let matches = if let Some(pattern) = from.strip_prefix("re:") {
            RegexBuilder::new(pattern.trim())
                .case_insensitive(true)
                .build()
                .is_ok_and(|regex| regex.is_match(input_model))
        } else {
            let from = from.to_ascii_lowercase();
            match from.strip_suffix('*') {
                Some(prefix) => input.starts_with(prefix),
                None => input == from,
            }
        };
        matches.then(|| to.to_owned())
    })
}

/// Phrases coding tools put in the housekeeping requests they send next to the real work.
// ponytail: fixed phrase list taken from Claude Code's title and suggestion prompts; a tool that
// words them differently is simply not downgraded. Conversation compaction is left out on
// purpose, because a poor summary costs more than the quota it saves.
const BACKGROUND_MARKERS: &[&str] = &[
    "write a 5-10 word title",
    "sentence-case title",
    "extract a 2-3 word title",
    "indicates a new conversation topic",
    "summarize this coding conversation in under",
    "suggest what the user might naturally type next",
];

/// Whether a chat request is a short housekeeping task (conversation title, topic check, next
/// prompt suggestion) rather than real work, judged from the system prompt and the start of the
/// last user message. Such requests can go to a cheap model without the user noticing.
pub fn is_background_task(system: &str, last_user_message: &str) -> bool {
    let head: String = last_user_message.chars().take(600).collect();
    let text = format!("{system}\n{head}").to_ascii_lowercase();
    BACKGROUND_MARKERS.iter().any(|marker| text.contains(marker))
}

/// What an image request asks for once the OpenAI-style fields and model-name suffixes are read.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageOptions {
    /// Requested model with any ratio/quality suffix removed.
    pub model: String,
    /// One of the ratios the image model accepts (`16:9`, `1:1`, ...), if the request named one.
    pub aspect_ratio: Option<&'static str>,
    /// `2K` or `4K` when a higher resolution was asked for; `None` leaves the model default.
    pub image_size: Option<&'static str>,
    /// Number of images to produce, 1 to 10.
    pub count: usize,
    /// Return `data:` URLs in `url` instead of raw base64 in `b64_json`.
    pub as_url: bool,
}

const RATIOS: &[(&str, f64)] = &[
    ("21:9", 21.0 / 9.0),
    ("16:9", 16.0 / 9.0),
    ("4:3", 4.0 / 3.0),
    ("1:1", 1.0),
    ("3:4", 3.0 / 4.0),
    ("9:16", 9.0 / 16.0),
];

fn ratio_from_size(size: &str) -> Option<&'static str> {
    let (width, height) = size.trim().split_once(['x', 'X', ':'])?;
    let (width, height) = (width.trim().parse::<f64>().ok()?, height.trim().parse::<f64>().ok()?);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let wanted = width / height;
    RATIOS
        .iter()
        .min_by(|a, b| (a.1 - wanted).abs().total_cmp(&(b.1 - wanted).abs()))
        .map(|(name, _)| *name)
}

fn size_name(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "2k" | "medium" => Some("2K"),
        "4k" | "hd" | "high" => Some("4K"),
        _ => None,
    }
}

/// Reads an OpenAI Images request body.
///
/// Sources, strongest first: the explicit `imageSize` / `size` / `quality` fields, then suffixes
/// on the model name such as `-16-9` or `-4k` (`gemini-3-pro-image-16-9-4k`). `size` accepts
/// `WIDTHxHEIGHT` or `W:H` and is rounded to the nearest supported ratio. `n` is clamped to 1-10.
pub fn image_options(request: &Value) -> ImageOptions {
    let text = |key: &str| request.get(key).and_then(Value::as_str);
    let mut model = text("model").unwrap_or("image").to_owned();
    let mut suffix_size = None;
    let mut suffix_ratio = None;
    for size in ["-4k", "-2k"] {
        if model.to_ascii_lowercase().ends_with(size) {
            suffix_size = size_name(&size[1..]);
            model.truncate(model.len() - size.len());
        }
    }
    for (name, _) in RATIOS {
        let suffix = format!("-{}", name.replace(':', "-"));
        if model.ends_with(&suffix) {
            suffix_ratio = Some(*name);
            model.truncate(model.len() - suffix.len());
        }
    }
    ImageOptions {
        model,
        aspect_ratio: text("size").and_then(ratio_from_size).or(suffix_ratio),
        image_size: text("imageSize").and_then(size_name).or_else(|| text("quality").and_then(size_name)).or(suffix_size),
        count: request.get("n").and_then(Value::as_u64).unwrap_or(1).clamp(1, 10) as usize,
        as_url: text("response_format") == Some("url"),
    }
}

/// The `generationConfig` an image request is sent upstream with.
pub fn image_generation_config(options: &ImageOptions) -> Value {
    let mut config = json!({ "responseModalities": ["TEXT", "IMAGE"] });
    let mut image = serde_json::Map::new();
    if let Some(ratio) = options.aspect_ratio {
        image.insert("aspectRatio".into(), ratio.into());
    }
    if let Some(size) = options.image_size {
        image.insert("imageSize".into(), size.into());
    }
    if !image.is_empty() {
        config["imageConfig"] = Value::Object(image);
    }
    config
}

/// Sort key for tier-priority routing: Ultra accounts first, then Pro, then everything else.
pub fn tier_rank(tier: Option<&str>) -> u8 {
    let tier = tier.unwrap_or_default().to_ascii_lowercase();
    if tier.contains("ultra") {
        0
    } else if tier.contains("pro") {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str) -> ModelMapRule {
        ModelMapRule { from: from.into(), to: to.into() }
    }

    #[test]
    fn rules_match_exact_prefix_and_regex() {
        let rules = [rule("GPT-4*", "a"), rule("re:^claude-.*-haiku", "b"), rule("re:([", "broken"), rule("o1", "c")];
        assert_eq!(mapped_model("gpt-4o-mini", &rules).as_deref(), Some("a"));
        assert_eq!(mapped_model("Claude-3-5-Haiku", &rules).as_deref(), Some("b"));
        assert_eq!(mapped_model("o1", &rules).as_deref(), Some("c"));
        assert_eq!(mapped_model("o1-preview", &rules), None);
        assert_eq!(mapped_model("x", &[rule("", "a"), rule("x", " ")]), None);
    }

    #[test]
    fn only_housekeeping_prompts_count_as_background() {
        assert!(is_background_task("", "Please write a 5-10 word title for the following conversation:"));
        assert!(is_background_task("Analyze if this message indicates a new conversation topic.", "hi"));
        assert!(!is_background_task("You are a coding agent.", "Fix the failing test in parser.rs"));
        assert!(!is_background_task("", "Your task is to create a detailed summary of the conversation so far"));
    }

    #[test]
    fn image_options_read_fields_then_model_suffix() {
        let options = image_options(&json!({"model": "gemini-3-pro-image-16-9-4k", "n": 99}));
        assert_eq!((options.model.as_str(), options.aspect_ratio, options.image_size, options.count), ("gemini-3-pro-image", Some("16:9"), Some("4K"), 10));

        let options = image_options(&json!({"size": "1280x720", "quality": "medium", "response_format": "url"}));
        assert_eq!((options.aspect_ratio, options.image_size, options.as_url), (Some("16:9"), Some("2K"), true));
        assert_eq!(image_generation_config(&options)["imageConfig"], json!({"aspectRatio": "16:9", "imageSize": "2K"}));

        let plain = image_options(&json!({"prompt": "a cat", "quality": "standard"}));
        assert_eq!((plain.aspect_ratio, plain.image_size, plain.count), (None, None, 1));
        assert!(image_generation_config(&plain).get("imageConfig").is_none());
    }

    #[test]
    fn tiers_rank_ultra_before_pro_before_the_rest() {
        assert!(tier_rank(Some("g1-ultra-tier")) < tier_rank(Some("g1-pro-tier")));
        assert!(tier_rank(Some("g1-pro-tier")) < tier_rank(Some("free-tier")));
        assert_eq!(tier_rank(None), tier_rank(Some("free-tier")));
    }
}
