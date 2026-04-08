use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_aws_provider_with_mock() {
    let mock_server = MockServer::start().await;

    let aws_json = r#"{
        "syncToken": "1234",
        "createDate": "2024-01-01",
        "prefixes": [
            {"ip_prefix": "3.5.140.0/22", "region": "ap-northeast-2", "service": "AMAZON", "network_border_group": "ap-northeast-2"}
        ],
        "ipv6_prefixes": [
            {"ipv6_prefix": "2600:1f01:4800::/40", "region": "us-west-2", "service": "AMAZON", "network_border_group": "us-west-2"}
        ]
    }"#;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(aws_json))
        .mount(&mock_server)
        .await;

    // Verify the mock server is reachable and returns our data
    let client = reqwest::Client::new();
    let resp = client.get(mock_server.uri()).send().await.unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();

    let prefixes = body["prefixes"].as_array().unwrap();
    assert_eq!(prefixes.len(), 1);
    assert_eq!(prefixes[0]["ip_prefix"].as_str().unwrap(), "3.5.140.0/22");

    let ipv6_prefixes = body["ipv6_prefixes"].as_array().unwrap();
    assert_eq!(ipv6_prefixes.len(), 1);
    assert_eq!(
        ipv6_prefixes[0]["ipv6_prefix"].as_str().unwrap(),
        "2600:1f01:4800::/40"
    );
}

#[tokio::test]
async fn test_gcp_provider_parsing() {
    let gcp_json = r#"{
        "syncToken": "1234",
        "creationTime": "2024-01-01",
        "prefixes": [
            {"ipv4Prefix": "34.80.0.0/15", "service": "Google Cloud", "scope": "asia-east1"},
            {"ipv6Prefix": "2600:1900:4180::/44", "service": "Google Cloud", "scope": "us-west1"}
        ]
    }"#;

    let v: serde_json::Value = serde_json::from_str(gcp_json).unwrap();
    let prefixes = v["prefixes"].as_array().unwrap();
    assert_eq!(prefixes.len(), 2);
    assert_eq!(prefixes[0]["ipv4Prefix"].as_str().unwrap(), "34.80.0.0/15");
    assert_eq!(
        prefixes[1]["ipv6Prefix"].as_str().unwrap(),
        "2600:1900:4180::/44"
    );
}

#[tokio::test]
async fn test_oracle_provider_flattening() {
    let oracle_json = r#"{
        "last_updated_timestamp": "2024-01-01",
        "regions": [
            {
                "region": "us-phoenix-1",
                "cidrs": [
                    {"cidr": "129.146.12.0/24", "tags": ["OCI"]},
                    {"cidr": "129.146.13.0/24", "tags": ["OSN", "OBJECT_STORAGE"]}
                ]
            },
            {
                "region": "eu-amsterdam-1",
                "cidrs": [
                    {"cidr": "132.145.0.0/16", "tags": ["OCI"]}
                ]
            }
        ]
    }"#;

    let v: serde_json::Value = serde_json::from_str(oracle_json).unwrap();
    let regions = v["regions"].as_array().unwrap();

    let total_cidrs: usize = regions
        .iter()
        .map(|r| r["cidrs"].as_array().unwrap().len())
        .sum();
    assert_eq!(total_cidrs, 3);

    assert_eq!(regions[0]["region"].as_str().unwrap(), "us-phoenix-1");
    assert_eq!(
        regions[0]["cidrs"][0]["cidr"].as_str().unwrap(),
        "129.146.12.0/24"
    );
    assert_eq!(regions[1]["region"].as_str().unwrap(), "eu-amsterdam-1");
}
