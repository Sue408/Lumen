use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};

/// 短时限流的等待上限：`Retry-After` 超过它即视为「额度 / 长冷却」，直接降级。
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);

/// 对一次上游尝试结果的处理动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptAction {
    /// 2xx：本次候选履约，停止切换。
    Served,
    /// 429 + 短 `Retry-After`：等待后重试**同一**目标（保住 prompt cache）。
    RetrySame { delay: Duration },
    /// 可恢复失败（5xx / 429 长冷却）：换下一个候选。
    Failover,
    /// 不可恢复（其余 4xx）：透传错误，不切换。
    GiveUp,
}

/// 由 HTTP 状态与 `Retry-After` 决定下一步。429 被重载：短等待重试同目标，长等待或
/// 无信号则降级；5xx（含 529）降级；其余 4xx 不降级。
pub fn classify(status: StatusCode, retry_after: Option<Duration>) -> AttemptAction {
    if status.is_success() {
        return AttemptAction::Served;
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return match retry_after {
            Some(delay) if delay <= MAX_RETRY_AFTER => AttemptAction::RetrySame { delay },
            _ => AttemptAction::Failover,
        };
    }
    if status.is_server_error() {
        return AttemptAction::Failover;
    }
    AttemptAction::GiveUp
}

/// 解析 `Retry-After` 的秒数形式。HTTP-date 形式与非法值一律视为「无信号」（→ 降级）。
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get("retry-after")?.to_str().ok()?.trim();
    raw.parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(value: u64) -> Option<Duration> {
        Some(Duration::from_secs(value))
    }

    #[test]
    fn success_is_served() {
        assert_eq!(
            classify(StatusCode::OK, None),
            AttemptAction::Served
        );
        assert_eq!(
            classify(StatusCode::CREATED, None),
            AttemptAction::Served
        );
    }

    #[test]
    fn server_errors_fail_over() {
        for status in [500, 502, 503, 504, 529] {
            assert_eq!(
                classify(StatusCode::from_u16(status).unwrap(), None),
                AttemptAction::Failover,
                "{status} 应降级"
            );
        }
    }

    #[test]
    fn short_retry_after_retries_same_target() {
        assert_eq!(
            classify(StatusCode::TOO_MANY_REQUESTS, seconds(2)),
            AttemptAction::RetrySame {
                delay: Duration::from_secs(2)
            }
        );
        assert_eq!(
            classify(StatusCode::TOO_MANY_REQUESTS, seconds(10)),
            AttemptAction::RetrySame {
                delay: Duration::from_secs(10)
            }
        );
    }

    #[test]
    fn long_or_missing_retry_after_fails_over() {
        assert_eq!(
            classify(StatusCode::TOO_MANY_REQUESTS, seconds(11)),
            AttemptAction::Failover
        );
        assert_eq!(
            classify(StatusCode::TOO_MANY_REQUESTS, seconds(60)),
            AttemptAction::Failover
        );
        assert_eq!(
            classify(StatusCode::TOO_MANY_REQUESTS, None),
            AttemptAction::Failover
        );
    }

    #[test]
    fn client_errors_give_up() {
        for status in [400, 401, 403, 404, 422] {
            assert_eq!(
                classify(StatusCode::from_u16(status).unwrap(), None),
                AttemptAction::GiveUp,
                "{status} 不应降级"
            );
        }
    }

    #[test]
    fn parses_retry_after_seconds_only() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "3".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), seconds(3));

        headers.insert("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), None);

        headers.insert("retry-after", "soon".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), None);

        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }
}
