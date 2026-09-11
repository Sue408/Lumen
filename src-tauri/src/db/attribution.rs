use std::collections::BTreeMap;

use serde::Serialize;

/// 某一层（密钥或模型）相对上期的花费变化。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mover {
    pub name: String,
    pub delta_cost: f64,
}

/// 缓存命中率相对上期的变化。命中率 = 缓存读取 / (缓存读取 + 缓存写入 + 输入)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheShift {
    pub from_rate: f64,
    pub to_rate: f64,
}

/// 一句话归因的结构化因子：总变化、贡献最大的层、缓存命中变化。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub delta_cost: f64,
    pub top_movers: Vec<Mover>,
    pub cache: Option<CacheShift>,
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// 组装归因。每期入参为 `(层名, 花费)` 与 `(缓存读取, 输入侧总量)`。
/// 变更幅度最大的层正负各取不超过 2 条；任一期缺输入侧 token 时不给缓存因子。
pub fn build_attribution(
    previous: &[(String, f64)],
    current: &[(String, f64)],
    previous_cache: (i64, i64),
    current_cache: (i64, i64),
) -> Attribution {
    let previous_total: f64 = previous.iter().map(|(_, cost)| cost).sum();
    let current_total: f64 = current.iter().map(|(_, cost)| cost).sum();

    let mut by_name: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (name, cost) in previous {
        by_name.entry(name.clone()).or_insert((0.0, 0.0)).0 = *cost;
    }
    for (name, cost) in current {
        by_name.entry(name.clone()).or_insert((0.0, 0.0)).1 = *cost;
    }
    let mut movers: Vec<Mover> = by_name
        .into_iter()
        .map(|(name, (prev, curr))| Mover {
            name,
            delta_cost: round2(curr - prev),
        })
        .filter(|mover| mover.delta_cost.abs() > 0.0)
        .collect();
    movers.sort_by(|a, b| {
        b.delta_cost
            .abs()
            .partial_cmp(&a.delta_cost.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut top_movers: Vec<Mover> = movers.iter().filter(|m| m.delta_cost > 0.0).take(2).cloned().collect();
    top_movers.extend(movers.iter().filter(|m| m.delta_cost < 0.0).take(2).cloned());

    let cache = if previous_cache.1 > 0 && current_cache.1 > 0 {
        Some(CacheShift {
            from_rate: round4(previous_cache.0 as f64 / previous_cache.1 as f64),
            to_rate: round4(current_cache.0 as f64 / current_cache.1 as f64),
        })
    } else {
        None
    };

    Attribution {
        delta_cost: round2(current_total - previous_total),
        top_movers,
        cache,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, f64)]) -> Vec<(String, f64)> {
        items.iter().map(|(name, cost)| (name.to_string(), *cost)).collect()
    }

    #[test]
    fn computes_delta_and_ranks_movers() {
        let previous = pairs(&[("a", 10.0), ("b", 5.0)]);
        let current = pairs(&[("a", 14.0), ("b", 3.0), ("c", 2.0)]);
        let result = build_attribution(&previous, &current, (0, 0), (0, 0));

        assert!((result.delta_cost - 4.0).abs() < 1e-9);
        assert_eq!(result.top_movers[0].name, "a");
        assert!((result.top_movers[0].delta_cost - 4.0).abs() < 1e-9);
        assert!(result
            .top_movers
            .iter()
            .any(|mover| mover.name == "b" && (mover.delta_cost + 2.0).abs() < 1e-9));
        assert!(result.cache.is_none());
    }

    #[test]
    fn caps_movers_to_two_per_direction() {
        let previous = pairs(&[("a", 0.0), ("b", 0.0), ("c", 0.0)]);
        let current = pairs(&[("a", 5.0), ("b", 4.0), ("c", 3.0)]);
        let result = build_attribution(&previous, &current, (0, 0), (0, 0));

        assert_eq!(result.top_movers.len(), 2);
        assert_eq!(result.top_movers[0].name, "a");
        assert_eq!(result.top_movers[1].name, "b");
    }

    #[test]
    fn reports_cache_shift_when_both_periods_have_tokens() {
        let result = build_attribution(&[], &[], (62, 100), (51, 100));
        let cache = result.cache.expect("应有缓存因子");
        assert!((cache.from_rate - 0.62).abs() < 1e-9);
        assert!((cache.to_rate - 0.51).abs() < 1e-9);
    }

    #[test]
    fn omits_cache_when_a_period_has_no_tokens() {
        assert!(build_attribution(&[], &[], (0, 0), (51, 100)).cache.is_none());
        assert!(build_attribution(&[], &[], (51, 100), (0, 0)).cache.is_none());
    }
}
