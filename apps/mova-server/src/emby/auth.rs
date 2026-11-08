use super::dto::{external_id, require_id, Kind};
use crate::{error::ApiError, handlers::auth as native_auth, state::AppState};
use axum::{
    extract::{FromRequestParts, Path, State},
    http::{request::Parts, HeaderMap, StatusCode},
    Json,
};
use mova_application::client_playback::{self as app, ClientContext, Device};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct ClientAuth {
    pub context: ClientContext,
    pub token: String,
}

impl FromRequestParts<AppState> for ClientAuth {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let query = axum::extract::Query::<HashMap<String, String>>::try_from_uri(&parts.uri)
            .map_err(|_| ApiError::BadRequest("invalid query".into()))?
            .0;
        let token = credential(&parts.headers, &query)?;
        let context = app::authenticate(&state.db, &token).await?;
        for (key, value) in &query {
            if key.eq_ignore_ascii_case("UserId") {
                check_user(&context, Some(value))?;
            }
        }
        Ok(Self { context, token })
    }
}

pub fn parameters(headers: &HeaderMap) -> Result<HashMap<String, String>, ApiError> {
    let mut result = HashMap::new();
    for name in ["authorization", "x-emby-authorization"] {
        let Some(header) = headers.get(name) else {
            continue;
        };
        let header = header
            .to_str()
            .map_err(|_| ApiError::BadRequest("invalid authorization".into()))?;
        if header.len() > 8192 {
            return Err(ApiError::BadRequest("authorization too long".into()));
        }
        let Some((scheme, fields)) = header.split_once(' ') else {
            continue;
        };
        if !scheme.eq_ignore_ascii_case("Emby") && !scheme.eq_ignore_ascii_case("MediaBrowser") {
            continue;
        }
        let mut quoted = false;
        let mut escaped = false;
        let mut start = 0;
        let mut segments = Vec::new();
        for (i, c) in fields.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' && quoted {
                escaped = true;
                continue;
            }
            if c == '"' {
                quoted = !quoted;
            }
            if c == ',' && !quoted {
                segments.push(&fields[start..i]);
                start = i + 1;
            }
        }
        if quoted {
            return Err(ApiError::BadRequest("invalid authorization quotes".into()));
        }
        segments.push(&fields[start..]);
        for segment in segments {
            let Some((key, value)) = segment.trim().split_once('=') else {
                continue;
            };
            let value = value
                .trim()
                .trim_matches('"')
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
            let key = key.trim().to_ascii_lowercase();
            if result.get(&key).is_some_and(|old| old != &value) {
                return Err(ApiError::BadRequest(
                    "conflicting client identification".into(),
                ));
            }
            result.insert(key, value);
        }
    }
    Ok(result)
}

pub fn credential(
    headers: &HeaderMap,
    query: &HashMap<String, String>,
) -> Result<String, ApiError> {
    let p = parameters(headers)?;
    let mut tokens = Vec::new();
    if let Some(t) = p.get("token") {
        tokens.push(t.clone());
    }
    for name in ["x-emby-token", "x-mediabrowser-token"] {
        if let Some(v) = headers.get(name) {
            tokens.push(
                v.to_str()
                    .map_err(|_| ApiError::Unauthorized("invalid token".into()))?
                    .to_string(),
            );
        }
    }
    if let Some(v) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some((scheme, t)) = v.split_once(' ') {
            if scheme.eq_ignore_ascii_case("Bearer") {
                tokens.push(t.trim().to_string());
            }
        }
    }
    for (key, value) in query {
        if key.eq_ignore_ascii_case("api_key") || key.eq_ignore_ascii_case("x-emby-token") {
            tokens.push(value.clone());
        }
    }
    let token = tokens
        .first()
        .filter(|t| !t.is_empty() && t.len() <= 256)
        .ok_or_else(|| ApiError::Unauthorized("authentication required".into()))?
        .clone();
    if tokens.iter().any(|t| t != &token) {
        return Err(ApiError::Unauthorized("conflicting credentials".into()));
    }
    Ok(token)
}

pub fn check_user(context: &ClientContext, value: Option<&str>) -> Result<(), ApiError> {
    if let Some(id) = value {
        if require_id(id, Kind::User)? != context.user.user.id {
            return Err(ApiError::Forbidden("user access denied".into()));
        }
    }
    Ok(())
}

pub fn user_json(context: &ClientContext, server_id: &str) -> Value {
    json!({"Id":external_id(Kind::User,context.user.user.id),"Name":context.user.user.username,"ServerId":server_id,
        "HasPassword":true,"HasConfiguredPassword":true,"HasConfiguredEasyPassword":false,
        "Policy":{"IsAdministrator":context.user.is_admin(),"IsDisabled":false,"EnableMediaPlayback":true,"EnableAudioPlaybackTranscoding":false,"EnableVideoPlaybackTranscoding":false,"EnableContentDeletion":false,"EnableRemoteControlOfOtherUsers":false},
        "Configuration":{"PlayDefaultAudioTrack":true,"SubtitleMode":"Default","EnableNextEpisodeAutoPlay":true}})
}

pub fn session_json(context: &ClientContext, server_id: &str) -> Value {
    json!({"Id":&context.token_hash[..32],"ServerId":server_id,"UserId":external_id(Kind::User,context.user.user.id),"UserName":context.user.user.username,
        "DeviceId":context.device.device_id,"DeviceName":context.device.device_name,"Client":context.device.client_name,"ApplicationVersion":context.device.client_version,
        "SupportsRemoteControl":false,"SupportsMediaControl":false})
}

pub async fn system_info(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        json!({"Id":app::server_id(&state.db).await?,"ServerName":"Mova","ProductName":"Mova",
        "Version":"4.8.0.0","MovaVersion":state.build_version,"StartupWizardCompleted":true,"SupportsHttps":false,"LocalAddresses":[],"RemoteAddresses":[]}),
    ))
}
pub async fn private_system_info(
    state: State<AppState>,
    _auth: ClientAuth,
) -> Result<Json<Value>, ApiError> {
    system_info(state).await
}

#[derive(Deserialize)]
pub struct Login {
    #[serde(rename = "Username", alias = "username")]
    username: String,
    #[serde(rename = "Pw", alias = "pw", alias = "Password", alias = "password")]
    password: String,
}
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Json<Value>, ApiError> {
    let p = parameters(&headers)?;
    let device = Device {
        device_id: p
            .get("deviceid")
            .cloned()
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()),
        device_name: p.get("device").cloned().unwrap_or_else(|| "Unknown".into()),
        client_name: p
            .get("client")
            .cloned()
            .unwrap_or_else(|| "Emby client".into()),
        client_version: p
            .get("version")
            .cloned()
            .unwrap_or_else(|| "Unknown".into()),
    };
    let key = native_auth::account_rate_limit_key(&input.username);
    native_auth::enforce_auth_rate_limit(&state, &key)?;
    let (token, context) = native_auth::resolve_auth_attempt(
        &state,
        &key,
        true,
        app::login(&state.db, input.username, input.password, device).await,
    )?;
    let server = app::server_id(&state.db).await?;
    Ok(Json(
        json!({"User":user_json(&context,&server),"SessionInfo":session_json(&context,&server),"AccessToken":token,"ServerId":server}),
    ))
}
pub async fn me(State(state): State<AppState>, auth: ClientAuth) -> Result<Json<Value>, ApiError> {
    Ok(Json(user_json(
        &auth.context,
        &app::server_id(&state.db).await?,
    )))
}
pub async fn user(
    State(state): State<AppState>,
    auth: ClientAuth,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    check_user(&auth.context, Some(&id))?;
    me(State(state), auth).await
}
pub async fn logout(
    State(state): State<AppState>,
    auth: ClientAuth,
) -> Result<StatusCode, ApiError> {
    app::logout(&state.db, &auth.context).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn capabilities(_auth: ClientAuth) -> StatusCode {
    StatusCode::NO_CONTENT
}
