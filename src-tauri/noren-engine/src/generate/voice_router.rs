use crate::types::VoiceMetadata;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub model: String,
    pub reason: String,
}

fn normalize_format(format: &str) -> String {
    let lower = format.to_lowercase();
    match lower.as_str() {
        "twitter" => "tweet".to_string(),
        "threads" => "thread".to_string(),
        _ => lower,
    }
}

fn short_formats() -> HashSet<&'static str> {
    ["tweet", "email", "thread", "slack"].into_iter().collect()
}

fn long_formats() -> HashSet<&'static str> {
    ["blog", "essay", "newsletter", "article", "longform"]
        .into_iter()
        .collect()
}

pub fn route_voice_to_model(metadata: &VoiceMetadata, format: &str) -> RouteDecision {
    let normalized = normalize_format(format);

    // Short-form: always Sonnet. Opus adds latency for minimal quality gain.
    if short_formats().contains(normalized.as_str()) {
        return RouteDecision {
            model: "claude-sonnet-4-6".to_string(),
            reason: "short-form, Sonnet preferred".to_string(),
        };
    }

    // Long-form: route to Opus for sustained formal prose
    if long_formats().contains(normalized.as_str()) {
        let rhythm = metadata
            .format_rhythms
            .as_ref()
            .and_then(|fr| fr.get(&normalized))
            .or(metadata.baseline_rhythm.as_ref());

        let rhythm = match rhythm {
            Some(r) => r,
            None => {
                return RouteDecision {
                    model: "claude-sonnet-4-6".to_string(),
                    reason: "no rhythm data, default".to_string(),
                };
            }
        };

        let long_to_short_ratio = rhythm.long_to_short_ratio;
        let is_low_casual = metadata.routing.casual_marker_density == "low";

        if long_to_short_ratio >= 2.5 && is_low_casual {
            return RouteDecision {
                model: "claude-opus-4-6".to_string(),
                reason: format!("sustained formal prose (L:S {})", long_to_short_ratio),
            };
        }

        return RouteDecision {
            model: "claude-sonnet-4-6".to_string(),
            reason: "conversational or short-sentence voice".to_string(),
        };
    }

    // Default
    RouteDecision {
        model: "claude-sonnet-4-6".to_string(),
        reason: "default route".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::collections::HashMap;

    fn make_rhythm(long_to_short_ratio: f64) -> BaselineRhythm {
        BaselineRhythm {
            total_sentences: 100,
            median_word_count: 14.0,
            mean_word_count: 16.0,
            distribution: RhythmDistribution {
                short: 20.0,
                medium: 40.0,
                long: 30.0,
                very_long: 10.0,
            },
            distribution_pct: RhythmDistribution {
                short: 20.0,
                medium: 40.0,
                long: 30.0,
                very_long: 10.0,
            },
            long_to_short_ratio,
            median_commas_per_sentence: 1.2,
            mean_commas_per_sentence: 1.5,
            mean_paragraph_sentences: 3.5,
            sentence_ceiling: 38,
        }
    }

    fn make_metadata(
        casual_marker_density: &str,
        baseline_rhythm: Option<BaselineRhythm>,
        format_rhythms: Option<HashMap<String, BaselineRhythm>>,
    ) -> VoiceMetadata {
        VoiceMetadata {
            version: 1,
            routing: VoiceRoutingFields {
                structure_predictability: "medium".to_string(),
                register_break_frequency: 2,
                casual_marker_density: casual_marker_density.to_string(),
                signature_phrase_rigidity: "medium".to_string(),
            },
            corpus: CorpusInfo {
                unique_sample_count: 50,
                formats: vec!["blog".to_string(), "tweet".to_string()],
            },
            counts: MetadataCounts {
                analogy_domains: 5,
                micro_constructions: 8,
                signature_phrases: 4,
                anti_patterns: 3,
                profile_lines: 200,
            },
            baseline_rhythm,
            format_rhythms,
        }
    }

    fn pg_metadata() -> VoiceMetadata {
        let mut format_rhythms = HashMap::new();
        format_rhythms.insert("blog".to_string(), make_rhythm(3.11));
        format_rhythms.insert("twitter".to_string(), make_rhythm(1.37));
        make_metadata("low", Some(make_rhythm(2.5)), Some(format_rhythms))
    }

    fn haseeb_metadata() -> VoiceMetadata {
        let mut format_rhythms = HashMap::new();
        format_rhythms.insert("blog".to_string(), make_rhythm(2.93));
        format_rhythms.insert("twitter".to_string(), make_rhythm(1.65));
        make_metadata("low", Some(make_rhythm(2.2)), Some(format_rhythms))
    }

    fn werg_metadata() -> VoiceMetadata {
        let mut format_rhythms = HashMap::new();
        format_rhythms.insert("blog".to_string(), make_rhythm(3.86));
        format_rhythms.insert("twitter".to_string(), make_rhythm(1.32));
        make_metadata("medium", Some(make_rhythm(3.0)), Some(format_rhythms))
    }

    fn founder_metadata() -> VoiceMetadata {
        make_metadata("medium", Some(make_rhythm(0.55)), None)
    }

    // --- Short-form always routes to Sonnet ---

    #[test]
    fn short_form_tweet() {
        let r = route_voice_to_model(&pg_metadata(), "tweet");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "short-form, Sonnet preferred");
    }

    #[test]
    fn short_form_twitter_alias() {
        let r = route_voice_to_model(&pg_metadata(), "twitter");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "short-form, Sonnet preferred");
    }

    #[test]
    fn short_form_email() {
        let r = route_voice_to_model(&pg_metadata(), "email");
        assert_eq!(r.model, "claude-sonnet-4-6");
    }

    #[test]
    fn short_form_thread() {
        let r = route_voice_to_model(&pg_metadata(), "thread");
        assert_eq!(r.model, "claude-sonnet-4-6");
    }

    #[test]
    fn short_form_slack() {
        let r = route_voice_to_model(&pg_metadata(), "slack");
        assert_eq!(r.model, "claude-sonnet-4-6");
    }

    // --- Long-form routes based on rhythm + formality ---

    #[test]
    fn pg_blog_routes_to_opus() {
        let r = route_voice_to_model(&pg_metadata(), "blog");
        assert_eq!(r.model, "claude-opus-4-6");
        assert!(r.reason.contains("sustained formal prose"));
        assert!(r.reason.contains("3.11"));
    }

    #[test]
    fn haseeb_blog_routes_to_opus() {
        let r = route_voice_to_model(&haseeb_metadata(), "blog");
        assert_eq!(r.model, "claude-opus-4-6");
        assert!(r.reason.contains("sustained formal prose"));
    }

    #[test]
    fn werg_blog_routes_to_sonnet() {
        let r = route_voice_to_model(&werg_metadata(), "blog");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "conversational or short-sentence voice");
    }

    #[test]
    fn founder_blog_routes_to_sonnet() {
        let r = route_voice_to_model(&founder_metadata(), "blog");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "conversational or short-sentence voice");
    }

    #[test]
    fn essay_routes_same_as_blog() {
        let r = route_voice_to_model(&pg_metadata(), "essay");
        assert_eq!(r.model, "claude-opus-4-6");
    }

    #[test]
    fn newsletter_routes_same_as_blog() {
        let r = route_voice_to_model(&pg_metadata(), "newsletter");
        assert_eq!(r.model, "claude-opus-4-6");
    }

    #[test]
    fn article_routes_same_as_blog() {
        let r = route_voice_to_model(&pg_metadata(), "article");
        assert_eq!(r.model, "claude-opus-4-6");
    }

    #[test]
    fn longform_routes_same_as_blog() {
        let r = route_voice_to_model(&pg_metadata(), "longform");
        assert_eq!(r.model, "claude-opus-4-6");
    }

    // --- Rhythm fallback ---

    #[test]
    fn no_rhythm_data_defaults_to_sonnet() {
        let meta = make_metadata("low", None, None);
        let r = route_voice_to_model(&meta, "blog");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "no rhythm data, default");
    }

    #[test]
    fn baseline_fallback_when_no_format_rhythm() {
        let meta = make_metadata("low", Some(make_rhythm(3.0)), None);
        let r = route_voice_to_model(&meta, "essay");
        assert_eq!(r.model, "claude-opus-4-6");
        assert!(r.reason.contains("sustained formal prose"));
    }

    #[test]
    fn format_rhythm_takes_priority_over_baseline() {
        let mut format_rhythms = HashMap::new();
        format_rhythms.insert("blog".to_string(), make_rhythm(1.8));
        let meta = make_metadata("low", Some(make_rhythm(3.5)), Some(format_rhythms));
        let r = route_voice_to_model(&meta, "blog");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "conversational or short-sentence voice");
    }

    // --- Threshold boundary ---

    #[test]
    fn exactly_2_5_with_low_casual_routes_opus() {
        let meta = make_metadata("low", Some(make_rhythm(2.5)), None);
        let r = route_voice_to_model(&meta, "blog");
        assert_eq!(r.model, "claude-opus-4-6");
    }

    #[test]
    fn below_threshold_2_49_routes_sonnet() {
        let meta = make_metadata("low", Some(make_rhythm(2.49)), None);
        let r = route_voice_to_model(&meta, "blog");
        assert_eq!(r.model, "claude-sonnet-4-6");
    }

    #[test]
    fn high_ratio_but_high_casual_routes_sonnet() {
        let meta = make_metadata("high", Some(make_rhythm(4.0)), None);
        let r = route_voice_to_model(&meta, "blog");
        assert_eq!(r.model, "claude-sonnet-4-6");
    }

    #[test]
    fn unknown_format_returns_default() {
        let r = route_voice_to_model(&pg_metadata(), "haiku");
        assert_eq!(r.model, "claude-sonnet-4-6");
        assert_eq!(r.reason, "default route");
    }

    #[test]
    fn twitter_alias_matches_tweet() {
        let tweet = route_voice_to_model(&pg_metadata(), "tweet");
        let twitter = route_voice_to_model(&pg_metadata(), "twitter");
        assert_eq!(twitter.model, tweet.model);
        assert_eq!(twitter.reason, tweet.reason);
    }
}
