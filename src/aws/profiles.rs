use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, warn};

/// List all AWS profiles from ~/.aws/credentials and ~/.aws/config
pub fn list_profiles() -> Result<Vec<String>> {
    let mut profiles = HashSet::new();

    // Always include default
    profiles.insert("default".to_string());

    // Read from ~/.aws/credentials
    if let Some(creds_path) = get_aws_credentials_path() {
        if let Ok(content) = fs::read_to_string(&creds_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('[') && line.ends_with(']') {
                    let profile = line[1..line.len() - 1].to_string();
                    profiles.insert(profile);
                }
            }
        }
    }

    // Read from ~/.aws/config
    if let Some(config_path) = get_aws_config_path() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('[') && line.ends_with(']') {
                    let section = &line[1..line.len() - 1];
                    // Config file uses "profile <name>" format, except for default
                    let profile = if section.starts_with("profile ") {
                        section.strip_prefix("profile ").unwrap().to_string()
                    } else {
                        section.to_string()
                    };
                    profiles.insert(profile);
                }
            }
        }
    }

    let mut profiles: Vec<String> = profiles.into_iter().collect();
    profiles.sort();

    Ok(profiles)
}

/// Fetch AWS regions dynamically from AWS API
/// Uses EC2 DescribeRegions API to get all enabled regions
pub async fn fetch_regions_from_aws(profile: &str, region: &str) -> Result<Vec<String>> {
    use crate::aws::client::AwsClients;
    use quick_xml::de::from_str;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct DescribeRegionsResponse {
        #[serde(rename = "regionInfo")]
        region_info: RegionInfo,
    }

    #[derive(Debug, Deserialize)]
    struct RegionInfo {
        #[serde(rename = "item", default)]
        items: Vec<RegionItem>,
    }

    #[derive(Debug, Deserialize)]
    struct RegionItem {
        #[serde(rename = "regionName")]
        region_name: String,
    }

    debug!("Fetching AWS regions dynamically using profile '{}' and region '{}'", profile, region);

    // Create AWS client
    let (client, _) = AwsClients::new(profile, region, None).await?;

    // Call DescribeRegions API with filters for opted-in regions
    let params = vec![
        ("Action", "DescribeRegions"),
        ("Version", "2016-11-15"),
        ("Filter.1.Name", "opt-in-status"),
        ("Filter.1.Value.1", "opt-in-not-required"),
        ("Filter.1.Value.2", "opted-in"),
    ];

    let response = client.http.call("ec2", params, None).await?;

    // Parse XML response
    let parsed: DescribeRegionsResponse = from_str(&response)
        .map_err(|e| anyhow::anyhow!("Failed to parse DescribeRegions response: {}", e))?;

    let mut regions: Vec<String> = parsed
        .region_info
        .items
        .into_iter()
        .map(|item| item.region_name)
        .collect();

    regions.sort();
    debug!("Fetched {} regions from AWS", regions.len());

    Ok(regions)
}

/// List common AWS regions (hardcoded fallback)
/// This is used as a fallback if dynamic region fetching fails
pub fn list_regions_hardcoded() -> Vec<String> {
    vec![
        "us-east-1".to_string(),
        "us-east-2".to_string(),
        "us-west-1".to_string(),
        "us-west-2".to_string(),
        "af-south-1".to_string(),
        "ap-east-1".to_string(),
        "ap-south-1".to_string(),
        "ap-south-2".to_string(),
        "ap-southeast-1".to_string(),
        "ap-southeast-2".to_string(),
        "ap-southeast-3".to_string(),
        "ap-southeast-4".to_string(),
        "ap-northeast-1".to_string(),
        "ap-northeast-2".to_string(),
        "ap-northeast-3".to_string(),
        "ca-central-1".to_string(),
        "eu-central-1".to_string(),
        "eu-central-2".to_string(),
        "eu-west-1".to_string(),
        "eu-west-2".to_string(),
        "eu-west-3".to_string(),
        "eu-south-1".to_string(),
        "eu-south-2".to_string(),
        "eu-north-1".to_string(),
        "me-south-1".to_string(),
        "me-central-1".to_string(),
        "sa-east-1".to_string(),
        "eusc-de-east-1".to_string(),
    ]
}

/// List AWS regions - tries to fetch dynamically, falls back to hardcoded list
pub async fn list_regions(profile: &str, region: &str) -> Vec<String> {
    match fetch_regions_from_aws(profile, region).await {
        Ok(regions) => {
            if regions.is_empty() {
                warn!("Dynamic region fetch returned empty list, using hardcoded fallback");
                list_regions_hardcoded()
            } else {
                regions
            }
        }
        Err(e) => {
            warn!("Failed to fetch regions dynamically: {}, using hardcoded fallback", e);
            list_regions_hardcoded()
        }
    }
}

fn get_aws_credentials_path() -> Option<PathBuf> {
    // Check AWS_SHARED_CREDENTIALS_FILE env var first
    if let Ok(path) = std::env::var("AWS_SHARED_CREDENTIALS_FILE") {
        return Some(PathBuf::from(path));
    }

    // Fall back to ~/.aws/credentials
    dirs::home_dir().map(|h| h.join(".aws").join("credentials"))
}

fn get_aws_config_path() -> Option<PathBuf> {
    // Check AWS_CONFIG_FILE env var first
    if let Ok(path) = std::env::var("AWS_CONFIG_FILE") {
        return Some(PathBuf::from(path));
    }

    // Fall back to ~/.aws/config
    dirs::home_dir().map(|h| h.join(".aws").join("config"))
}
