use std::collections::{BTreeMap, BTreeSet};

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;

fn format_aws_error(op: &str, err: &impl std::fmt::Display) -> String {
    let msg = err.to_string();
    if msg.contains("dispatch failure")
        || msg.contains("no credentials")
        || msg.contains("expired")
    {
        "AWS credentials expired or missing. Run: aws sso login --profile <your-profile> (r to retry)"
            .to_string()
    } else if msg.contains("AccessDenied") {
        format!("Access denied for {}. Check your IAM permissions.", op)
    } else {
        format!("Failed to {}: {}", op, msg)
    }
}

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub status: String,
    pub item_count: i64,
    pub size_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct TableDetail {
    pub name: String,
    pub status: String,
    pub partition_key: String,
    pub partition_key_type: String,
    pub sort_key: Option<String>,
    pub sort_key_type: Option<String>,
    pub item_count: i64,
    pub size_bytes: i64,
    pub billing_mode: String,
    pub indexes: Vec<IndexInfo>,
}

#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub partition_key: String,
    pub partition_key_type: String,
    pub sort_key: Option<String>,
    pub sort_key_type: Option<String>,
    pub item_count: i64,
}

#[derive(Debug, Clone)]
pub struct DynamoItem {
    pub attributes: BTreeMap<String, String>,
    pub raw: BTreeMap<String, AttributeValue>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub items: Vec<DynamoItem>,
    pub columns: Vec<String>,
    pub last_key: Option<BTreeMap<String, AttributeValue>>,
    pub scanned_count: i64,
}

pub async fn list_tables(client: &Client) -> Result<Vec<TableInfo>, String> {
    let mut tables = Vec::new();
    let mut exclusive_start = None;

    loop {
        let mut req = client.list_tables().limit(100);
        if let Some(ref start) = exclusive_start {
            req = req.exclusive_start_table_name(start);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format_aws_error("list tables", &e))?;

        let names = resp.table_names();
        if names.is_empty() {
            break;
        }

        for name in names {
            tables.push(TableInfo {
                name: name.to_string(),
                status: String::new(),
                item_count: 0,
                size_bytes: 0,
            });
        }

        match resp.last_evaluated_table_name() {
            Some(last) => exclusive_start = Some(last.to_string()),
            None => break,
        }
    }

    for table in &mut tables {
        if let Ok(desc) = client
            .describe_table()
            .table_name(&table.name)
            .send()
            .await
        {
            if let Some(t) = desc.table() {
                table.status = t
                    .table_status()
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_default();
                table.item_count = t.item_count().unwrap_or(0);
                table.size_bytes = t.table_size_bytes().unwrap_or(0);
            }
        }
    }

    Ok(tables)
}

pub async fn describe_table(client: &Client, name: &str) -> Result<TableDetail, String> {
    let resp = client
        .describe_table()
        .table_name(name)
        .send()
        .await
        .map_err(|e| format_aws_error("describe table", &e))?;

    let table = resp
        .table()
        .ok_or_else(|| "Table not found".to_string())?;

    let key_schema = table.key_schema();
    let attr_defs = table.attribute_definitions();

    let mut pk = String::new();
    let mut pk_type = String::new();
    let mut sk: Option<String> = None;
    let mut sk_type: Option<String> = None;

    for ks in key_schema {
        let attr_name = ks.attribute_name().to_string();
        let attr_type = attr_defs
            .iter()
            .find(|a| a.attribute_name() == ks.attribute_name())
            .and_then(|a| Some(a.attribute_type().as_str().to_string()))
            .unwrap_or_default();

        match ks.key_type().as_str() {
            "HASH" => {
                pk = attr_name;
                pk_type = attr_type;
            }
            "RANGE" => {
                sk = Some(attr_name);
                sk_type = Some(attr_type);
            }
            _ => {}
        }
    }

    let mut indexes = Vec::new();
    for gsi in table.global_secondary_indexes() {
        let mut idx = IndexInfo {
            name: gsi.index_name().unwrap_or("").to_string(),
            partition_key: String::new(),
            partition_key_type: String::new(),
            sort_key: None,
            sort_key_type: None,
            item_count: gsi.item_count().unwrap_or(0),
        };

        for ks in gsi.key_schema() {
            let attr_name = ks.attribute_name().to_string();
            let attr_type = attr_defs
                .iter()
                .find(|a| a.attribute_name() == ks.attribute_name())
                .and_then(|a| Some(a.attribute_type().as_str().to_string()))
                .unwrap_or_default();

            match ks.key_type().as_str() {
                "HASH" => {
                    idx.partition_key = attr_name;
                    idx.partition_key_type = attr_type;
                }
                "RANGE" => {
                    idx.sort_key = Some(attr_name);
                    idx.sort_key_type = Some(attr_type);
                }
                _ => {}
            }
        }
        indexes.push(idx);
    }

    let billing = table
        .billing_mode_summary()
        .and_then(|b| b.billing_mode())
        .map(|b| b.as_str().to_string())
        .unwrap_or_else(|| "PROVISIONED".to_string());

    Ok(TableDetail {
        name: name.to_string(),
        status: table
            .table_status()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default(),
        partition_key: pk,
        partition_key_type: pk_type,
        sort_key: sk,
        sort_key_type: sk_type,
        item_count: table.item_count().unwrap_or(0),
        size_bytes: table.table_size_bytes().unwrap_or(0),
        billing_mode: billing,
        indexes,
    })
}

pub async fn scan_table(
    client: &Client,
    table: &str,
    index: Option<&str>,
    limit: i32,
    start_key: Option<&BTreeMap<String, AttributeValue>>,
) -> Result<ScanResult, String> {
    let mut req = client.scan().table_name(table).limit(limit);

    if let Some(idx) = index {
        req = req.index_name(idx);
    }
    if let Some(key) = start_key {
        for (k, v) in key {
            req = req.exclusive_start_key(k, v.clone());
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format_aws_error("scan table", &e))?;

    let scanned = resp.scanned_count() as i64;
    let last_key = resp
        .last_evaluated_key()
        .map(|k| k.iter().map(|(k, v)| (k.clone(), v.clone())).collect());

    let mut columns_set = BTreeSet::new();
    let mut items = Vec::new();

    for item in resp.items() {
        let mut attributes = BTreeMap::new();
        let mut raw = BTreeMap::new();
        for (k, v) in item {
            columns_set.insert(k.clone());
            attributes.insert(k.clone(), format_attribute_value(v));
            raw.insert(k.clone(), v.clone());
        }
        items.push(DynamoItem { attributes, raw });
    }

    let columns: Vec<String> = columns_set.into_iter().collect();

    Ok(ScanResult {
        items,
        columns,
        last_key,
        scanned_count: scanned,
    })
}

pub async fn query_table(
    client: &Client,
    table: &str,
    index: Option<&str>,
    pk_name: &str,
    pk_value: &str,
    sk_name: Option<&str>,
    sk_condition: Option<&str>,
    sk_value: Option<&str>,
    scan_forward: bool,
    limit: i32,
) -> Result<ScanResult, String> {
    let mut req = client
        .query()
        .table_name(table)
        .limit(limit)
        .scan_index_forward(scan_forward)
        .key_condition_expression("#pk = :pkval")
        .expression_attribute_names("#pk", pk_name)
        .expression_attribute_values(":pkval", AttributeValue::S(pk_value.to_string()));

    if let Some(idx) = index {
        req = req.index_name(idx);
    }

    if let (Some(sk_n), Some(cond), Some(sk_v)) = (sk_name, sk_condition, sk_value) {
        let expr = match cond {
            "=" => "#pk = :pkval AND #sk = :skval",
            "begins_with" => "#pk = :pkval AND begins_with(#sk, :skval)",
            ">" => "#pk = :pkval AND #sk > :skval",
            ">=" => "#pk = :pkval AND #sk >= :skval",
            "<" => "#pk = :pkval AND #sk < :skval",
            "<=" => "#pk = :pkval AND #sk <= :skval",
            _ => "#pk = :pkval AND #sk = :skval",
        };
        req = req
            .key_condition_expression(expr)
            .expression_attribute_names("#sk", sk_n)
            .expression_attribute_values(":skval", AttributeValue::S(sk_v.to_string()));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format_aws_error("query table", &e))?;

    let scanned = resp.scanned_count() as i64;
    let last_key = resp
        .last_evaluated_key()
        .map(|k| k.iter().map(|(k, v)| (k.clone(), v.clone())).collect());

    let mut columns_set = BTreeSet::new();
    let mut items = Vec::new();

    for item in resp.items() {
        let mut attributes = BTreeMap::new();
        let mut raw = BTreeMap::new();
        for (k, v) in item {
            columns_set.insert(k.clone());
            attributes.insert(k.clone(), format_attribute_value(v));
            raw.insert(k.clone(), v.clone());
        }
        items.push(DynamoItem { attributes, raw });
    }

    let columns: Vec<String> = columns_set.into_iter().collect();

    Ok(ScanResult {
        items,
        columns,
        last_key,
        scanned_count: scanned,
    })
}

pub fn format_attribute_value(val: &AttributeValue) -> String {
    match val {
        AttributeValue::S(s) => s.clone(),
        AttributeValue::N(n) => n.clone(),
        AttributeValue::Bool(b) => b.to_string(),
        AttributeValue::Null(_) => "null".to_string(),
        AttributeValue::Ss(list) => format!("[{}]", list.join(", ")),
        AttributeValue::Ns(list) => format!("[{}]", list.join(", ")),
        AttributeValue::L(list) => {
            if list.is_empty() {
                "[]".to_string()
            } else {
                format!("[{} items]", list.len())
            }
        }
        AttributeValue::M(map) => {
            if map.is_empty() {
                "{}".to_string()
            } else {
                format!("{{{} keys}}", map.len())
            }
        }
        AttributeValue::B(_) => "<binary>".to_string(),
        AttributeValue::Bs(_) => "<binary set>".to_string(),
        _ => "<unknown>".to_string(),
    }
}

pub fn attribute_value_to_json(val: &AttributeValue) -> serde_json::Value {
    match val {
        AttributeValue::S(s) => serde_json::Value::String(s.clone()),
        AttributeValue::N(n) => {
            if let Ok(i) = n.parse::<i64>() {
                serde_json::Value::Number(i.into())
            } else if let Ok(f) = n.parse::<f64>() {
                serde_json::json!(f)
            } else {
                serde_json::Value::String(n.clone())
            }
        }
        AttributeValue::Bool(b) => serde_json::Value::Bool(*b),
        AttributeValue::Null(_) => serde_json::Value::Null,
        AttributeValue::Ss(list) => {
            serde_json::Value::Array(list.iter().map(|s| serde_json::Value::String(s.clone())).collect())
        }
        AttributeValue::Ns(list) => {
            serde_json::Value::Array(list.iter().map(|n| {
                n.parse::<i64>()
                    .map(|i| serde_json::Value::Number(i.into()))
                    .unwrap_or_else(|_| serde_json::Value::String(n.clone()))
            }).collect())
        }
        AttributeValue::L(list) => {
            serde_json::Value::Array(list.iter().map(attribute_value_to_json).collect())
        }
        AttributeValue::M(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), attribute_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        AttributeValue::B(b) => serde_json::Value::String(format!("<binary {} bytes>", b.as_ref().len())),
        _ => serde_json::Value::String("<unknown>".to_string()),
    }
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
