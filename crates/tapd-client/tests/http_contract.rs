use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tapd_client::{AttachmentUpload, ImageUpload, TapdClient, field_string, record};

static TEMP_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct MockServer {
    base_url: String,
    join: thread::JoinHandle<Vec<Vec<u8>>>,
}

impl MockServer {
    fn spawn(response_bodies: Vec<&'static str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("read mock address");
        let join = thread::spawn(move || {
            let mut requests = Vec::new();
            for body in response_bodies {
                let (mut stream, _) = listener.accept().expect("accept mock request");
                requests.push(read_request(&mut stream));
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write mock response");
            }
            requests
        });
        Self {
            base_url: format!("http://{address}"),
            join,
        }
    }

    fn client(&self) -> TapdClient {
        TapdClient::builder()
            .api_base_url(&self.base_url)
            .web_base_url(&self.base_url)
            .access_token("contract-test-secret")
            .build()
            .expect("build mock client")
    }

    fn requests(self) -> Vec<Vec<u8>> {
        self.join.join().expect("join mock server")
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = stream.read(&mut buffer).expect("read mock request");
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        let Some(header_end) = find_bytes(&bytes, b"\r\n\r\n") else {
            continue;
        };
        let content_length = content_length(&bytes[..header_end]);
        if bytes.len() >= header_end + 4 + content_length {
            break;
        }
    }
    bytes
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(headers: &[u8]) -> usize {
    String::from_utf8_lossy(headers)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn unique_temp_file() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let sequence = TEMP_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "tapd-client-http-contract-{}-{nanos}-{sequence}.txt",
        std::process::id(),
    ));
    fs::create_dir(&directory).expect("create fixture directory");
    directory.join("analysis.txt")
}

fn unique_temp_image() -> PathBuf {
    unique_temp_file()
        .parent()
        .expect("fixture parent")
        .join("overview.png")
}

#[tokio::test]
async fn form_post_uses_bearer_auth_and_decodes_tapd_envelope() {
    let server = MockServer::spawn(vec![
        r#"{"status":1,"data":{"Story":{"id":1137916943001018636,"name":"人工录入内容"}}}"#,
    ]);
    let client = server.client();

    let data = client
        .post_form(
            "/stories",
            &vec![
                ("workspace_id".into(), "37916943".into()),
                ("name".into(), "人工录入内容".into()),
            ],
        )
        .await
        .expect("post form");
    let story = record(&data, "Story").expect("one story");
    assert_eq!(
        field_string(&story, "id").as_deref(),
        Some("1137916943001018636")
    );

    let requests = server.requests();
    let request = String::from_utf8_lossy(&requests[0]);
    let request_lowercase = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /stories HTTP/1.1"));
    assert!(request_lowercase.contains("authorization: bearer contract-test-secret"));
    assert!(request_lowercase.contains("content-type: application/x-www-form-urlencoded"));
    assert!(request.contains("workspace_id=37916943"));
    assert!(request.contains("name=%E4%BA%BA%E5%B7%A5%E5%BD%95%E5%85%A5%E5%86%85%E5%AE%B9"));
}

#[tokio::test]
async fn ordinary_story_attachment_is_verified_by_read_back() {
    let server = MockServer::spawn(vec![
        r#"{"status":1,"data":{"Attachment":{"id":"9001","entry_id":"different-response-id","type":"story","filename":"analysis.txt"}}}"#,
        r#"{"status":1,"data":[{"Attachment":{"id":"9001","entry_id":"123","type":"story","filename":"analysis.txt"}}]}"#,
    ]);
    let client = server.client();
    let file = unique_temp_file();
    fs::write(&file, "business attachment").expect("write fixture");

    let upload = client
        .upload_attachment(&AttachmentUpload {
            workspace_id: "37916943".into(),
            entry_id: "123".into(),
            attachment_type: "story".into(),
            custom_field: None,
            owner: None,
            file: file.clone(),
        })
        .await
        .expect("upload and verify attachment");
    assert!(upload.verified);
    assert_eq!(
        field_string(&upload.attachment, "id").as_deref(),
        Some("9001")
    );

    fs::remove_file(&file).expect("remove fixture");
    fs::remove_dir(file.parent().expect("fixture parent")).expect("remove fixture directory");
    let requests = server.requests();
    let upload_request = String::from_utf8_lossy(&requests[0]);
    assert!(upload_request.starts_with("POST /files/upload_attachment HTTP/1.1"));
    assert!(upload_request.contains("name=\"workspace_id\""));
    assert!(upload_request.contains("37916943"));
    assert!(upload_request.contains("name=\"type\""));
    assert!(upload_request.contains("story"));
    assert!(upload_request.contains("name=\"entry_id\""));
    assert!(upload_request.contains("filename=\"analysis.txt\""));

    let list_request = String::from_utf8_lossy(&requests[1]);
    assert!(
        list_request
            .starts_with("GET /attachments?workspace_id=37916943&entry_id=123&limit=200 HTTP/1.1")
    );
}

#[tokio::test]
async fn inline_image_is_uploaded_and_verified_by_path_read_back() {
    let image_path = "/tfl/pictures/202609/api_37916943_overview.png";
    let upload_response = format!(r#"{{"status":1,"data":{{"url":"{image_path}"}}}}"#);
    let read_back_response = format!(
        r#"{{"status":1,"data":{{"Attachment":{{"type":"tfl_image","value":"{image_path}","workspace_id":37916943,"filename":"api_37916943_overview.png"}}}}}}"#
    );
    let upload_response: &'static str = Box::leak(upload_response.into_boxed_str());
    let read_back_response: &'static str = Box::leak(read_back_response.into_boxed_str());
    let server = MockServer::spawn(vec![upload_response, read_back_response]);
    let client = server.client();
    let file = unique_temp_image();
    fs::write(&file, b"png fixture bytes").expect("write image fixture");

    let upload = client
        .upload_image(&ImageUpload {
            workspace_id: "37916943".into(),
            file: file.clone(),
        })
        .await
        .expect("upload and verify image");
    assert!(upload.verified);
    assert_eq!(upload.path, image_path);

    fs::remove_file(&file).expect("remove fixture");
    fs::remove_dir(file.parent().expect("fixture parent")).expect("remove fixture directory");
    let requests = server.requests();
    let upload_request = String::from_utf8_lossy(&requests[0]);
    assert!(upload_request.starts_with("POST /files/upload_image HTTP/1.1"));
    assert!(upload_request.contains("name=\"workspace_id\""));
    assert!(upload_request.contains("37916943"));
    assert!(upload_request.contains("name=\"image\""));
    assert!(upload_request.contains("filename=\"overview.png\""));
    assert!(upload_request.contains("Content-Type: image/png"));

    let read_back_request = String::from_utf8_lossy(&requests[1]);
    assert!(read_back_request.starts_with("GET /files/get_image?"));
    assert!(read_back_request.contains("workspace_id=37916943"));
    assert!(read_back_request.contains("image_path="));
}

#[tokio::test]
async fn tapd_error_envelope_is_not_treated_as_success() {
    let server = MockServer::spawn(vec![r#"{"status":0,"data":[],"info":"permission denied"}"#]);
    let client = server.client();
    let error = client
        .get("/stories", &Vec::new())
        .await
        .expect_err("TAPD error envelope must fail");
    assert!(error.to_string().contains("permission denied"));
    let _ = server.requests();
}
