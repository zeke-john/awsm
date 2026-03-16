use std::collections::BTreeMap;

use aws_sdk_cloudwatchlogs::Client;

fn format_aws_error(op: &str, err: &impl std::fmt::Display) -> String {
    let msg = err.to_string();
    if msg.contains("dispatch failure") || msg.contains("no credentials") || msg.contains("expired")
    {
        "AWS credentials expired or missing (r to retry)".to_string()
    } else if msg.contains("AccessDenied") {
        format!("Access denied for {}. Check your IAM permissions.", op)
    } else if msg.contains("service error") && op.contains("filter") {
        format!(
            "Failed to {}: {}. Try a shorter time range or use Insights (i) for large searches.",
            op, msg
        )
    } else {
        format!("Failed to {}: {}", op, msg)
    }
}

#[derive(Debug, Clone)]
pub struct LogGroupInfo {
    pub name: String,
    pub arn: String,
    pub stored_bytes: i64,
    pub retention_days: Option<i32>,
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct LogStreamInfo {
    pub name: String,
    pub last_event: Option<String>,
    pub last_ingestion: Option<String>,
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct LogEvent {
    pub timestamp: i64,
    pub message: String,
}

pub struct LogEventsResult {
    pub events: Vec<LogEvent>,
    pub next_token: Option<String>,
}

pub async fn list_log_groups(client: &Client) -> Result<Vec<LogGroupInfo>, String> {
    let mut groups = Vec::new();
    let mut token: Option<String> = None;

    loop {
        let mut req = client.describe_log_groups().limit(50);
        if let Some(ref t) = token {
            req = req.next_token(t);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format_aws_error("list log groups", &e))?;

        for g in resp.log_groups() {
            groups.push(LogGroupInfo {
                name: g.log_group_name().unwrap_or("").to_string(),
                arn: g.arn().unwrap_or("").to_string(),
                stored_bytes: g.stored_bytes().unwrap_or(0),
                retention_days: g.retention_in_days(),
                created: g
                    .creation_time()
                    .map(|t| format_epoch_ms(t))
                    .unwrap_or_default(),
            });
        }

        token = resp.next_token().map(|s| s.to_string());
        if token.is_none() {
            break;
        }
    }

    Ok(groups)
}

pub async fn list_log_streams(
    client: &Client,
    group_name: &str,
) -> Result<Vec<LogStreamInfo>, String> {
    let mut streams = Vec::new();
    let mut token: Option<String> = None;

    loop {
        let mut req = client
            .describe_log_streams()
            .log_group_name(group_name)
            .order_by(aws_sdk_cloudwatchlogs::types::OrderBy::LastEventTime)
            .descending(true)
            .limit(50);
        if let Some(ref t) = token {
            req = req.next_token(t);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format_aws_error("list log streams", &e))?;

        for s in resp.log_streams() {
            streams.push(LogStreamInfo {
                name: s.log_stream_name().unwrap_or("").to_string(),
                last_event: s.last_event_timestamp().map(|t| format_epoch_ms(t)),
                last_ingestion: s.last_ingestion_time().map(|t| format_epoch_ms(t)),
                created: s
                    .creation_time()
                    .map(|t| format_epoch_ms(t))
                    .unwrap_or_default(),
            });
        }

        token = resp.next_token().map(|s| s.to_string());
        if token.is_none() || streams.len() >= 200 {
            break;
        }
    }

    Ok(streams)
}

/// Fetch log events using FilterLogEvents (the same API the AWS console uses).
/// Events come back in chronological order (oldest first).
/// Returns a next_token for loading more events.
pub async fn get_stream_events(
    client: &Client,
    group_name: &str,
    stream_name: &str,
    next_token: Option<&str>,
) -> Result<LogEventsResult, String> {
    let mut req = client
        .filter_log_events()
        .log_group_name(group_name)
        .log_stream_names(stream_name)
        .limit(1000);

    if let Some(token) = next_token {
        req = req.next_token(token);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format_aws_error("get log events", &e))?;

    let events: Vec<LogEvent> = resp
        .events()
        .iter()
        .map(|e| LogEvent {
            timestamp: e.timestamp().unwrap_or(0),
            message: e.message().unwrap_or("").to_string(),
        })
        .collect();

    let token = resp.next_token().map(|s| s.to_string());

    Ok(LogEventsResult {
        events,
        next_token: token,
    })
}

/// Search all streams in a log group using filter_log_events (no stream filter).
pub async fn filter_log_group_events(
    client: &Client,
    group_name: &str,
    pattern: &str,
    start_ms: i64,
    end_ms: i64,
    next_token: Option<&str>,
) -> Result<LogEventsResult, String> {
    let mut req = client
        .filter_log_events()
        .log_group_name(group_name)
        .filter_pattern(pattern)
        .start_time(start_ms)
        .end_time(end_ms)
        .limit(1000);

    if let Some(token) = next_token {
        req = req.next_token(token);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format_aws_error("filter log events", &e))?;

    let events: Vec<LogEvent> = resp
        .events()
        .iter()
        .map(|e| LogEvent {
            timestamp: e.timestamp().unwrap_or(0),
            message: e.message().unwrap_or("").to_string(),
        })
        .collect();

    let token = resp.next_token().map(|s| s.to_string());

    Ok(LogEventsResult {
        events,
        next_token: token,
    })
}

pub async fn start_insights_query(
    client: &Client,
    groups: &[String],
    query: &str,
    start_secs: i64,
    end_secs: i64,
) -> Result<String, String> {
    let mut req = client
        .start_query()
        .query_string(query)
        .start_time(start_secs)
        .end_time(end_secs);

    for g in groups {
        req = req.log_group_names(g);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format_aws_error("start insights query", &e))?;

    Ok(resp.query_id().unwrap_or("").to_string())
}

#[derive(Debug, Clone)]
pub struct InsightsResult {
    pub rows: Vec<BTreeMap<String, String>>,
    pub status: String,
}

pub async fn get_insights_results(
    client: &Client,
    query_id: &str,
) -> Result<InsightsResult, String> {
    let resp = client
        .get_query_results()
        .query_id(query_id)
        .send()
        .await
        .map_err(|e| format_aws_error("get insights results", &e))?;

    let status = resp
        .status()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default();

    let rows: Vec<BTreeMap<String, String>> = resp
        .results()
        .iter()
        .map(|row| {
            row.iter()
                .map(|field| {
                    (
                        field.field().unwrap_or("").to_string(),
                        field.value().unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .collect();

    Ok(InsightsResult { rows, status })
}

pub fn format_epoch_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0);
    match dt {
        Some(d) => d.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("{}", ms),
    }
}

pub fn format_stored_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b < KB {
        format!("{} B", bytes)
    } else if b < MB {
        format!("{:.1} KB", b / KB)
    } else if b < GB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.1} GB", b / GB)
    }
}
