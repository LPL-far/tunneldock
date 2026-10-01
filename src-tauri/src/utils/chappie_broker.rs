use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn version_major(version: &str) -> Option<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

pub fn versions_compatible(broker_version: &str, extension_version: &str) -> bool {
    match (
        version_major(broker_version),
        version_major(extension_version),
    ) {
        (Some(broker_major), Some(extension_major)) => broker_major == extension_major,
        _ => true,
    }
}

pub fn chappie_directory() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chappie")
}

#[cfg(target_os = "windows")]
pub fn broker_endpoint(directory: &Path) -> String {
    let identity = directory
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();

    let digest = Sha256::digest(identity.as_bytes());
    let hash = format!("{:x}", digest);

    format!(r"\\.\pipe\chappie-{}", &hash[..16])
}

#[cfg(not(target_os = "windows"))]
pub fn broker_endpoint(directory: &Path) -> String {
    directory.join("broker.sock").to_string_lossy().to_string()
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct BrokerSession {
    pub id: String,
    pub agent: String,
    pub cwd: String,
    pub device: String,
    pub model: Option<String>,
    pub name: Option<String>,
    pub status: String,

    #[serde(rename = "bindingCount", default)]
    pub binding_count: u32,
}

#[derive(Debug, serde::Deserialize)]
struct BrokerSessionsResponse {
    #[serde(rename = "type")]
    response_type: String,
    id: u64,
    sessions: Vec<BrokerSession>,
}

fn parse_sessions_response(raw: &str, request_id: u64) -> Result<Vec<BrokerSession>, String> {
    let response: BrokerSessionsResponse =
        serde_json::from_str(raw).map_err(|e| format!("解析 Chappie Broker 响应失败: {}", e))?;

    if response.response_type != "response" {
        return Err(format!(
            "Chappie Broker 返回了意外消息类型: {}",
            response.response_type
        ));
    }

    if response.id != request_id {
        return Err(format!(
            "Chappie Broker 响应 ID 不匹配: expected {}, got {}",
            request_id, response.id
        ));
    }

    Ok(response.sessions)
}

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

async fn query_sessions_from_stream<S>(mut stream: S) -> Result<Vec<BrokerSession>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    const REQUEST_ID: u64 = 1;

    let request = serde_json::json!({
        "type": "request",
        "id": REQUEST_ID,
        "clientId": "tunneldock",
        "request": {
            "type": "sessions"
        }
    });

    let mut payload =
        serde_json::to_vec(&request).map_err(|e| format!("序列化 Broker 请求失败: {}", e))?;
    payload.push(b'\n');

    stream
        .write_all(&payload)
        .await
        .map_err(|e| format!("写入 Chappie Broker 请求失败: {}", e))?;

    stream
        .flush()
        .await
        .map_err(|e| format!("刷新 Chappie Broker 请求失败: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        reader.read_line(&mut line),
    )
    .await
    .map_err(|_| "等待 Chappie Broker 响应超时".to_string())?
    .map_err(|e| format!("读取 Chappie Broker 响应失败: {}", e))?;

    if bytes == 0 {
        return Err("Chappie Broker 在返回 sessions 前关闭了连接".to_string());
    }

    parse_sessions_response(line.trim(), REQUEST_ID)
}

pub async fn list_sessions() -> Result<Vec<BrokerSession>, String> {
    let directory = chappie_directory();
    let endpoint = broker_endpoint(&directory);

    #[cfg(target_os = "windows")]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        let stream = ClientOptions::new()
            .open(&endpoint)
            .map_err(|e| format!("连接 Chappie Broker {} 失败: {}", endpoint, e))?;

        query_sessions_from_stream(stream).await
    }

    #[cfg(not(target_os = "windows"))]
    {
        let stream = tokio::net::UnixStream::connect(&endpoint)
            .await
            .map_err(|e| format!("连接 Chappie Broker {} 失败: {}", endpoint, e))?;

        query_sessions_from_stream(stream).await
    }
}

pub async fn find_session(session_id: &str) -> Result<Option<BrokerSession>, String> {
    let sessions = list_sessions().await?;

    Ok(sessions
        .into_iter()
        .find(|session| session.id == session_id))
}

pub async fn wait_for_session(
    session_id: &str,
    expected_cwd: &str,
    timeout: std::time::Duration,
) -> Result<BrokerSession, String> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        match find_session(session_id).await {
            Ok(Some(session)) => {
                let actual_cwd = session.cwd.replace('\\', "/").to_lowercase();
                let expected_cwd = expected_cwd.replace('\\', "/").to_lowercase();

                if actual_cwd != expected_cwd {
                    return Err(format!(
                        "Chappie Session cwd 不匹配: expected {}, got {}",
                        expected_cwd, actual_cwd
                    ));
                }

                return Ok(session);
            }
            Ok(None) => {}
            Err(_) => {
                // Broker may still be starting together with otunnel.
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "等待 Chappie Broker 注册 Session {} 超时",
                session_id
            ));
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chappie_major_version_mismatch() {
        assert!(versions_compatible("1.0.0", "1.1.0"));
        assert!(!versions_compatible("1.0.0", "0.5.0"));
        assert!(!versions_compatible("2.0.0", "1.9.0"));
    }

    #[test]
    fn chappie_directory_ends_with_dot_chappie() {
        assert_eq!(
            chappie_directory()
                .file_name()
                .and_then(|name| name.to_str()),
            Some(".chappie")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_broker_endpoint_is_named_pipe() {
        let endpoint = broker_endpoint(Path::new(r"C:\Users\Test\.chappie"));

        assert!(endpoint.starts_with(r"\\.\pipe\chappie-"));
        assert_eq!(endpoint.len(), r"\\.\pipe\chappie-".len() + 16);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn unix_broker_endpoint_is_socket_path() {
        assert_eq!(
            broker_endpoint(Path::new("/home/test/.chappie")),
            "/home/test/.chappie/broker.sock"
        );
    }

    #[test]
    fn parses_broker_sessions_response() {
        let raw = r#"{
        "type": "response",
        "id": 1,
        "sessions": [
            {
                "id": "session-123",
                "agent": "pi",
                "cwd": "D:\\project",
                "device": "desktop",
                "model": "chappie/chatgpt",
                "name": "TunnelDock:test",
                "status": "idle",
                "bindingCount": 2
            }
        ]
    }"#;

        let sessions =
            parse_sessions_response(raw, 1).expect("valid broker sessions response should parse");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "session-123");
        assert_eq!(sessions[0].cwd, r"D:\project");
        assert_eq!(sessions[0].binding_count, 2);
    }
}
