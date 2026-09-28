//! Detail-view usage: price each call the transcript carries, flag the calls another session
//! owns (fork/resume copies), and split the session's attributed calls by call and by model.

use crate::models::{
    ConversationDetail, CostParts, NormalizedUsage, SessionModelUsage, UsageBreakdown, UsageCall,
};
use crate::parser::UsageEntry;
use crate::pricing;
use std::collections::{BTreeMap, HashSet};

#[derive(Default)]
struct ModelAggregate {
    calls: usize,
    usage: NormalizedUsage,
    cost: f64,
    no_cache_cost: f64,
    unpriced: bool,
}

/// Fill `detail.usage_breakdown`, and each message's `usage.est_cost_usd` / `attributed`, from
/// the calls the index attributes to the session (`AppIndex::session_calls`). Token sums over
/// the breakdown equal `detail.usage`; costs cover known-price models only, as everywhere else.
pub fn attach_usage_breakdown(detail: &mut ConversationDetail, calls: &[(&UsageEntry, bool)]) {
    let agent = detail.agent;
    let owned: HashSet<&str> = calls
        .iter()
        .map(|(entry, _)| entry.dedup_key.as_str())
        .collect();

    let mut unattributed_calls = 0usize;
    for message in &mut detail.messages {
        let Some(usage) = message.usage.as_mut() else {
            continue;
        };
        usage.est_cost_usd = pricing::estimate_cost(agent, &usage.model, usage.usage);
        usage.attributed = owned.contains(usage.call_key.as_str());
        if !usage.attributed {
            unattributed_calls += 1;
        }
    }

    let mut by_model: BTreeMap<&str, ModelAggregate> = BTreeMap::new();
    let mut cost_parts = CostParts::default();
    let mut est_cost_no_cache_usd = 0.0;
    let mut timeline = Vec::with_capacity(calls.len());
    for (entry, subagent) in calls {
        let parts = pricing::estimate_cost_parts(agent, &entry.model, entry.usage);
        let no_cache = pricing::estimate_cost_without_cache(agent, &entry.model, entry.usage);
        let aggregate = by_model.entry(entry.model.as_str()).or_default();
        aggregate.calls += 1;
        aggregate.usage.add_assign(entry.usage);
        match (parts, no_cache) {
            (Some(parts), Some(no_cache)) => {
                cost_parts.add_assign(parts);
                est_cost_no_cache_usd += no_cache;
                aggregate.cost += parts.total();
                aggregate.no_cache_cost += no_cache;
            }
            _ => aggregate.unpriced = true,
        }
        timeline.push(UsageCall {
            call_key: entry.dedup_key.clone(),
            timestamp: entry.timestamp,
            model: entry.model.clone(),
            usage: entry.usage,
            est_cost_usd: parts.map(CostParts::total),
            subagent: *subagent,
        });
    }

    let mut by_model: Vec<SessionModelUsage> = by_model
        .into_iter()
        .map(|(model, aggregate)| SessionModelUsage {
            model: model.to_string(),
            calls: aggregate.calls,
            usage: aggregate.usage,
            total_tokens_including_cache: aggregate.usage.total_tokens_including_cache(),
            est_cost_usd: (!aggregate.unpriced).then_some(aggregate.cost),
            est_cost_no_cache_usd: (!aggregate.unpriced).then_some(aggregate.no_cache_cost),
        })
        .collect();
    by_model.sort_by(|left, right| {
        right
            .total_tokens_including_cache
            .cmp(&left.total_tokens_including_cache)
            .then_with(|| left.model.cmp(&right.model))
    });

    detail.usage_breakdown = UsageBreakdown {
        calls: timeline,
        by_model,
        cost_parts,
        est_cost_no_cache_usd,
        unattributed_calls,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Agent, ChatMessage, ContentBlock, MessageUsage, SessionUsage};

    fn usage(uncached: u64, cache_read: u64, output: u64) -> NormalizedUsage {
        NormalizedUsage {
            uncached_input: uncached,
            cache_read,
            cache_creation: 0,
            cache_creation_1h: 0,
            output,
            reasoning_output: 0,
        }
    }

    fn entry(key: &str, model: &str, timestamp: i64, usage: NormalizedUsage) -> UsageEntry {
        UsageEntry {
            agent: Agent::Claude,
            dedup_key: key.to_string(),
            model: model.to_string(),
            timestamp,
            project: "/synthetic".to_string(),
            usage,
        }
    }

    fn message(role: &str, usage: Option<MessageUsage>) -> ChatMessage {
        ChatMessage {
            agent: Agent::Claude,
            uuid: String::new(),
            role: role.to_string(),
            timestamp: 0,
            is_sidechain: false,
            blocks: vec![ContentBlock {
                kind: "text".to_string(),
                text: Some("x".to_string()),
                tool_name: None,
                tool_input: None,
                truncated: false,
                persisted_output: None,
            }],
            usage,
        }
    }

    fn carried(key: &str, model: &str, usage: NormalizedUsage) -> Option<MessageUsage> {
        Some(MessageUsage {
            call_key: key.to_string(),
            model: model.to_string(),
            usage,
            est_cost_usd: None,
            attributed: true,
        })
    }

    fn detail(messages: Vec<ChatMessage>) -> ConversationDetail {
        ConversationDetail {
            agent: Agent::Claude,
            session_id: "s".to_string(),
            project: "/synthetic".to_string(),
            git_branch: None,
            started_at: 0,
            ended_at: 0,
            cli_version: None,
            source: None,
            models: Vec::new(),
            messages,
            usage: SessionUsage::default(),
            usage_breakdown: UsageBreakdown::default(),
        }
    }

    #[test]
    fn prices_messages_flags_copies_and_splits_owned_calls_by_model() {
        const SONNET: &str = "claude-sonnet-4-5-20250929";
        let a = usage(1_000, 9_000, 100);
        let b = usage(500, 0, 50);
        let c = usage(2_000, 0, 10);
        let d = usage(300, 700, 20);
        let mut detail = detail(vec![
            message("user", None),
            message("assistant", carried("a", SONNET, a)),
            message("assistant", carried("b", SONNET, b)),
            message("assistant", carried("c", "mystery-model", c)),
            message("assistant", None),
        ]);
        let index_calls = [
            entry("a", SONNET, 10, a),
            entry("c", "mystery-model", 20, c),
            entry("d", SONNET, 30, d),
        ];
        let calls: Vec<(&UsageEntry, bool)> = vec![
            (&index_calls[0], false),
            (&index_calls[1], false),
            (&index_calls[2], true),
        ];
        attach_usage_breakdown(&mut detail, &calls);

        let carried = |index: usize| detail.messages[index].usage.as_ref().unwrap();
        assert!(carried(1).attributed);
        assert!(carried(1).est_cost_usd.unwrap() > 0.0);
        assert!(!carried(2).attributed, "b belongs to another session");
        assert!(
            carried(2).est_cost_usd.is_some(),
            "copies are still priced for display"
        );
        assert!(carried(3).attributed);
        assert!(
            carried(3).est_cost_usd.is_none(),
            "unknown model stays unpriced"
        );
        assert!(detail.messages[4].usage.is_none());

        let breakdown = &detail.usage_breakdown;
        assert_eq!(breakdown.unattributed_calls, 1);
        assert_eq!(
            breakdown
                .calls
                .iter()
                .map(|call| (call.call_key.as_str(), call.subagent))
                .collect::<Vec<_>>(),
            vec![("a", false), ("c", false), ("d", true)]
        );
        assert_eq!(breakdown.by_model.len(), 2);
        let sonnet = &breakdown.by_model[0];
        assert_eq!(sonnet.model, SONNET, "largest model first");
        assert_eq!(sonnet.calls, 2);
        assert_eq!(sonnet.usage.uncached_input, 1_300);
        assert_eq!(sonnet.usage.cache_read, 9_700);
        assert_eq!(sonnet.total_tokens_including_cache, 11_120);
        let mystery = &breakdown.by_model[1];
        assert_eq!(mystery.calls, 1);
        assert!(mystery.est_cost_usd.is_none());
        assert!(mystery.est_cost_no_cache_usd.is_none());

        let token_sum: u64 = breakdown
            .calls
            .iter()
            .map(|call| call.usage.total_tokens_including_cache())
            .sum();
        let model_sum: u64 = breakdown
            .by_model
            .iter()
            .map(|row| row.total_tokens_including_cache)
            .sum();
        assert_eq!(token_sum, model_sum);
        assert_eq!(token_sum, 11_120 + 2_010);

        let priced: f64 = breakdown
            .calls
            .iter()
            .filter_map(|call| call.est_cost_usd)
            .sum();
        assert!((breakdown.cost_parts.total() - priced).abs() < 1e-12);
        assert!((sonnet.est_cost_usd.unwrap() - priced).abs() < 1e-12);
        assert!(
            breakdown.est_cost_no_cache_usd > priced,
            "9.7k cached reads at a tenth of the price save money"
        );
        assert!(
            (sonnet.est_cost_no_cache_usd.unwrap() - breakdown.est_cost_no_cache_usd).abs() < 1e-12
        );
    }

    #[test]
    fn no_owned_calls_leaves_an_empty_breakdown_and_marks_every_call_foreign() {
        let mut detail = detail(vec![message(
            "assistant",
            carried("x", "claude-sonnet-4-5", usage(10, 0, 5)),
        )]);
        attach_usage_breakdown(&mut detail, &[]);
        assert!(!detail.messages[0].usage.as_ref().unwrap().attributed);
        assert_eq!(detail.usage_breakdown.unattributed_calls, 1);
        assert!(detail.usage_breakdown.calls.is_empty());
        assert!(detail.usage_breakdown.by_model.is_empty());
        assert_eq!(detail.usage_breakdown.est_cost_no_cache_usd, 0.0);
    }
}
