use chrono::{DateTime, Datelike, Duration, Local, Timelike, Utc};
use rusqlite::Connection;
use serde::Serialize;

use super::keys::save_virtual_key;
use super::logs::insert_log;
use super::models::{
    ProviderEndpointInput, ProviderInput, RequestLog, RouteInput, RouteTargetInput,
    UpstreamModelInput, VirtualKeyInput,
};
use super::{clear_business_data, providers, routes};
use crate::error::AppError;
use crate::gateway::usage::{calculate_cost, Pricing, UsageSource, UsageTotals};

/// 日志时间跨度；覆盖本月 + 上月同期 + 本周 + 上周。
const DAYS: i64 = 45;

/// 各时段相对权重（0–23），白天高、凌晨低，让日内曲线有形状。
const HOUR_WEIGHTS: [u32; 24] = [
    1, 1, 1, 1, 2, 2, 3, 6, 12, 18, 20, 16, 10, 12, 20, 22, 18, 14, 12, 10, 8, 6, 4, 2,
];

const ERROR_POOL: [&str; 5] = [
    "上游超时",
    "上游返回 429，请求过于频繁",
    "上游鉴权失败（401）",
    "连接被重置",
    "上游网关超时（504）",
];

const ERROR_STATUS: [i64; 4] = [500, 429, 401, 504];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoScenario {
    Rich,
    Empty,
    Errors,
    Unassigned,
    Spike,
    Solo,
}

impl DemoScenario {
    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "rich" => Ok(Self::Rich),
            "empty" => Ok(Self::Empty),
            "errors" => Ok(Self::Errors),
            "unassigned" => Ok(Self::Unassigned),
            "spike" => Ok(Self::Spike),
            "solo" => Ok(Self::Solo),
            other => Err(AppError::message(format!("未知演示场景：{other}"))),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoSummary {
    pub providers: usize,
    pub models: usize,
    pub routes: usize,
    pub virtual_keys: usize,
    pub logs: usize,
}

struct ProviderSpec {
    id: &'static str,
    name: &'static str,
    base_url: &'static str,
    api_key: &'static str,
    auth_scheme: &'static str,
    protocol: &'static str,
    icon: &'static str,
    icon_tint: &'static str,
    enabled: bool,
}

struct ModelSpec {
    id: &'static str,
    provider_id: &'static str,
    model_id: &'static str,
    display_name: &'static str,
    input_price: f64,
    output_price: f64,
    cache_read_price: f64,
    cache_creation_price: f64,
    context_window: i64,
    capabilities: &'static [&'static str],
    icon: &'static str,
    enabled: bool,
    /// OpenAI / DeepSeek 语义：输入已包含缓存读取部分。
    contains_cache_read: bool,
    /// 命中占输入侧的比例。Anthropic 语义下即为显示的命中率。
    cache_hit_ratio: f64,
    /// 缓存写入占输入侧的比例；OpenAI 语义恒为 0。
    cache_creation_ratio: f64,
    /// 单次调用输入侧 token 的典型区间。
    input_lo: i64,
    input_hi: i64,
}

struct RouteSpec {
    id: &'static str,
    alias: &'static str,
    display_name: &'static str,
    protocol: &'static str,
    enabled: bool,
    primary: &'static str,
    backup: Option<&'static str>,
}

struct KeySpec {
    id: &'static str,
    key: &'static str,
    name: &'static str,
    enabled: bool,
    quota_limit: Option<f64>,
    quota_period: &'static str,
}

struct ChannelSpec {
    key_id: Option<&'static str>,
    route_id: &'static str,
    weight: u32,
}

struct DemoSet {
    providers: &'static [ProviderSpec],
    models: &'static [ModelSpec],
    routes: &'static [RouteSpec],
    keys: &'static [KeySpec],
    channels: &'static [ChannelSpec],
}

struct Profile {
    error_rate: f64,
    missing_rate: f64,
    partial_rate: f64,
    unassigned_multiplier: u32,
    recent_days: i64,
    recent_multiplier: f64,
    spike_days: i64,
    spike_multiplier: f64,
}

impl Profile {
    fn rich() -> Self {
        Self {
            error_rate: 0.03,
            missing_rate: 0.02,
            partial_rate: 0.02,
            unassigned_multiplier: 1,
            recent_days: 7,
            recent_multiplier: 1.25,
            spike_days: 0,
            spike_multiplier: 1.0,
        }
    }

    fn errors() -> Self {
        Self {
            error_rate: 0.22,
            missing_rate: 0.05,
            partial_rate: 0.03,
            ..Self::rich()
        }
    }

    fn unassigned() -> Self {
        Self {
            unassigned_multiplier: 6,
            ..Self::rich()
        }
    }

    fn spike() -> Self {
        Self {
            spike_days: 3,
            spike_multiplier: 3.5,
            ..Self::rich()
        }
    }
}

// ---------------------------------------------------------------------------
// 丰富场景（rich / errors / unassigned / spike 共用配置）
// ---------------------------------------------------------------------------

static RICH_PROVIDERS: [ProviderSpec; 4] = [
    ProviderSpec {
        id: "demo-provider-anthropic",
        name: "Anthropic",
        base_url: "https://api.anthropic.com/v1",
        api_key: "sk-ant-demo-000000000000",
        auth_scheme: "x-api-key",
        protocol: "anthropic",
        icon: "anthropic",
        icon_tint: "brand",
        enabled: true,
    },
    ProviderSpec {
        id: "demo-provider-deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com/anthropic/v1",
        api_key: "sk-demo-deepseek-000000000000",
        auth_scheme: "x-api-key",
        protocol: "anthropic",
        icon: "deepseek",
        icon_tint: "brand",
        enabled: true,
    },
    ProviderSpec {
        id: "demo-provider-openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        api_key: "sk-demo-openai-000000000000",
        auth_scheme: "bearer",
        protocol: "openai",
        icon: "openai",
        icon_tint: "brand",
        enabled: true,
    },
    ProviderSpec {
        id: "demo-provider-ollama",
        name: "Ollama",
        base_url: "http://127.0.0.1:11434/v1",
        api_key: "",
        auth_scheme: "bearer",
        protocol: "openai",
        icon: "ollama",
        icon_tint: "ink",
        enabled: false,
    },
];

static RICH_MODELS: [ModelSpec; 7] = [
    ModelSpec {
        id: "demo-model-sonnet",
        provider_id: "demo-provider-anthropic",
        model_id: "claude-sonnet-4-5",
        display_name: "Claude Sonnet 4.5",
        input_price: 3.0,
        output_price: 15.0,
        cache_read_price: 0.3,
        cache_creation_price: 3.75,
        context_window: 200_000,
        capabilities: &["vision", "tools"],
        icon: "anthropic",
        enabled: true,
        contains_cache_read: false,
        cache_hit_ratio: 0.89,
        cache_creation_ratio: 0.05,
        input_lo: 20_000,
        input_hi: 90_000,
    },
    ModelSpec {
        id: "demo-model-haiku",
        provider_id: "demo-provider-anthropic",
        model_id: "claude-haiku-4-5",
        display_name: "Claude Haiku 4.5",
        input_price: 1.0,
        output_price: 5.0,
        cache_read_price: 0.1,
        cache_creation_price: 1.25,
        context_window: 200_000,
        capabilities: &["vision", "tools"],
        icon: "anthropic",
        enabled: true,
        contains_cache_read: false,
        cache_hit_ratio: 0.86,
        cache_creation_ratio: 0.05,
        input_lo: 12_000,
        input_hi: 50_000,
    },
    ModelSpec {
        id: "demo-model-flash",
        provider_id: "demo-provider-deepseek",
        model_id: "deepseek-v4-flash",
        display_name: "DeepSeek V4 Flash",
        input_price: 0.15,
        output_price: 0.6,
        cache_read_price: 0.015,
        cache_creation_price: 0.15,
        context_window: 128_000,
        capabilities: &["tools"],
        icon: "deepseek",
        enabled: true,
        contains_cache_read: false,
        cache_hit_ratio: 0.95,
        cache_creation_ratio: 0.04,
        input_lo: 40_000,
        input_hi: 110_000,
    },
    ModelSpec {
        id: "demo-model-reason",
        provider_id: "demo-provider-deepseek",
        model_id: "deepseek-v4-reasoner",
        display_name: "DeepSeek V4 Reasoner",
        input_price: 0.55,
        output_price: 2.19,
        cache_read_price: 0.14,
        cache_creation_price: 0.55,
        context_window: 128_000,
        capabilities: &["reasoning"],
        icon: "deepseek",
        enabled: true,
        contains_cache_read: false,
        cache_hit_ratio: 0.94,
        cache_creation_ratio: 0.04,
        input_lo: 40_000,
        input_hi: 100_000,
    },
    ModelSpec {
        id: "demo-model-gpt5",
        provider_id: "demo-provider-openai",
        model_id: "gpt-5",
        display_name: "GPT-5",
        input_price: 1.25,
        output_price: 10.0,
        cache_read_price: 0.125,
        cache_creation_price: 0.0,
        context_window: 400_000,
        capabilities: &["vision", "tools", "reasoning"],
        icon: "openai",
        enabled: true,
        contains_cache_read: true,
        cache_hit_ratio: 0.90,
        cache_creation_ratio: 0.0,
        input_lo: 20_000,
        input_hi: 90_000,
    },
    ModelSpec {
        id: "demo-model-mini",
        provider_id: "demo-provider-openai",
        model_id: "gpt-4o-mini",
        display_name: "GPT-4o mini",
        input_price: 0.15,
        output_price: 0.6,
        cache_read_price: 0.075,
        cache_creation_price: 0.0,
        context_window: 128_000,
        capabilities: &["vision", "tools"],
        icon: "openai",
        enabled: true,
        contains_cache_read: true,
        cache_hit_ratio: 0.85,
        cache_creation_ratio: 0.0,
        input_lo: 12_000,
        input_hi: 50_000,
    },
    ModelSpec {
        id: "demo-model-qwen",
        provider_id: "demo-provider-ollama",
        model_id: "qwen2.5:14b",
        display_name: "Qwen2.5 14B",
        input_price: 0.0,
        output_price: 0.0,
        cache_read_price: 0.0,
        cache_creation_price: 0.0,
        context_window: 32_768,
        capabilities: &["tools"],
        icon: "qwen",
        enabled: false,
        contains_cache_read: false,
        cache_hit_ratio: 0.0,
        cache_creation_ratio: 0.0,
        input_lo: 0,
        input_hi: 0,
    },
];

static RICH_ROUTES: [RouteSpec; 4] = [
    RouteSpec {
        id: "demo-route-gpt",
        alias: "openai/gpt-5",
        display_name: "GPT-5",
        protocol: "openai",
        enabled: true,
        primary: "demo-model-gpt5",
        backup: Some("demo-model-mini"),
    },
    RouteSpec {
        id: "demo-route-claude",
        alias: "anthropic/claude-sonnet",
        display_name: "Claude Sonnet",
        protocol: "anthropic",
        enabled: true,
        primary: "demo-model-sonnet",
        backup: Some("demo-model-haiku"),
    },
    RouteSpec {
        id: "demo-route-deepseek",
        alias: "deepseek/deepseek-v4-flash",
        display_name: "DeepSeek V4 Flash",
        protocol: "anthropic",
        enabled: true,
        primary: "demo-model-flash",
        backup: Some("demo-model-reason"),
    },
    RouteSpec {
        id: "demo-route-reason",
        alias: "deepseek/deepseek-v4-reasoner",
        display_name: "DeepSeek V4 Reasoner",
        protocol: "anthropic",
        enabled: false,
        primary: "demo-model-reason",
        backup: None,
    },
];

static RICH_KEYS: [KeySpec; 5] = [
    KeySpec {
        id: "demo-key-desktop",
        key: "sk-lumen-demo-desktop",
        name: "Claude 桌面端",
        enabled: true,
        quota_limit: Some(250.0),
        quota_period: "monthly",
    },
    KeySpec {
        id: "demo-key-mobile",
        key: "sk-lumen-demo-mobile",
        name: "移动端",
        enabled: true,
        quota_limit: Some(25.0),
        quota_period: "weekly",
    },
    KeySpec {
        id: "demo-key-script",
        key: "sk-lumen-demo-script",
        name: "GPT-5 脚本",
        enabled: true,
        quota_limit: None,
        quota_period: "monthly",
    },
    KeySpec {
        id: "demo-key-sandbox",
        key: "sk-lumen-demo-sandbox",
        name: "实验沙盒",
        enabled: true,
        quota_limit: Some(5.0),
        quota_period: "monthly",
    },
    KeySpec {
        id: "demo-key-legacy",
        key: "sk-lumen-demo-legacy",
        name: "旧设备",
        enabled: false,
        quota_limit: Some(150.0),
        quota_period: "monthly",
    },
];

static RICH_CHANNELS: [ChannelSpec; 10] = [
    ChannelSpec { key_id: Some("demo-key-desktop"), route_id: "demo-route-claude", weight: 15 },
    ChannelSpec { key_id: Some("demo-key-mobile"), route_id: "demo-route-claude", weight: 5 },
    ChannelSpec { key_id: Some("demo-key-mobile"), route_id: "demo-route-deepseek", weight: 18 },
    ChannelSpec { key_id: Some("demo-key-script"), route_id: "demo-route-gpt", weight: 6 },
    ChannelSpec { key_id: Some("demo-key-sandbox"), route_id: "demo-route-reason", weight: 5 },
    ChannelSpec { key_id: Some("demo-key-sandbox"), route_id: "demo-route-deepseek", weight: 6 },
    ChannelSpec { key_id: Some("demo-key-legacy"), route_id: "demo-route-claude", weight: 3 },
    ChannelSpec { key_id: None, route_id: "demo-route-deepseek", weight: 27 },
    ChannelSpec { key_id: None, route_id: "demo-route-claude", weight: 7 },
    ChannelSpec { key_id: None, route_id: "demo-route-gpt", weight: 7 },
];

static RICH_SET: DemoSet = DemoSet {
    providers: &RICH_PROVIDERS,
    models: &RICH_MODELS,
    routes: &RICH_ROUTES,
    keys: &RICH_KEYS,
    channels: &RICH_CHANNELS,
};

// ---------------------------------------------------------------------------
// 单模型场景
// ---------------------------------------------------------------------------

static SOLO_PROVIDERS: [ProviderSpec; 1] = [ProviderSpec {
    id: "demo-provider-openai",
    name: "OpenAI",
    base_url: "https://api.openai.com/v1",
    api_key: "sk-demo-openai-000000000000",
    auth_scheme: "bearer",
    protocol: "openai",
    icon: "openai",
    icon_tint: "brand",
    enabled: true,
}];

static SOLO_MODELS: [ModelSpec; 1] = [ModelSpec {
    id: "demo-model-mini",
    provider_id: "demo-provider-openai",
    model_id: "gpt-4o-mini",
    display_name: "GPT-4o mini",
    input_price: 0.15,
    output_price: 0.6,
    cache_read_price: 0.075,
    cache_creation_price: 0.0,
    context_window: 128_000,
    capabilities: &["vision", "tools"],
    icon: "openai",
    enabled: true,
    contains_cache_read: true,
    cache_hit_ratio: 0.85,
    cache_creation_ratio: 0.0,
    input_lo: 8_000,
    input_hi: 40_000,
}];

static SOLO_ROUTES: [RouteSpec; 1] = [RouteSpec {
    id: "demo-route-mini",
    alias: "lumen/mini",
    display_name: "Lumen 小模型",
    protocol: "openai",
    enabled: true,
    primary: "demo-model-mini",
    backup: None,
}];

static SOLO_KEYS: [KeySpec; 1] = [KeySpec {
    id: "demo-key-solo",
    key: "sk-lumen-demo-solo",
    name: "示例密钥",
    enabled: true,
    quota_limit: Some(20.0),
    quota_period: "monthly",
}];

static SOLO_CHANNELS: [ChannelSpec; 1] = [ChannelSpec {
    key_id: Some("demo-key-solo"),
    route_id: "demo-route-mini",
    weight: 100,
}];

static SOLO_SET: DemoSet = DemoSet {
    providers: &SOLO_PROVIDERS,
    models: &SOLO_MODELS,
    routes: &SOLO_ROUTES,
    keys: &SOLO_KEYS,
    channels: &SOLO_CHANNELS,
};

// ---------------------------------------------------------------------------
// 注入
// ---------------------------------------------------------------------------

/// 完全替换：清空全部业务数据后写入演示配置与流水。`settings` 保留。
pub fn inject_demo(conn: &Connection, scenario: DemoScenario) -> Result<DemoSummary, AppError> {
    clear_business_data(conn)?;
    if scenario == DemoScenario::Empty {
        return Ok(DemoSummary::default());
    }

    let (set, profile) = match scenario {
        DemoScenario::Rich => (&RICH_SET, Profile::rich()),
        DemoScenario::Errors => (&RICH_SET, Profile::errors()),
        DemoScenario::Unassigned => (&RICH_SET, Profile::unassigned()),
        DemoScenario::Spike => (&RICH_SET, Profile::spike()),
        DemoScenario::Solo => (&SOLO_SET, Profile::rich()),
        DemoScenario::Empty => unreachable!("empty 已提前返回"),
    };

    let mut summary = write_config(conn, set)?;
    let logs = generate_logs(set, &profile, Local::now());
    // 数万条流水必须包在单事务里，否则逐条 autocommit 会慢到不可用。
    {
        let tx = conn.unchecked_transaction()?;
        for log in &logs {
            insert_log(&tx, log)?;
        }
        tx.commit()?;
    }
    summary.logs = logs.len();
    Ok(summary)
}

fn write_config(conn: &Connection, set: &DemoSet) -> Result<DemoSummary, AppError> {
    for provider in set.providers {
        providers::save_provider(
            conn,
            &ProviderInput {
                id: Some(provider.id.to_string()),
                name: provider.name.to_string(),
                api_key: provider.api_key.to_string(),
                endpoints: vec![ProviderEndpointInput {
                    id: None,
                    protocol: provider.protocol.to_string(),
                    base_url: provider.base_url.to_string(),
                    auth_scheme: provider.auth_scheme.to_string(),
                }],
                extra_headers: Default::default(),
                header_rules: Default::default(),
                icon: Some(provider.icon.to_string()),
                icon_tint: provider.icon_tint.to_string(),
                enabled: provider.enabled,
            },
        )?;
    }
    for model in set.models {
        providers::save_upstream_model(
            conn,
            &UpstreamModelInput {
                id: Some(model.id.to_string()),
                provider_id: model.provider_id.to_string(),
                model_id: model.model_id.to_string(),
                display_name: model.display_name.to_string(),
                input_price: model.input_price,
                output_price: model.output_price,
                cache_read_price: model.cache_read_price,
                cache_creation_price: model.cache_creation_price,
                context_window: model.context_window,
                capabilities: model.capabilities.iter().map(|item| item.to_string()).collect(),
                icon: Some(model.icon.to_string()),
                icon_tint: "brand".to_string(),
                enabled: model.enabled,
            },
        )?;
    }
    for route in set.routes {
        let mut targets = vec![RouteTargetInput {
            upstream_model_id: route.primary.to_string(),
            priority: 0,
            enabled: true,
        }];
        if let Some(backup) = route.backup {
            targets.push(RouteTargetInput {
                upstream_model_id: backup.to_string(),
                priority: 1,
                enabled: true,
            });
        }
        routes::save_route(
            conn,
            &RouteInput {
                id: Some(route.id.to_string()),
                alias: route.alias.to_string(),
                display_name: route.display_name.to_string(),
                protocol: route.protocol.to_string(),
                icon: None,
                icon_tint: None,
                enabled: route.enabled,
                targets,
            },
        )?;
    }
    for key in set.keys {
        save_virtual_key(
            conn,
            &VirtualKeyInput {
                id: Some(key.id.to_string()),
                key: Some(key.key.to_string()),
                name: key.name.to_string(),
                enabled: key.enabled,
                quota_limit: key.quota_limit,
                quota_period: key.quota_period.to_string(),
            },
        )?;
    }

    Ok(DemoSummary {
        providers: set.providers.len(),
        models: set.models.len(),
        routes: set.routes.len(),
        virtual_keys: set.keys.len(),
        logs: 0,
    })
}

// ---------------------------------------------------------------------------
// 流水生成（纯函数，确定性）
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        (self.next() % u64::from(bound)) as u32
    }

    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo + i64::from(self.below((hi - lo + 1) as u32))
    }

    fn ratio(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn chance(&mut self, probability: f64) -> bool {
        self.ratio() < probability
    }
}

fn find_model<'a>(set: &'a DemoSet, id: &str) -> &'a ModelSpec {
    set.models
        .iter()
        .find(|model| model.id == id)
        .expect("演示模型必然存在")
}

fn find_route<'a>(set: &'a DemoSet, id: &str) -> &'a RouteSpec {
    set.routes
        .iter()
        .find(|route| route.id == id)
        .expect("演示路由必然存在")
}

fn channel_weight(channel: &ChannelSpec, profile: &Profile) -> u32 {
    if channel.key_id.is_none() {
        channel.weight * profile.unassigned_multiplier
    } else {
        channel.weight
    }
}

fn pick_channel<'a>(rng: &mut Rng, set: &'a DemoSet, profile: &Profile) -> &'a ChannelSpec {
    let total: u32 = set
        .channels
        .iter()
        .map(|channel| channel_weight(channel, profile))
        .sum();
    let mut pick = rng.below(total);
    for channel in set.channels {
        let weight = channel_weight(channel, profile);
        if pick < weight {
            return channel;
        }
        pick -= weight;
    }
    &set.channels[set.channels.len() - 1]
}

fn pick_hour(rng: &mut Rng) -> u32 {
    let total: u32 = HOUR_WEIGHTS.iter().sum();
    let mut pick = rng.below(total);
    for (hour, weight) in HOUR_WEIGHTS.iter().enumerate() {
        if pick < *weight {
            return hour as u32;
        }
        pick -= *weight;
    }
    23
}

fn day_volume(rng: &mut Rng, day_offset: i64, weekday: u32, profile: &Profile) -> i64 {
    let mut base: i64 = if weekday < 5 { 1_450 } else { 900 };
    if day_offset < profile.recent_days {
        base = (base as f64 * profile.recent_multiplier).round() as i64;
    }
    if profile.spike_days > 0 && day_offset < profile.spike_days {
        base = (base as f64 * profile.spike_multiplier).round() as i64;
    }
    (base + rng.range(-150, 150)).max(0)
}

fn has_reasoning(model: &ModelSpec) -> bool {
    model.capabilities.contains(&"reasoning")
}

fn build_usage(rng: &mut Rng, model: &ModelSpec, source: UsageSource) -> UsageTotals {
    if source == UsageSource::Missing {
        return UsageTotals {
            source,
            ..UsageTotals::default()
        };
    }

    let output = rng.range(300, 3_500);
    let reasoning = if has_reasoning(model) {
        output * rng.range(250, 450) / 1000
    } else {
        0
    };

    if source == UsageSource::Partial {
        return UsageTotals {
            output_tokens: output,
            total_tokens: output,
            reasoning_tokens: reasoning,
            source,
            ..UsageTotals::default()
        };
    }

    // 输入侧总量受上下文窗口约束；命中与写入按模型画像从总量中拆分。
    let cap = (model.context_window * 85 / 100).max(1);
    let hi = model.input_hi.min(cap);
    let lo = model.input_lo.min(hi);
    let total_input = rng.range(lo, hi);
    let hit = (total_input as f64 * model.cache_hit_ratio).round() as i64;

    let (input_tokens, cache_read, cache_creation) = if model.contains_cache_read {
        (total_input, hit, 0)
    } else {
        let creation = (total_input as f64 * model.cache_creation_ratio).round() as i64;
        ((total_input - hit - creation).max(0), hit, creation)
    };
    let total = if model.contains_cache_read {
        input_tokens + output
    } else {
        total_input + output
    };

    UsageTotals {
        input_tokens,
        output_tokens: output,
        total_tokens: total,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        reasoning_tokens: reasoning,
        contains_cache_read: model.contains_cache_read,
        source,
    }
}

fn generate_logs(set: &DemoSet, profile: &Profile, now: DateTime<Local>) -> Vec<RequestLog> {
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
    let mut logs = Vec::new();
    let mut sequence: u64 = 0;

    for day_offset in 0..DAYS {
        let day = now - Duration::days(day_offset);
        let naive = day.date_naive();
        let weekday = naive.weekday().num_days_from_monday();
        let volume = day_volume(&mut rng, day_offset, weekday, profile);

        for _ in 0..volume {
            let channel = pick_channel(&mut rng, set, profile);
            let route = find_route(set, channel.route_id);
            let use_backup = route.backup.is_some() && rng.chance(0.15);
            let model = find_model(
                set,
                if use_backup {
                    route.backup.unwrap_or(route.primary)
                } else {
                    route.primary
                },
            );

            let roll = rng.ratio();
            let (status, source) = if roll < profile.error_rate {
                ("error", UsageSource::Missing)
            } else if roll < profile.error_rate + profile.missing_rate {
                ("success", UsageSource::Missing)
            } else if roll < profile.error_rate + profile.missing_rate + profile.partial_rate {
                ("success", UsageSource::Partial)
            } else {
                ("success", UsageSource::Provider)
            };

            let usage = build_usage(&mut rng, model, source);
            let pricing = Pricing {
                input: model.input_price,
                output: model.output_price,
                cache_read: model.cache_read_price,
                cache_creation: model.cache_creation_price,
            };
            let cost = if status == "error" {
                0.0
            } else {
                calculate_cost(&usage, &pricing)
            };

            let mut hour = pick_hour(&mut rng);
            if day_offset == 0 {
                hour = hour.min(now.hour());
            }
            let minute = rng.below(60);
            let occurred = naive
                .and_hms_opt(hour, minute, 0)
                .expect("时间合法")
                .and_local_timezone(Local)
                .earliest()
                .unwrap_or(day);

            let is_stream = rng.chance(0.4);
            let http_status = if status == "error" {
                Some(ERROR_STATUS[rng.below(ERROR_STATUS.len() as u32) as usize])
            } else {
                Some(200)
            };
            let error_message = if status == "error" {
                Some(ERROR_POOL[rng.below(ERROR_POOL.len() as u32) as usize].to_string())
            } else {
                None
            };
            let latency_ms = rng.range(200, 2_600) + if is_stream { 300 } else { 0 };
            // 流式记录给一个约三分之一的首字等待，供生成速度演示；非流式为 NULL。
            let ttfb_ms = is_stream.then_some(latency_ms / 3);

            logs.push(RequestLog {
                id: format!("demo-log-{sequence}"),
                occurred_at: occurred.with_timezone(&Utc).to_rfc3339(),
                endpoint: if route.protocol == "openai" {
                    "/v1/chat/completions".to_string()
                } else {
                    "/v1/messages".to_string()
                },
                method: "POST".to_string(),
                route_alias: Some(route.alias.to_string()),
                route_id: Some(route.id.to_string()),
                upstream_model_id: Some(model.id.to_string()),
                upstream_model_name: Some(model.display_name.to_string()),
                model_real: Some(model.model_id.to_string()),
                provider_id: Some(model.provider_id.to_string()),
                virtual_key_id: channel.key_id.map(str::to_string),
                kind: "chat".to_string(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
                cache_read_tokens: usage.cache_read_tokens,
                cache_creation_tokens: usage.cache_creation_tokens,
                cache_read_in_input: usage.contains_cache_read,
                reasoning_tokens: usage.reasoning_tokens,
                cost,
                usage_source: usage.source.as_str().to_string(),
                status: status.to_string(),
                http_status,
                latency_ms: Some(latency_ms),
                ttfb_ms,
                error_message,
                request_id: Some(format!("req_demo_{sequence}")),
                is_stream,
                attempt_index: 0,
                session_id: None,
            });
            sequence += 1;
        }
    }

    logs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        keys::list_virtual_keys, logs::count_logs, logs::list_logs, open_in_memory, providers,
        routes,
    };

    fn fixed_now() -> DateTime<Local> {
        chrono::NaiveDate::from_ymd_opt(2026, 9, 11)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap()
    }

    #[test]
    fn parses_known_scenarios_and_rejects_unknown() {
        assert_eq!(DemoScenario::parse("rich").unwrap(), DemoScenario::Rich);
        assert_eq!(DemoScenario::parse("solo").unwrap(), DemoScenario::Solo);
        assert!(DemoScenario::parse("nope").is_err());
    }

    #[test]
    fn generation_is_deterministic() {
        let first = generate_logs(&RICH_SET, &Profile::rich(), fixed_now());
        let second = generate_logs(&RICH_SET, &Profile::rich(), fixed_now());
        assert_eq!(first.len(), second.len());
        assert!(first.len() > 20_000, "应生成足量流水，实际 {}", first.len());
        assert_eq!(first[0].id, second[0].id);
        assert_eq!(first[0].total_tokens, second[0].total_tokens);
        assert_eq!(first[0].cost, second[0].cost);
    }

    #[test]
    fn rich_profile_matches_realistic_scale() {
        let logs = generate_logs(&RICH_SET, &Profile::rich(), fixed_now());
        let successful: Vec<&RequestLog> = logs
            .iter()
            .filter(|log| log.usage_source == "provider")
            .collect();
        let read: i64 = successful.iter().map(|log| log.cache_read_tokens).sum();
        let creation: i64 = successful.iter().map(|log| log.cache_creation_tokens).sum();
        let input: i64 = successful.iter().map(|log| log.input_tokens).sum();
        let hit = read as f64 / (read + creation + input) as f64;
        assert!(hit > 0.8, "整体命中率应高于 80%，实际 {hit}");

        let today = fixed_now().date_naive();
        let today_logs: Vec<&RequestLog> = logs
            .iter()
            .filter(|log| {
                DateTime::parse_from_rfc3339(&log.occurred_at)
                    .map(|moment| moment.with_timezone(&Local).date_naive() == today)
                    .unwrap_or(false)
            })
            .collect();
        let calls = today_logs.len();
        let tokens: i64 = today_logs.iter().map(|log| log.total_tokens).sum();
        assert!(calls > 800, "今日调用应超过 800，实际 {calls}");
        assert!(tokens > 20_000_000, "今日 token 应上千万，实际 {tokens}");
        println!("hit={hit:.3} today_calls={calls} today_tokens={tokens}");
    }

    #[test]
    fn rich_logs_cover_success_error_and_unreliable() {
        let logs = generate_logs(&RICH_SET, &Profile::rich(), fixed_now());
        assert!(logs.iter().any(|log| log.status == "error"));
        assert!(logs
            .iter()
            .any(|log| log.usage_source == "missing" || log.usage_source == "partial"));
        assert!(logs.iter().any(|log| log.status == "success" && log.cost > 0.0));
        assert!(logs.iter().any(|log| log.virtual_key_id.is_none()));
        assert!(logs.iter().all(|log| log.kind == "chat"));
    }

    #[test]
    fn inject_rich_populates_every_table() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        let summary = inject_demo(&conn, DemoScenario::Rich).unwrap();

        assert_eq!(summary.providers, 4);
        assert_eq!(summary.models, 7);
        assert_eq!(summary.routes, 4);
        assert_eq!(summary.virtual_keys, 5);
        assert!(summary.logs > 20_000);
        assert_eq!(providers::list_providers(&conn).unwrap().len(), 4);
        assert_eq!(providers::list_upstream_models(&conn).unwrap().len(), 7);
        assert_eq!(routes::list_routes(&conn).unwrap().len(), 4);
        assert_eq!(list_virtual_keys(&conn).unwrap().len(), 5);
        assert_eq!(count_logs(&conn, &Default::default()).unwrap() as usize, summary.logs);
    }

    #[test]
    fn inject_empty_clears_business_data() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        inject_demo(&conn, DemoScenario::Rich).unwrap();
        inject_demo(&conn, DemoScenario::Empty).unwrap();

        assert!(providers::list_providers(&conn).unwrap().is_empty());
        assert!(providers::list_upstream_models(&conn).unwrap().is_empty());
        assert!(routes::list_routes(&conn).unwrap().is_empty());
        assert!(list_virtual_keys(&conn).unwrap().is_empty());
        assert!(list_logs(&conn, &Default::default()).unwrap().is_empty());
    }

    #[test]
    fn replace_is_idempotent_across_scenarios() {
        let db = open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        inject_demo(&conn, DemoScenario::Rich).unwrap();
        let errors = inject_demo(&conn, DemoScenario::Errors).unwrap();
        // 替换后不应叠加：提供商仍是 4 家。
        assert_eq!(providers::list_providers(&conn).unwrap().len(), 4);
        assert_eq!(count_logs(&conn, &Default::default()).unwrap() as usize, errors.logs);
    }
}
