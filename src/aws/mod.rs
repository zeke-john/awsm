pub mod cloudwatch;
pub mod dynamodb;
pub mod lambda;
pub mod s3;
pub mod secrets;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use aws_config::SdkConfig;

pub struct AwsClients {
    pub config: SdkConfig,
    pub s3: aws_sdk_s3::Client,
    pub dynamodb: aws_sdk_dynamodb::Client,
    pub lambda: aws_sdk_lambda::Client,
    pub cloudwatch: aws_sdk_cloudwatchlogs::Client,
    pub secrets: aws_sdk_secretsmanager::Client,
}

impl AwsClients {
    pub async fn new(profile: &str) -> Self {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .profile_name(profile)
            .load()
            .await;

        Self {
            s3: aws_sdk_s3::Client::new(&config),
            dynamodb: aws_sdk_dynamodb::Client::new(&config),
            lambda: aws_sdk_lambda::Client::new(&config),
            cloudwatch: aws_sdk_cloudwatchlogs::Client::new(&config),
            secrets: aws_sdk_secretsmanager::Client::new(&config),
            config,
        }
    }

    pub fn region(&self) -> String {
        self.config
            .region()
            .map(|r| r.to_string())
            .unwrap_or_else(|| "us-east-1".to_string())
    }
}

pub fn list_profiles() -> Vec<String> {
    let mut profiles = BTreeSet::new();

    if let Some(config_path) = aws_config_path() {
        if let Ok(contents) = fs::read_to_string(config_path) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if let Some(name) = trimmed.strip_prefix("[profile ") {
                    if let Some(name) = name.strip_suffix(']') {
                        profiles.insert(name.to_string());
                    }
                } else if trimmed == "[default]" {
                    profiles.insert("default".to_string());
                }
            }
        }
    }

    if let Some(creds_path) = aws_credentials_path() {
        if let Ok(contents) = fs::read_to_string(creds_path) {
            for line in contents.lines() {
                let trimmed = line.trim();
                if let Some(name) = trimmed.strip_prefix('[') {
                    if let Some(name) = name.strip_suffix(']') {
                        profiles.insert(name.to_string());
                    }
                }
            }
        }
    }

    profiles.into_iter().collect()
}

fn aws_config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AWS_CONFIG_FILE") {
        return Some(PathBuf::from(p));
    }
    dirs_home().map(|h| h.join(".aws").join("config"))
}

fn aws_credentials_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AWS_SHARED_CREDENTIALS_FILE") {
        return Some(PathBuf::from(p));
    }
    dirs_home().map(|h| h.join(".aws").join("credentials"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}
