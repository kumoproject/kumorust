use kumo_contracts::{
    CreateGameRequest, DlsiteLookupRequest, DlsiteLookupResponse, ExecutableFingerprint,
    GameLookupRequest, GameLookupResponse, GameMetadata,
};
use reqwest::blocking::{Client, Response};
use serde::de::DeserializeOwned;
use std::time::Duration;

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:3100";

pub fn lookup_game(fingerprint: ExecutableFingerprint) -> Result<Option<GameMetadata>, String> {
    let response = client()?
        .post(endpoint("/api/games/lookup"))
        .json(&GameLookupRequest { fingerprint })
        .send()
        .map_err(|error| format!("请求游戏库失败：{error}"))?;
    decode::<GameLookupResponse>(response).map(|response| response.game)
}

pub fn search_dlsite(rj_code: String) -> Result<GameMetadata, String> {
    let response = client()?
        .post(endpoint("/api/games/dlsite"))
        .json(&DlsiteLookupRequest { rj_code })
        .send()
        .map_err(|error| format!("请求 DLsite 失败：{error}"))?;
    decode::<DlsiteLookupResponse>(response).map(|response| response.metadata)
}

pub fn register_game(
    fingerprint: ExecutableFingerprint,
    metadata: GameMetadata,
) -> Result<GameMetadata, String> {
    let response = client()?
        .post(endpoint("/api/games"))
        .json(&CreateGameRequest {
            fingerprint,
            metadata,
        })
        .send()
        .map_err(|error| format!("保存游戏信息失败：{error}"))?;
    decode(response)
}

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|error| format!("创建网络客户端失败：{error}"))
}

fn endpoint(path: &str) -> String {
    let base = std::env::var("KUMO_SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER_URL.to_owned());
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn decode<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let detail = body.trim();
        return Err(if detail.is_empty() {
            format!("服务器返回 HTTP {status}")
        } else {
            format!("服务器返回 HTTP {status}：{detail}")
        });
    }
    response
        .json::<T>()
        .map_err(|error| format!("服务器响应格式错误：{error}"))
}
