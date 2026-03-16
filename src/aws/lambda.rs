use aws_sdk_lambda::Client;

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
pub struct FunctionInfo {
    pub name: String,
    pub runtime: String,
    pub last_modified: String,
    pub memory: i32,
    pub timeout: i32,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct FunctionDetail {
    pub name: String,
    pub arn: String,
    pub runtime: String,
    pub handler: String,
    pub description: String,
    pub role: String,
    pub memory: i32,
    pub timeout: i32,
    pub last_modified: String,
    pub code_size: i64,
    pub code_sha256: String,
    pub architectures: Vec<String>,
    pub ephemeral_storage: Option<i32>,
    pub package_type: String,
    pub state: Option<String>,
    pub state_reason: Option<String>,
    pub env_vars: Vec<(String, String)>,
    pub layers: Vec<String>,
    pub tags: Vec<(String, String)>,
    pub dead_letter_arn: Option<String>,
    pub tracing_mode: Option<String>,
    pub vpc_id: Option<String>,
    pub subnet_ids: Vec<String>,
    pub security_group_ids: Vec<String>,
}

pub async fn list_functions(client: &Client) -> Result<Vec<FunctionInfo>, String> {
    let mut functions = Vec::new();
    let mut marker: Option<String> = None;

    loop {
        let mut req = client.list_functions().max_items(200);
        if let Some(ref m) = marker {
            req = req.marker(m);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format_aws_error("list functions", &e))?;

        for f in resp.functions() {
            functions.push(FunctionInfo {
                name: f.function_name().unwrap_or("").to_string(),
                runtime: f
                    .runtime()
                    .map(|r| r.as_str().to_string())
                    .unwrap_or_else(|| "-".to_string()),
                last_modified: f.last_modified().unwrap_or("").to_string(),
                memory: f.memory_size().unwrap_or(0),
                timeout: f.timeout().unwrap_or(0),
                description: f.description().unwrap_or("").to_string(),
            });
        }

        marker = resp.next_marker().map(|s| s.to_string());
        if marker.is_none() {
            break;
        }
    }

    Ok(functions)
}

pub async fn get_function_detail(client: &Client, name: &str) -> Result<FunctionDetail, String> {
    let resp = client
        .get_function()
        .function_name(name)
        .send()
        .await
        .map_err(|e| format_aws_error("get function", &e))?;

    let config = resp
        .configuration()
        .ok_or_else(|| "No configuration returned".to_string())?;

    let env_vars = config
        .environment()
        .and_then(|e| e.variables())
        .map(|vars| {
            let mut v: Vec<(String, String)> = vars
                .iter()
                .map(|(k, val)| (k.clone(), val.clone()))
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        })
        .unwrap_or_default();

    let layers: Vec<String> = config
        .layers()
        .iter()
        .filter_map(|l| l.arn().map(|a| a.to_string()))
        .collect();

    let tags = resp
        .tags()
        .map(|t| {
            let mut v: Vec<(String, String)> =
                t.iter().map(|(k, val)| (k.clone(), val.clone())).collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        })
        .unwrap_or_default();

    let architectures: Vec<String> = config
        .architectures()
        .iter()
        .map(|a| a.as_str().to_string())
        .collect();

    let vpc_config = config.vpc_config();

    Ok(FunctionDetail {
        name: config.function_name().unwrap_or("").to_string(),
        arn: config.function_arn().unwrap_or("").to_string(),
        runtime: config
            .runtime()
            .map(|r| r.as_str().to_string())
            .unwrap_or_else(|| "-".to_string()),
        handler: config.handler().unwrap_or("").to_string(),
        description: config.description().unwrap_or("").to_string(),
        role: config.role().unwrap_or("").to_string(),
        memory: config.memory_size().unwrap_or(0),
        timeout: config.timeout().unwrap_or(0),
        last_modified: config.last_modified().unwrap_or("").to_string(),
        code_size: config.code_size(),
        code_sha256: config.code_sha256().unwrap_or("").to_string(),
        architectures,
        ephemeral_storage: config.ephemeral_storage().map(|e| e.size()),
        package_type: config
            .package_type()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "Zip".to_string()),
        state: config.state().map(|s| s.as_str().to_string()),
        state_reason: config.state_reason().map(|s| s.to_string()),
        env_vars,
        layers,
        tags,
        dead_letter_arn: config
            .dead_letter_config()
            .and_then(|d| d.target_arn())
            .map(|s| s.to_string()),
        tracing_mode: config
            .tracing_config()
            .and_then(|t| t.mode())
            .map(|m| m.as_str().to_string()),
        vpc_id: vpc_config.and_then(|v| v.vpc_id()).map(|s| s.to_string()),
        subnet_ids: vpc_config
            .map(|v| v.subnet_ids().iter().map(|s| s.to_string()).collect())
            .unwrap_or_default(),
        security_group_ids: vpc_config
            .map(|v| {
                v.security_group_ids()
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn format_size(bytes: i64) -> String {
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
