use super::*;
use std::os::unix::fs::PermissionsExt;

const PROFILE: &str = r#"
library_name = "dev"
identify_automatically = false
import_when_identified = true

[s3]
endpoint = "https://storage.example.test"
region = "region-1"
bucket = "bucket-name"
key_prefix = "prefix/path"
access_key_id = "access-key-id"
secret_access_key = "secret-access-key"

[discogs]
token = "discogs-token"
"#;

fn write(dir: &tempfile::TempDir, text: &str, mode: u32) -> PathBuf {
    let path = dir.path().join("profile.toml");
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

#[test]
fn a_private_profile_reads_into_the_library_it_describes() {
    let dir = tempfile::tempdir().unwrap();
    let profile = DevResetProfile::read(&write(&dir, PROFILE, 0o600)).unwrap();

    assert_eq!(profile.library_name.as_str(), "dev");
    assert!(!profile.identify_automatically);
    assert!(profile.import_when_identified);
    assert_eq!(profile.discogs_token, "discogs-token");
    assert_eq!(profile.s3.bucket, "bucket-name");
    assert_eq!(profile.s3.region, "region-1");
    assert_eq!(
        profile.s3.endpoint.as_deref(),
        Some("https://storage.example.test")
    );
    assert_eq!(profile.s3.key_prefix.as_deref(), Some("prefix/path"));
    assert_eq!(profile.s3.access_key, "access-key-id");
    assert_eq!(profile.s3.secret_key, "secret-access-key");
    assert!(profile.s3.profile.is_none());
}

#[test]
fn a_profile_others_can_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = DevResetProfile::read(&write(&dir, PROFILE, 0o644))
        .err()
        .expect("a readable profile is refused");

    assert!(matches!(
        error,
        DevResetProfileError::Exposed { mode: 0o644, .. }
    ));
}

#[test]
fn an_empty_credential_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let text = PROFILE.replace("\"secret-access-key\"", "\"\"");
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("an empty secret is refused");

    assert!(matches!(
        error,
        DevResetProfileError::Empty {
            field: "s3.secret_access_key",
            ..
        }
    ));
}

/// toml's own message quotes the offending line; the profile's error must
/// not, since that line may hold a credential.
#[test]
fn a_malformed_profile_names_the_line_without_repeating_it() {
    let dir = tempfile::tempdir().unwrap();
    let text = PROFILE.replace(
        "secret_access_key = \"secret-access-key\"",
        "secret_access_key = secret-access-key",
    );
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("a malformed profile is refused");

    let DevResetProfileError::Malformed { line, .. } = &error else {
        panic!("expected a malformed profile, got {error}");
    };
    assert_eq!(*line, Some(12));
    assert!(!error.to_string().contains("secret-access-key"));
}

#[test]
fn an_unknown_field_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let text = format!("{PROFILE}\n[other]\nkey = \"value\"\n");
    let error = DevResetProfile::read(&write(&dir, &text, 0o600))
        .err()
        .expect("an unknown section is refused");

    assert!(matches!(error, DevResetProfileError::Malformed { .. }));
}

fn storage(endpoint: &str, key_prefix: Option<&str>) -> S3ConfigData {
    S3ConfigData {
        bucket: "bucket-name".into(),
        region: "us-east-1".into(),
        endpoint: Some(endpoint.into()),
        key_prefix: key_prefix.map(str::to_owned),
        access_key: "access-key-id".into(),
        secret_key: "secret-access-key".into(),
        // An endpoint matching no known host names its service; AWS keys can
        // be limited to a prefix, so the location may have one.
        profile: Some(coven::S3Profile::Named(coven::S3ProfileName::Aws)),
    }
}

/// A one-connection-at-a-time S3 stand-in: the prefix holds one object until
/// it is deleted, and no uploads. Records each request line.
async fn fake_s3() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = requests.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            loop {
                let mut request = String::new();
                if stream.read_line(&mut request).await.unwrap() == 0 {
                    break;
                }
                let request = request.trim_end().to_owned();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    stream.read_line(&mut header).await.unwrap();
                    if header == "\r\n" {
                        break;
                    }
                    if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                let deleted = seen
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|line: &String| line.starts_with("DELETE"));
                let (status, reply) = if request.starts_with("DELETE") {
                    ("204 No Content", String::new())
                } else if request.contains("uploads") {
                    ("200 OK", "<ListMultipartUploadsResult><IsTruncated>false</IsTruncated></ListMultipartUploadsResult>".into())
                } else if deleted {
                    (
                        "200 OK",
                        "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>"
                            .into(),
                    )
                } else {
                    ("200 OK", "<ListBucketResult><IsTruncated>false</IsTruncated><Contents><Key>prefix/path/store/floor</Key></Contents></ListBucketResult>".into())
                };
                seen.lock().unwrap().push(request);
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\n\r\n{reply}",
                    reply.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });
    (endpoint, requests)
}

#[tokio::test]
async fn the_profile_storage_prefix_is_emptied_in_its_bucket() {
    let (endpoint, requests) = fake_s3().await;

    let emptied = empty_storage_prefix(storage(&endpoint, Some("prefix/path")))
        .await
        .unwrap();

    assert_eq!(emptied.objects, 1);
    assert_eq!(emptied.uploads, 0);
    let requests = requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .filter(|line| line.starts_with("GET"))
            .all(|line| line.starts_with("GET /bucket-name/?")
                && line.contains("prefix=prefix%2Fpath%2F")),
        "{requests:?}"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|line| line.starts_with("DELETE"))
            .collect::<Vec<_>>(),
        ["DELETE /bucket-name/prefix/path/store/floor?x-id=DeleteObject HTTP/1.1"]
    );
}

#[tokio::test]
async fn a_storage_without_a_key_prefix_is_refused_before_any_request() {
    // Nothing listens here.
    let error = empty_storage_prefix(storage("http://127.0.0.1:9", None))
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            EmptyStoragePrefixError::Coven(coven::EmptyS3PrefixError::WholeBucket)
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn an_endpoint_that_is_not_a_url_is_refused() {
    let error = empty_storage_prefix(storage("storage example", Some("prefix")))
        .await
        .unwrap_err();

    assert!(
        matches!(error, EmptyStoragePrefixError::Endpoint(_)),
        "{error:?}"
    );
}
