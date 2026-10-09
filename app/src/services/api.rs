use crate::domain::folder::RemoteGame;
use kumo_contracts::{
    CreateGameRequest, DlsiteLookupRequest, DlsiteLookupResponse, ExecutableFingerprint,
    GameLookupRequest, GameLookupResponse, GameMetadata,
};
use reqwest::blocking::{Client, Response};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;

const DEFAULT_SERVER_URL: &str = "https://kumokumo.top";

#[derive(serde::Serialize)]
struct Credentials<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Clone, serde::Deserialize)]
pub struct AccountSession {
    pub username: String,
    pub token: String,
}

impl std::fmt::Debug for AccountSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccountSession")
            .field("username", &self.username)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(serde::Serialize)]
struct FingerprintHashesRequest {
    fingerprint_hashes: Vec<String>,
}

#[derive(serde::Serialize)]
struct GameActivityRequest<'a> {
    player_id: &'a str,
    session_id: &'a str,
    fingerprint_hash: &'a str,
}

#[derive(serde::Deserialize)]
struct PlayerCountsResponse {
    counts: HashMap<String, u64>,
}

#[derive(serde::Deserialize)]
struct AccountGamesResponse {
    games: Vec<RemoteGame>,
}

pub fn lookup_game(fingerprint: ExecutableFingerprint) -> Result<Option<GameMetadata>, String> {
    let response = client()?
        .post(endpoint("/api/games/lookup"))
        .json(&GameLookupRequest { fingerprint })
        .send()
        .map_err(|error| format!("请求游戏库失败：{error}"))?;
    decode::<GameLookupResponse>(response).map(|response| response.game)
}

pub fn lookup_games(fingerprint_hashes: Vec<String>) -> Result<Vec<RemoteGame>, String> {
    let response = client()?
        .post(endpoint("/api/games/lookup/batch"))
        .json(&FingerprintHashesRequest { fingerprint_hashes })
        .send()
        .map_err(|error| format!("请求游戏库失败：{error}"))?;
    decode::<AccountGamesResponse>(response).map(|response| response.games)
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

pub fn register_account(username: &str, password: &str) -> Result<AccountSession, String> {
    let response = client()?
        .post(auth_endpoint("/api/auth/register")?)
        .json(&Credentials { username, password })
        .send()
        .map_err(|error| format!("注册账号失败：{error}"))?;
    decode(response)
}

pub fn login_account(username: &str, password: &str) -> Result<AccountSession, String> {
    let response = client()?
        .post(auth_endpoint("/api/auth/login")?)
        .json(&Credentials { username, password })
        .send()
        .map_err(|error| format!("登录失败：{error}"))?;
    decode(response)
}

pub fn logout_account(token: &str) -> Result<(), String> {
    let response = client()?
        .post(auth_endpoint("/api/auth/logout")?)
        .bearer_auth(token)
        .send()
        .map_err(|error| format!("退出账号失败：{error}"))?;
    decode::<serde_json::Value>(response).map(|_| ())
}

pub fn sync_account_games(
    token: &str,
    fingerprint_hashes: Vec<String>,
) -> Result<Vec<RemoteGame>, String> {
    let response = client()?
        .post(auth_endpoint("/api/account/games/sync")?)
        .bearer_auth(token)
        .json(&FingerprintHashesRequest { fingerprint_hashes })
        .send()
        .map_err(|error| format!("同步账号游戏库失败：{error}"))?;
    decode::<AccountGamesResponse>(response).map(|response| response.games)
}

pub fn player_counts(fingerprint_hashes: Vec<String>) -> Result<HashMap<String, u64>, String> {
    let response = client()?
        .post(endpoint("/api/games/activity/counts"))
        .json(&FingerprintHashesRequest { fingerprint_hashes })
        .send()
        .map_err(|error| format!("获取在线人数失败：{error}"))?;
    decode::<PlayerCountsResponse>(response).map(|response| response.counts)
}

pub fn heartbeat_game_activity(
    player_id: &str,
    session_id: &str,
    fingerprint_hash: &str,
) -> Result<(), String> {
    let response = client()?
        .post(endpoint("/api/games/activity"))
        .json(&GameActivityRequest {
            player_id,
            session_id,
            fingerprint_hash,
        })
        .send()
        .map_err(|error| format!("发送游戏状态失败：{error}"))?;
    decode::<serde_json::Value>(response).map(|_| ())
}

pub fn stop_game_activity(
    player_id: &str,
    session_id: &str,
    fingerprint_hash: &str,
) -> Result<(), String> {
    let response = client()?
        .delete(endpoint("/api/games/activity"))
        .json(&GameActivityRequest {
            player_id,
            session_id,
            fingerprint_hash,
        })
        .send()
        .map_err(|error| format!("结束游戏状态失败：{error}"))?;
    decode::<serde_json::Value>(response).map(|_| ())
}

fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| format!("创建网络客户端失败：{error}"))
}

fn endpoint(path: &str) -> String {
    let base = server_url();
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn auth_endpoint(path: &str) -> Result<String, String> {
    let base = server_url();
    let parsed = url::Url::parse(&base).map_err(|error| format!("服务器地址无效：{error}"))?;
    if parsed.scheme() != "https" {
        return Err("账号登录和同步需要 HTTPS，请将 KUMO_SERVER_URL 设置为 HTTPS 地址".to_owned());
    }
    Ok(format!("{}{}", base.trim_end_matches('/'), path))
}

fn server_url() -> String {
    std::env::var("KUMO_SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER_URL.to_owned())
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
