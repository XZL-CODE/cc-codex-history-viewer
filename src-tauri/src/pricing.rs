//! Conservative API-equivalent model pricing.
//!
//! Rates are USD per million tokens. Claude and Codex rates were checked against the Anthropic
//! and OpenAI model pricing pages on 2026-07-17, Gemini rates against Google's Gemini API pricing
//! page on 2026-11-26. Unknown or ambiguous model IDs intentionally return `None`.
//!
//! A model can have several rates with different `since` days: each call is priced at the rate
//! in effect on its UTC day, so a later price change does not reprice older calls.

use crate::models::{Agent, CostParts, NormalizedUsage};
use chrono::{DateTime, NaiveDate};

#[derive(Clone, Copy)]
struct Rate {
    model: &'static str,
    /// First UTC day (`yyyy-mm-dd`) the rate applies to; `None` = from the model's release.
    since: Option<&'static str>,
    input: f64,
    cached_input: f64,
    /// 5-minute cache write rate (Claude: 1.25x input).
    cache_write: f64,
    /// 1-hour cache write rate (Claude: 2x input). Codex and Gemini never record cache writes.
    cache_write_1h: f64,
    output: f64,
    long_context: Option<LongContext>,
}

/// Higher tier for long prompts: when uncached + cached input exceeds `threshold`, the whole
/// call's input (cached included) and output are billed at the multiplied rates.
#[derive(Clone, Copy)]
struct LongContext {
    threshold: u64,
    input_multiplier: f64,
    output_multiplier: f64,
}

/// OpenAI's long-context rule for models with a 1M-token window.
const OPENAI_LONG_CONTEXT: LongContext = LongContext {
    threshold: 272_000,
    input_multiplier: 2.0,
    output_multiplier: 1.5,
};

/// Gemini Pro models bill prompts over 200k tokens at 2x input / cached input and 1.5x output.
const GEMINI_LONG_CONTEXT: LongContext = LongContext {
    threshold: 200_000,
    input_multiplier: 2.0,
    output_multiplier: 1.5,
};

const CLAUDE_RATES: &[Rate] = &[
    rate("claude-fable-5", 10.0, 1.0, 12.5, 50.0),
    rate("claude-opus-4-8", 5.0, 0.5, 6.25, 25.0),
    rate("claude-opus-4-7", 5.0, 0.5, 6.25, 25.0),
    rate("claude-opus-4-6", 5.0, 0.5, 6.25, 25.0),
    rate("claude-opus-4-5", 5.0, 0.5, 6.25, 25.0),
    rate("claude-opus-4-1", 15.0, 1.5, 18.75, 75.0),
    rate("claude-opus-4-0", 15.0, 1.5, 18.75, 75.0),
    // Date snapshots such as claude-opus-4-20250514 represent Opus 4.0.
    rate("claude-opus-4-20", 15.0, 1.5, 18.75, 75.0),
    rate("claude-sonnet-5", 2.0, 0.2, 2.5, 10.0),
    rate("claude-sonnet-4-6", 3.0, 0.3, 3.75, 15.0),
    rate("claude-sonnet-4-5", 3.0, 0.3, 3.75, 15.0),
    rate("claude-sonnet-4-0", 3.0, 0.3, 3.75, 15.0),
    rate("claude-sonnet-4-20", 3.0, 0.3, 3.75, 15.0),
    rate("claude-3-7-sonnet", 3.0, 0.3, 3.75, 15.0),
    rate("claude-3-5-sonnet", 3.0, 0.3, 3.75, 15.0),
    rate("claude-haiku-4-5", 1.0, 0.1, 1.25, 5.0),
    rate("claude-3-5-haiku", 0.8, 0.08, 1.0, 4.0),
    rate("claude-3-haiku", 0.25, 0.025, 0.3125, 1.25),
];

const CODEX_RATES: &[Rate] = &[
    openai_long("gpt-5.5", 5.0, 0.5, 30.0),
    openai("gpt-5.4-mini", 0.75, 0.075, 4.5),
    openai("gpt-5.4-nano", 0.2, 0.02, 1.25),
    openai_long("gpt-5.4", 2.5, 0.25, 15.0),
    openai("gpt-5.3-codex", 1.75, 0.175, 14.0),
    openai("gpt-5.2-codex", 1.75, 0.175, 14.0),
    openai("gpt-5.2", 1.75, 0.175, 14.0),
    openai("gpt-5.1-codex-max", 1.25, 0.125, 10.0),
    openai("gpt-5.1-codex", 1.25, 0.125, 10.0),
    openai("gpt-5.1", 1.25, 0.125, 10.0),
    openai("gpt-5-codex", 1.25, 0.125, 10.0),
    openai("gpt-5", 1.25, 0.125, 10.0),
    openai("gpt-5-mini", 0.25, 0.025, 2.0),
    openai("o4-mini", 1.1, 0.275, 4.4),
    openai("gpt-4.1", 2.0, 0.5, 8.0),
];

/// Gemini API Standard paid-tier rates from the official pricing page
/// (<https://aistudio.google.com/docs/pricing>) and the deprecation notes, checked 2026-11-26.
/// A row holds from its `since` day (or the model's release) until the model's next row; prices
/// the page lists without a start date are assumed to hold since release. Retired models the
/// page no longer prices (gemini-3.5-flash, gemini-3.7-flash, gemini-2.0-flash, 2.5 previews,
/// gemini-3-pro-preview before it pointed to 3.1 Pro) stay unpriced. Explicit-cache storage
/// (per hour) cannot be derived from transcripts and is not counted.
const GEMINI_RATES: &[Rate] = &[
    gemini("gemini-4-neon", None, 3.0, 0.15, 15.0),
    gemini("gemini-4-hydrogen", None, 1.5, 0.075, 7.5),
    // Introductory price through 2026-11-30.
    gemini("gemini-4-carbon", None, 2.0, 0.1, 10.0),
    gemini("gemini-4-carbon", Some("2026-12-01"), 4.0, 0.2, 20.0),
    gemini("gemini-4-argon", None, 2.0, 0.1, 10.0),
    gemini("gemini-4-argon", Some("2026-12-01"), 4.0, 0.2, 20.0),
    // Introductory price through 2026-12-31.
    gemini("gemini-3.8-flash", None, 0.75, 0.075, 3.75),
    gemini("gemini-3.8-flash", Some("2027-01-01"), 1.5, 0.15, 7.5),
    gemini("gemini-3.6-flash", None, 0.75, 0.075, 3.75),
    gemini("gemini-3.6-flash", Some("2027-01-01"), 1.5, 0.15, 7.5),
    gemini("gemini-3.5-flash-lite", None, 0.3, 0.03, 2.5),
    gemini_long("gemini-3.1-pro-preview", None, 2.0, 0.2, 12.0),
    gemini_long("gemini-3.1-pro-preview-customtools", None, 2.0, 0.2, 12.0),
    // Points to gemini-3.1-pro-preview since 2026-03-09; its earlier price is no longer listed.
    gemini_long("gemini-3-pro-preview", Some("2026-03-09"), 2.0, 0.2, 12.0),
    gemini("gemini-3.1-flash-lite", None, 0.25, 0.025, 1.5),
    gemini("gemini-3-flash-preview", None, 0.5, 0.05, 3.0),
    gemini_long("gemini-2.5-pro", None, 1.25, 0.125, 10.0),
    // From 2026-12-01 requests route to gemini-3.8-flash and are billed at its rates.
    gemini("gemini-2.5-pro", Some("2026-12-01"), 0.75, 0.075, 3.75),
    gemini("gemini-2.5-pro", Some("2027-01-01"), 1.5, 0.15, 7.5),
    // From 2026-12-01 requests route to gemini-3.5-flash-lite, billed at the same rates.
    gemini("gemini-2.5-flash", None, 0.3, 0.03, 2.5),
    gemini("gemini-2.5-flash-lite", None, 0.1, 0.01, 0.4),
];

/// Antigravity CLI appends the reasoning effort to the model name (`gemini-4-neon-xhigh`); the
/// API model ID is the name without it.
const EFFORT_SUFFIXES: &[&str] = &["-minimal", "-low", "-medium", "-high", "-xhigh", "-max"];

fn gemini_model_id(model: &str) -> &str {
    EFFORT_SUFFIXES
        .iter()
        .find_map(|suffix| model.strip_suffix(suffix))
        .unwrap_or(model)
}

const fn rate(
    model: &'static str,
    input: f64,
    cached_input: f64,
    cache_write: f64,
    output: f64,
) -> Rate {
    Rate {
        model,
        since: None,
        input,
        cached_input,
        cache_write,
        cache_write_1h: input * 2.0,
        output,
        long_context: None,
    }
}

const fn openai(model: &'static str, input: f64, cached_input: f64, output: f64) -> Rate {
    rate(model, input, cached_input, input * 1.25, output)
}

const fn openai_long(model: &'static str, input: f64, cached_input: f64, output: f64) -> Rate {
    Rate {
        long_context: Some(OPENAI_LONG_CONTEXT),
        ..openai(model, input, cached_input, output)
    }
}

/// Gemini bills implicit cache hits at `cached_input` and records no cache writes; should one
/// appear it is billed as plain input.
const fn gemini(
    model: &'static str,
    since: Option<&'static str>,
    input: f64,
    cached_input: f64,
    output: f64,
) -> Rate {
    Rate {
        model,
        since,
        input,
        cached_input,
        cache_write: input,
        cache_write_1h: input,
        output,
        long_context: None,
    }
}

const fn gemini_long(
    model: &'static str,
    since: Option<&'static str>,
    input: f64,
    cached_input: f64,
    output: f64,
) -> Rate {
    Rate {
        long_context: Some(GEMINI_LONG_CONTEXT),
        ..gemini(model, since, input, cached_input, output)
    }
}

fn is_snapshot_or_exact(model: &str, base: &str) -> bool {
    if model == base {
        return true;
    }
    model
        .strip_prefix(base)
        .and_then(|suffix| suffix.strip_prefix('-'))
        .is_some_and(|suffix| {
            let (shape_matches, format) = match suffix.as_bytes() {
                bytes if bytes.len() == 8 && bytes.iter().all(u8::is_ascii_digit) => {
                    (true, "%Y%m%d")
                }
                bytes
                    if bytes.len() == 10
                        && bytes[4] == b'-'
                        && bytes[7] == b'-'
                        && bytes.iter().enumerate().all(|(index, byte)| {
                            [4, 7].contains(&index) || byte.is_ascii_digit()
                        }) =>
                {
                    (true, "%Y-%m-%d")
                }
                _ => (false, ""),
            };
            shape_matches && NaiveDate::parse_from_str(suffix, format).is_ok()
        })
}

/// The call's UTC day as `yyyy-mm-dd`, comparable with [`Rate::since`].
fn call_day(timestamp_ms: i64) -> String {
    DateTime::from_timestamp_millis(timestamp_ms)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string()
}

/// The rate for `model` on the call's day: the most specific matching model ID wins, then the
/// latest `since` that is not after the call.
fn lookup(agent: Agent, model: &str, timestamp_ms: i64) -> Option<Rate> {
    // Antigravity calls are priced at Gemini API rates only; other vendors' models it can run
    // (such as Claude) stay unpriced.
    let (rates, model) = match agent {
        Agent::Claude => (CLAUDE_RATES, model),
        Agent::Codex => (CODEX_RATES, model),
        Agent::Antigravity => (GEMINI_RATES, gemini_model_id(model)),
    };
    let matches = |rate: &&Rate| match agent {
        Agent::Claude => model.starts_with(rate.model),
        Agent::Codex => is_snapshot_or_exact(model, rate.model),
        Agent::Antigravity => model == rate.model,
    };
    let longest = rates
        .iter()
        .filter(matches)
        .map(|rate| rate.model.len())
        .max()?;
    let day = call_day(timestamp_ms);
    rates
        .iter()
        .filter(matches)
        .filter(|rate| rate.model.len() == longest)
        .filter(|rate| rate.since.map_or(true, |since| since <= day.as_str()))
        .max_by_key(|rate| rate.since)
        .copied()
}

/// Estimate standard API-equivalent cost of a call made at `timestamp_ms` (Unix milliseconds).
/// It is not a subscription charge.
pub fn estimate_cost(
    agent: Agent,
    model: &str,
    timestamp_ms: i64,
    usage: NormalizedUsage,
) -> Option<f64> {
    estimate_cost_parts(agent, model, timestamp_ms, usage).map(CostParts::total)
}

/// [`estimate_cost`] split by billing category; the parts add up to the estimate.
pub fn estimate_cost_parts(
    agent: Agent,
    model: &str,
    timestamp_ms: i64,
    usage: NormalizedUsage,
) -> Option<CostParts> {
    let rate = lookup(agent, model, timestamp_ms)?;
    let (input_multiplier, output_multiplier) = multipliers(rate, usage);
    Some(CostParts {
        uncached_input: usage.uncached_input as f64 * rate.input * input_multiplier / 1_000_000.0,
        cache_read: usage.cache_read as f64 * rate.cached_input * input_multiplier / 1_000_000.0,
        cache_creation: (usage.cache_creation_5m() as f64 * rate.cache_write
            + usage.cache_creation_1h as f64 * rate.cache_write_1h)
            * input_multiplier
            / 1_000_000.0,
        output: usage.output as f64 * rate.output * output_multiplier / 1_000_000.0,
    })
}

/// What the same call would cost with every input token billed at the uncached rate, i.e.
/// without prompt caching. `estimate_cost_without_cache - estimate_cost` is the cache saving,
/// which is negative when cache writes cost more than the reads they enabled.
pub fn estimate_cost_without_cache(
    agent: Agent,
    model: &str,
    timestamp_ms: i64,
    usage: NormalizedUsage,
) -> Option<f64> {
    let rate = lookup(agent, model, timestamp_ms)?;
    let (input_multiplier, output_multiplier) = multipliers(rate, usage);
    Some(
        (usage.context_tokens() as f64 * rate.input * input_multiplier
            + usage.output as f64 * rate.output * output_multiplier)
            / 1_000_000.0,
    )
}

/// Long-context surcharge for rates with a [`LongContext`] tier: (input, output) multipliers.
fn multipliers(rate: Rate, usage: NormalizedUsage) -> (f64, f64) {
    let input_total = usage.uncached_input.saturating_add(usage.cache_read);
    match rate.long_context {
        Some(tier) if input_total > tier.threshold => {
            (tier.input_multiplier, tier.output_multiplier)
        }
        _ => (1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call day after every dated rate in the Claude and Codex tables.
    const T: i64 = 1_790_000_000_000;

    fn usage(input: u64, cached: u64, creation: u64, output: u64) -> NormalizedUsage {
        NormalizedUsage {
            uncached_input: input,
            cache_read: cached,
            cache_creation: creation,
            cache_creation_1h: 0,
            output,
            reasoning_output: output / 2,
        }
    }

    #[test]
    fn claude_cache_rates_are_accounted_separately() {
        let cost = estimate_cost(
            Agent::Claude,
            "claude-sonnet-4-5-20250929",
            T,
            usage(1_000_000, 1_000_000, 1_000_000, 1_000_000),
        )
        .unwrap();
        assert!((cost - 22.05).abs() < 1e-9);
    }

    #[test]
    fn one_hour_cache_writes_cost_twice_the_input_rate() {
        let mut split = usage(0, 0, 1_000_000, 0);
        split.cache_creation_1h = 400_000;
        let parts =
            estimate_cost_parts(Agent::Claude, "claude-sonnet-4-5-20250929", T, split).unwrap();
        // 600k at 3.75 + 400k at 6.00
        assert!((parts.cache_creation - (2.25 + 2.4)).abs() < 1e-9);
        assert_eq!(parts.uncached_input, 0.0);
        assert!(
            (parts.total() - estimate_cost(Agent::Claude, "claude-sonnet-4-5", T, split).unwrap())
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn cost_parts_add_up_and_no_cache_estimate_bills_all_input_uncached() {
        let call = usage(100_000, 900_000, 50_000, 10_000);
        let parts = estimate_cost_parts(Agent::Claude, "claude-sonnet-4-5", T, call).unwrap();
        let total = estimate_cost(Agent::Claude, "claude-sonnet-4-5", T, call).unwrap();
        assert!((parts.total() - total).abs() < 1e-12);
        let no_cache =
            estimate_cost_without_cache(Agent::Claude, "claude-sonnet-4-5", T, call).unwrap();
        // 1.05M input at 3.00 + 10k output at 15.00
        assert!((no_cache - (3.15 + 0.15)).abs() < 1e-9);
        assert!(no_cache > total, "reads at 0.3 beat writes at 3.75 here");

        // Codex: cached input is discounted, nothing is written, long-context rule applies to both.
        let codex = usage(300_000, 100_000, 0, 100_000);
        let priced = estimate_cost(Agent::Codex, "gpt-5.4", T, codex).unwrap();
        let unpriced = estimate_cost_without_cache(Agent::Codex, "gpt-5.4", T, codex).unwrap();
        assert!((priced - (1.5 + 0.05 + 2.25)).abs() < 1e-9);
        assert!((unpriced - (2.0 + 2.25)).abs() < 1e-9);
        assert!(estimate_cost_without_cache(Agent::Claude, "unknown-model", T, call).is_none());
    }

    #[test]
    fn codex_cached_input_is_not_charged_as_uncached_again() {
        let cost = estimate_cost(
            Agent::Codex,
            "gpt-5.3-codex",
            T,
            usage(600_000, 400_000, 0, 200_000),
        )
        .unwrap();
        assert!((cost - (1.05 + 0.07 + 2.8)).abs() < 1e-9);
    }

    #[test]
    fn codex_long_context_multiplier_is_applied_when_observable() {
        let cost =
            estimate_cost(Agent::Codex, "gpt-5.4", T, usage(300_000, 0, 0, 100_000)).unwrap();
        assert!((cost - 3.75).abs() < 1e-9);
    }

    #[test]
    fn mini_and_nano_do_not_get_a_long_context_surcharge() {
        let mini = estimate_cost(
            Agent::Codex,
            "gpt-5.4-mini",
            T,
            usage(300_000, 0, 0, 100_000),
        )
        .unwrap();
        let nano = estimate_cost(
            Agent::Codex,
            "gpt-5.4-nano",
            T,
            usage(300_000, 0, 0, 100_000),
        )
        .unwrap();
        assert!((mini - 0.675).abs() < 1e-9);
        assert!((nano - 0.185).abs() < 1e-9);
    }

    #[test]
    fn newly_supported_official_codex_rates_are_priced() {
        let gpt_55 =
            estimate_cost(Agent::Codex, "gpt-5.5", T, usage(300_000, 0, 0, 100_000)).unwrap();
        let codex_max = estimate_cost(
            Agent::Codex,
            "gpt-5.1-codex-max",
            T,
            usage(1_000_000, 0, 0, 1_000_000),
        )
        .unwrap();
        let o4_mini = estimate_cost(
            Agent::Codex,
            "o4-mini",
            T,
            usage(1_000_000, 0, 0, 1_000_000),
        )
        .unwrap();
        let gpt_41 = estimate_cost(
            Agent::Codex,
            "gpt-4.1",
            T,
            usage(1_000_000, 0, 0, 1_000_000),
        )
        .unwrap();
        assert!((gpt_55 - 7.5).abs() < 1e-9);
        assert!((codex_max - 11.25).abs() < 1e-9);
        assert!((o4_mini - 5.5).abs() < 1e-9);
        assert!((gpt_41 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn codex_snapshots_require_an_exact_supported_date_suffix() {
        for model in [
            "gpt-5.1-codex-max",
            "gpt-5.1-codex-max-20260717",
            "gpt-5.1-codex-max-2026-07-17",
        ] {
            assert!(estimate_cost(Agent::Codex, model, T, usage(100, 0, 0, 100)).is_some());
        }

        for model in [
            "gpt-5.1-codex-max-preview",
            "gpt-5.1-codex-max-202607",
            "gpt-5.1-codex-max-2026717",
            "gpt-5.1-codex-max-2026-7-17",
            "gpt-5.1-codex-max-2026-02-30",
            "gpt-5.1-codex-max-20260717-preview",
        ] {
            assert!(estimate_cost(Agent::Codex, model, T, usage(100, 0, 0, 100)).is_none());
        }
    }

    #[test]
    fn ambiguous_or_unknown_models_are_not_priced() {
        assert!(estimate_cost(Agent::Codex, "gpt-5.6-sol", T, usage(100, 0, 0, 100)).is_none());
        assert!(estimate_cost(Agent::Claude, "unknown-model", T, usage(100, 0, 0, 100)).is_none());
    }

    /// Noon UTC on the given day, in Unix milliseconds.
    fn day(date: &str) -> i64 {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis()
    }

    fn gemini_cost(model: &str, date: &str, call: NormalizedUsage) -> Option<f64> {
        estimate_cost(Agent::Antigravity, model, day(date), call)
    }

    #[test]
    fn gemini_rates_have_valid_ordered_since_days() {
        for rate in GEMINI_RATES {
            if let Some(since) = rate.since {
                assert!(
                    NaiveDate::parse_from_str(since, "%Y-%m-%d").is_ok(),
                    "{}: {since}",
                    rate.model
                );
            }
        }
        // Rows of one model are listed oldest first, without duplicate days.
        for (index, rate) in GEMINI_RATES.iter().enumerate() {
            for later in &GEMINI_RATES[index + 1..] {
                if later.model == rate.model {
                    assert!(later.since > rate.since, "{} rows out of order", rate.model);
                }
            }
        }
    }

    #[test]
    fn antigravity_effort_suffix_is_stripped_before_lookup() {
        let call = usage(1_000_000, 1_000_000, 0, 1_000_000);
        // 3.00 input + 0.15 cached + 15.00 output
        for model in ["gemini-4-neon", "gemini-4-neon-xhigh", "gemini-4-neon-low"] {
            let cost = gemini_cost(model, "2026-11-25", call).unwrap();
            assert!((cost - 18.15).abs() < 1e-9, "{model}");
        }
        let flash = gemini_cost("gemini-3.8-flash-high", "2026-11-20", call).unwrap();
        assert!((flash - (0.75 + 0.075 + 3.75)).abs() < 1e-9);
    }

    #[test]
    fn gemini_calls_are_priced_at_the_rate_of_their_day() {
        let call = usage(1_000_000, 0, 0, 1_000_000);
        let carbon = |date| gemini_cost("gemini-4-carbon", date, call).unwrap();
        assert!((carbon("2026-11-30") - 12.0).abs() < 1e-9);
        assert!((carbon("2026-12-01") - 24.0).abs() < 1e-9);
        assert!((gemini_cost("gemini-4-argon", "2026-12-15", call).unwrap() - 24.0).abs() < 1e-9);

        let flash = |date| gemini_cost("gemini-3.8-flash", date, call).unwrap();
        assert!((flash("2026-12-31") - 4.5).abs() < 1e-9);
        assert!((flash("2027-01-01") - 9.0).abs() < 1e-9);

        // gemini-2.5-pro is billed at gemini-3.8-flash rates once requests route there.
        let small = usage(100_000, 0, 0, 100_000);
        let pro = |date| gemini_cost("gemini-2.5-pro", date, small).unwrap();
        assert!((pro("2026-11-30") - (0.125 + 1.0)).abs() < 1e-9);
        assert!((pro("2026-12-01") - (0.075 + 0.375)).abs() < 1e-9);
        assert!((pro("2027-01-02") - (0.15 + 0.75)).abs() < 1e-9);
    }

    #[test]
    fn gemini_pro_long_prompts_use_the_higher_tier() {
        let short = gemini_cost(
            "gemini-3.1-pro-preview",
            "2026-11-25",
            usage(100_000, 50_000, 0, 100_000),
        )
        .unwrap();
        assert!((short - (0.2 + 0.01 + 1.2)).abs() < 1e-9);

        // 250k prompt tokens: input and cached input at 2x, output at 1.5x
        let long_call = usage(150_000, 100_000, 0, 100_000);
        let long = gemini_cost("gemini-3.1-pro-preview", "2026-11-25", long_call).unwrap();
        assert!((long - (0.6 + 0.04 + 1.8)).abs() < 1e-9);
        let parts = estimate_cost_parts(
            Agent::Antigravity,
            "gemini-3.1-pro-preview",
            day("2026-11-25"),
            long_call,
        )
        .unwrap();
        assert!((parts.cache_read - 0.04).abs() < 1e-12);

        // Flash models have no prompt-size tier.
        let flash = gemini_cost("gemini-3.8-flash", "2026-11-25", long_call).unwrap();
        assert!((flash - (0.1125 + 0.0075 + 0.375)).abs() < 1e-9);
    }

    #[test]
    fn retired_or_other_vendor_models_under_antigravity_stay_unpriced() {
        let call = usage(100, 0, 0, 100);
        assert!(gemini_cost("gemini-3-pro-preview", "2026-01-10", call).is_none());
        assert!(gemini_cost("gemini-3-pro-preview", "2026-03-09", call).is_some());
        for model in [
            "gemini-3.7-flash",
            "gemini-3.5-flash",
            "gemini-2.0-flash",
            "gemini-2.5-flash-preview-09-2025",
            "gemini-4-neon-preview",
            "claude-sonnet-4-6",
        ] {
            assert!(gemini_cost(model, "2026-11-25", call).is_none(), "{model}");
        }
        assert!(gemini_cost("gemini-3.1-pro-preview-customtools", "2026-11-25", call).is_some());
    }

    #[test]
    fn claude_and_codex_rates_do_not_depend_on_the_call_day() {
        let call = usage(1_000, 1_000, 0, 1_000);
        for date in ["2024-01-01", "2026-11-25", "2030-01-01"] {
            assert_eq!(
                estimate_cost(Agent::Claude, "claude-sonnet-4-5", day(date), call),
                estimate_cost(Agent::Claude, "claude-sonnet-4-5", T, call)
            );
            assert_eq!(
                estimate_cost(Agent::Codex, "gpt-5.4", day(date), call),
                estimate_cost(Agent::Codex, "gpt-5.4", T, call)
            );
        }
    }
}
