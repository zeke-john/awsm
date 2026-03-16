use aws_sdk_secretsmanager::Client;

fn format_aws_error(op: &str, err: &impl std::fmt::Display) -> String {
    let msg = err.to_string();
    if msg.contains("dispatch failure") || msg.contains("no credentials") || msg.contains("expired")
    {
        "AWS credentials expired or missing (r to retry)".to_string()
    } else if msg.contains("AccessDenied") {
        format!("Access denied for {}. Check your IAM permissions.", op)
    } else {
        format!("Failed to {}: {}", op, msg)
    }
}

#[derive(Debug, Clone)]
pub struct SecretInfo {
    pub name: String,
    pub arn: String,
    pub description: String,
    pub last_accessed: String,
    pub last_changed: String,
    pub created: String,
}

#[derive(Debug, Clone)]
pub struct SecretDetail {
    pub name: String,
    pub arn: String,
    pub description: String,
    pub kms_key_id: Option<String>,
    pub rotation_enabled: bool,
    pub rotation_lambda_arn: Option<String>,
    pub rotation_days: Option<i64>,
    pub last_rotated: Option<String>,
    pub last_accessed: Option<String>,
    pub last_changed: Option<String>,
    pub created: Option<String>,
    pub deleted_date: Option<String>,
    pub tags: Vec<(String, String)>,
    pub version_ids: Vec<(String, Vec<String>)>,
    pub secret_value: Option<String>,
    pub secret_binary: bool,
}

pub async fn list_secrets(client: &Client) -> Result<Vec<SecretInfo>, String> {
    let mut secrets = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = client.list_secrets().max_results(100);
        if let Some(ref token) = next_token {
            req = req.next_token(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format_aws_error("list secrets", &e))?;

        for s in resp.secret_list() {
            secrets.push(SecretInfo {
                name: s.name().unwrap_or("").to_string(),
                arn: s.arn().unwrap_or("").to_string(),
                description: s.description().unwrap_or("").to_string(),
                last_accessed: s
                    .last_accessed_date()
                    .map(|d| {
                        d.fmt(aws_sdk_secretsmanager::primitives::DateTimeFormat::DateTime)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default(),
                last_changed: s
                    .last_changed_date()
                    .map(|d| {
                        d.fmt(aws_sdk_secretsmanager::primitives::DateTimeFormat::DateTime)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default(),
                created: s
                    .created_date()
                    .map(|d| {
                        d.fmt(aws_sdk_secretsmanager::primitives::DateTimeFormat::DateTime)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default(),
            });
        }

        next_token = resp.next_token().map(|s| s.to_string());
        if next_token.is_none() {
            break;
        }
    }

    Ok(secrets)
}

pub async fn get_secret_detail(client: &Client, secret_id: &str) -> Result<SecretDetail, String> {
    let desc = client
        .describe_secret()
        .secret_id(secret_id)
        .send()
        .await
        .map_err(|e| format_aws_error("describe secret", &e))?;

    let tags: Vec<(String, String)> = desc
        .tags()
        .iter()
        .map(|t| {
            (
                t.key().unwrap_or("").to_string(),
                t.value().unwrap_or("").to_string(),
            )
        })
        .collect();

    let version_ids: Vec<(String, Vec<String>)> = desc
        .version_ids_to_stages()
        .map(|m| {
            m.iter()
                .map(|(vid, stages)| {
                    let stage_strs: Vec<String> =
                        stages.iter().map(|s| s.as_str().to_string()).collect();
                    (vid.clone(), stage_strs)
                })
                .collect()
        })
        .unwrap_or_default();

    let fmt_date = |d: &aws_sdk_secretsmanager::primitives::DateTime| -> String {
        d.fmt(aws_sdk_secretsmanager::primitives::DateTimeFormat::DateTime)
            .unwrap_or_default()
    };

    let rotation_days = desc
        .rotation_rules()
        .and_then(|r| r.automatically_after_days());

    let mut detail = SecretDetail {
        name: desc.name().unwrap_or("").to_string(),
        arn: desc.arn().unwrap_or("").to_string(),
        description: desc.description().unwrap_or("").to_string(),
        kms_key_id: desc.kms_key_id().map(|s| s.to_string()),
        rotation_enabled: desc.rotation_enabled().unwrap_or(false),
        rotation_lambda_arn: desc.rotation_lambda_arn().map(|s| s.to_string()),
        rotation_days,
        last_rotated: desc.last_rotated_date().map(|d| fmt_date(d)),
        last_accessed: desc.last_accessed_date().map(|d| fmt_date(d)),
        last_changed: desc.last_changed_date().map(|d| fmt_date(d)),
        created: desc.created_date().map(|d| fmt_date(d)),
        deleted_date: desc.deleted_date().map(|d| fmt_date(d)),
        tags,
        version_ids,
        secret_value: None,
        secret_binary: false,
    };

    match client.get_secret_value().secret_id(secret_id).send().await {
        Ok(val) => {
            if let Some(s) = val.secret_string() {
                detail.secret_value = Some(s.to_string());
            } else if val.secret_binary().is_some() {
                detail.secret_binary = true;
            }
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("AccessDenied") {
                detail.secret_value = Some(
                    "(access denied — missing secretsmanager:GetSecretValue permission)"
                        .to_string(),
                );
            }
        }
    }

    Ok(detail)
}
