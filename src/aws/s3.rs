use aws_sdk_s3::Client;

fn format_aws_error(op: &str, err: &impl std::fmt::Display) -> String {
    let msg = err.to_string();
    if msg.contains("dispatch failure") || msg.contains("no credentials") || msg.contains("expired")
    {
        format!("AWS credentials expired or missing (r to retry)")
    } else if msg.contains("AccessDenied") {
        format!("Access denied for {}. Check your IAM permissions.", op)
    } else {
        format!("Failed to {}: {}", op, msg)
    }
}

#[derive(Debug, Clone)]
pub struct BucketInfo {
    pub name: String,
    pub region: Option<String>,
    pub created: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub key: String,
    pub display_name: String,
    pub is_prefix: bool,
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub storage_class: Option<String>,
}

pub async fn list_buckets(client: &Client) -> Result<Vec<BucketInfo>, String> {
    let resp = client
        .list_buckets()
        .send()
        .await
        .map_err(|e| format_aws_error("list buckets", &e))?;

    let buckets = resp
        .buckets()
        .iter()
        .map(|b| BucketInfo {
            name: b.name().unwrap_or("").to_string(),
            region: None,
            created: b.creation_date().map(|d| {
                d.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                    .unwrap_or_default()
            }),
        })
        .collect();

    Ok(buckets)
}

pub async fn list_objects(
    client: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<ObjectInfo>, String> {
    let mut builder = client
        .list_objects_v2()
        .bucket(bucket)
        .delimiter("/")
        .max_keys(1000);

    if !prefix.is_empty() {
        builder = builder.prefix(prefix);
    }

    let resp = builder
        .send()
        .await
        .map_err(|e| format_aws_error("list objects", &e))?;

    let mut items = Vec::new();

    for cp in resp.common_prefixes() {
        if let Some(p) = cp.prefix() {
            let display = p.strip_prefix(prefix).unwrap_or(p).trim_end_matches('/');
            if !display.is_empty() {
                items.push(ObjectInfo {
                    key: p.to_string(),
                    display_name: format!("{}/", display),
                    is_prefix: true,
                    size: None,
                    last_modified: None,
                    storage_class: None,
                });
            }
        }
    }

    for obj in resp.contents() {
        let key = obj.key().unwrap_or("");
        if key == prefix {
            continue;
        }
        let display = key.strip_prefix(prefix).unwrap_or(key);
        if display.is_empty() || display.contains('/') {
            continue;
        }
        items.push(ObjectInfo {
            key: key.to_string(),
            display_name: display.to_string(),
            is_prefix: false,
            size: obj.size(),
            last_modified: obj.last_modified().map(|d| {
                d.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                    .unwrap_or_default()
            }),
            storage_class: obj.storage_class().map(|s| s.as_str().to_string()),
        });
    }

    Ok(items)
}

#[derive(Debug, Clone)]
pub struct ObjectDetail {
    pub key: String,
    pub size: i64,
    pub content_type: String,
    pub last_modified: String,
    pub storage_class: String,
    pub etag: String,
    pub version_id: Option<String>,
    pub server_side_encryption: Option<String>,
    pub sse_kms_key_id: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub metadata: Vec<(String, String)>,
    pub content: Option<String>,
}

pub async fn get_object_detail(
    client: &Client,
    bucket: &str,
    key: &str,
) -> Result<ObjectDetail, String> {
    let head = client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format_aws_error("get object metadata", &e))?;

    let size = head.content_length().unwrap_or(0);
    let content_type = head
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();
    let last_modified = head
        .last_modified()
        .map(|d| {
            d.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let storage_class = head
        .storage_class()
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|| "STANDARD".to_string());
    let etag = head.e_tag().unwrap_or("").to_string();
    let version_id = head.version_id().map(|s| s.to_string());
    let server_side_encryption = head
        .server_side_encryption()
        .map(|s| s.as_str().to_string());
    let sse_kms_key_id = head.ssekms_key_id().map(|s| s.to_string());
    let content_encoding = head.content_encoding().map(|s| s.to_string());
    let content_language = head.content_language().map(|s| s.to_string());
    let cache_control = head.cache_control().map(|s| s.to_string());
    let content_disposition = head.content_disposition().map(|s| s.to_string());
    let metadata: Vec<(String, String)> = head
        .metadata()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let ext_is_text = matches!(
        key.rsplit('.').next().map(|e| e.to_lowercase()).as_deref(),
        Some(
            "txt"
                | "json"
                | "xml"
                | "yaml"
                | "yml"
                | "csv"
                | "log"
                | "md"
                | "html"
                | "htm"
                | "css"
                | "js"
                | "ts"
                | "py"
                | "rb"
                | "rs"
                | "go"
                | "java"
                | "sh"
                | "bash"
                | "zsh"
                | "toml"
                | "ini"
                | "cfg"
                | "conf"
                | "env"
                | "sql"
                | "graphql"
                | "tf"
                | "hcl"
                | "jsx"
                | "tsx"
                | "vue"
                | "svelte"
        )
    );

    let is_text = ext_is_text
        || content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("yaml")
        || content_type.contains("javascript");

    let content = if is_text && size < 512_000 {
        match client.get_object().bucket(bucket).key(key).send().await {
            Ok(resp) => {
                let body = resp.body.collect().await.map_err(|e| format!("{}", e))?;
                String::from_utf8(body.to_vec()).ok()
            }
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(ObjectDetail {
        key: key.to_string(),
        size,
        content_type,
        last_modified,
        storage_class,
        etag,
        version_id,
        server_side_encryption,
        sse_kms_key_id,
        content_encoding,
        content_language,
        cache_control,
        content_disposition,
        metadata,
        content,
    })
}

pub async fn download_object(
    client: &Client,
    bucket: &str,
    key: &str,
    dest: &std::path::Path,
) -> Result<(), String> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format_aws_error("download object", &e))?;

    let body = resp
        .body
        .collect()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    std::fs::write(dest, body.to_vec()).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

pub fn s3_uri(bucket: &str, key: &str) -> String {
    format!("s3://{}/{}", bucket, key)
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
