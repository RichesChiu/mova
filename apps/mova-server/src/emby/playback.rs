use super::{
    auth::ClientAuth,
    browse::{authorize_params, path_id, Params},
    dto::{self, Kind},
};
use crate::{auth::AuthenticatedUser, error::ApiError, handlers, state::AppState};
use axum::{
    extract::{OriginalUri, Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    Json,
};
use mova_application::client_playback as app;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

async fn info(
    state: AppState,
    auth: ClientAuth,
    path: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Value,
) -> Result<Json<Value>, ApiError> {
    let mut values: HashMap<_, _> = query
        .into_iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v))
        .collect();
    if let Some(body) = body.as_object() {
        for (key, value) in body {
            if value.is_string() || value.is_number() || value.is_boolean() {
                values.insert(
                    key.to_ascii_lowercase(),
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string()),
                );
            }
        }
    }
    let q = Params::new(values);
    authorize_params(&auth, &path, &q)?;
    let id = path_id(&path, "item", Kind::Item)?;
    let source = q
        .get("MediaSourceId")
        .map(|id| dto::require_id(id, Kind::File))
        .transpose()?;
    let file = app::select_file(&state.db, &auth.context.user, id, source).await?;
    let profiles = body
        .get("DeviceProfile")
        .and_then(|p| p.get("DirectPlayProfiles"))
        .and_then(Value::as_array)
        .map(|profiles| {
            profiles
                .iter()
                .filter(|p| {
                    p.get("Type")
                        .and_then(Value::as_str)
                        .is_none_or(|t| t.eq_ignore_ascii_case("Video"))
                })
                .map(|p| app::DirectProfile {
                    container: p
                        .get("Container")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    video_codec: p
                        .get("VideoCodec")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    audio_codec: p
                        .get("AudioCodec")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                })
                .collect()
        });
    if !app::can_deliver_original(
        &file,
        &app::PlaybackCapabilities {
            direct_play: q.boolean("EnableDirectPlay", true)?,
            direct_stream: q.boolean("EnableDirectStream", true)?,
            max_bitrate: q.integer("MaxStreamingBitrate", i64::MAX)?,
            profiles,
        },
    )? {
        return Ok(Json(
            json!({"MediaSources":[],"ErrorCode":"NoCompatibleStream"}),
        ));
    }
    let (session, _) = app::prepare(&state.db, &auth.context, id, Some(file.id)).await?;
    let item = app::item(&state.db, &auth.context.user, id).await?;
    let details = app::details(&state.db, &auth.context.user, &[item]).await?;
    let mut media = dto::media_source(
        &file,
        details
            .audio
            .get(&file.id)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        details
            .subtitles
            .get(&file.id)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        &auth.token,
    );
    media["SupportsDirectPlay"] = json!(q.boolean("EnableDirectPlay", true)?);
    media["SupportsDirectStream"] = json!(q.boolean("EnableDirectStream", true)?);
    if let Some(url) = media["DirectStreamUrl"].as_str() {
        media["DirectStreamUrl"] = json!(format!("{url}&PlaySessionId={}", session.id));
    }
    Ok(Json(
        json!({"MediaSources":[media],"PlaySessionId":session.id}),
    ))
}
pub async fn info_get(
    State(state): State<AppState>,
    auth: ClientAuth,
    Path(path): Path<HashMap<String, String>>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    info(state, auth, path, q, Value::Null).await
}
pub async fn info_post(
    State(state): State<AppState>,
    auth: ClientAuth,
    Path(path): Path<HashMap<String, String>>,
    Query(q): Query<HashMap<String, String>>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    info(
        state,
        auth,
        path,
        q,
        body.map(|Json(v)| v).unwrap_or(Value::Null),
    )
    .await
}

pub async fn stream(
    State(state): State<AppState>,
    auth: ClientAuth,
    Path(path): Path<HashMap<String, String>>,
    Query(q): Query<HashMap<String, String>>,
    method: Method,
    headers: HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    let q = Params::new(q);
    authorize_params(&auth, &path, &q)?;
    if !q.boolean("Static", true)? {
        return Err(ApiError::BadRequest("transcoding is not supported".into()));
    }
    let item = path_id(&path, "item", Kind::Item)?;
    let source = q
        .get("MediaSourceId")
        .map(|id| dto::require_id(id, Kind::File))
        .transpose()?;
    let file = app::select_file(&state.db, &auth.context.user, item, source).await?;
    // Static delivery retains every embedded track; native players select their
    // audio/subtitle stream locally. No transcoding process is implied here.
    handlers::media_files::build_media_file_stream_response(
        state,
        &auth.context.user,
        file.id,
        None,
        headers,
        method == Method::HEAD,
    )
    .await
}

pub async fn subtitle(
    State(state): State<AppState>,
    auth: ClientAuth,
    Path(path): Path<HashMap<String, String>>,
    method: Method,
) -> Result<axum::response::Response, ApiError> {
    let item = path_id(&path, "item", Kind::Item)?;
    let file_id = path_id(&path, "source", Kind::File)?;
    let file = app::select_file(&state.db, &auth.context.user, item, Some(file_id)).await?;
    let audio = mova_application::list_audio_tracks_for_media_file(&state.db, file.id).await?;
    let subs = mova_application::list_subtitle_files_for_media_file(&state.db, file.id).await?;
    let index: i32 = path["index"]
        .parse()
        .map_err(|_| ApiError::BadRequest("invalid subtitle index".into()))?;
    let position = dto::subtitle_indexes(&audio, &subs)
        .iter()
        .position(|i| *i == index)
        .ok_or_else(|| ApiError::NotFound("subtitle not found".into()))?;
    let user = AuthenticatedUser(auth.context.user);
    if method == Method::HEAD {
        handlers::subtitle_files::head_subtitle_file(State(state), user, Path(subs[position].id))
            .await
    } else {
        handlers::subtitle_files::stream_subtitle_file(State(state), user, Path(subs[position].id))
            .await
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Report {
    item_id: String,
    media_source_id: Option<String>,
    play_session_id: Option<String>,
    position_ticks: Option<i64>,
    run_time_ticks: Option<i64>,
    user_id: Option<String>,
    item: Option<Value>,
}
pub async fn report(
    State(state): State<AppState>,
    auth: ClientAuth,
    OriginalUri(uri): OriginalUri,
    Json(input): Json<Report>,
) -> Result<StatusCode, ApiError> {
    super::auth::check_user(&auth.context, input.user_id.as_deref())?;
    let changed = app::report(
        &state.db,
        &auth.context,
        app::PlaybackReport {
            item_id: dto::require_id(&input.item_id, Kind::Item)?,
            file_id: input
                .media_source_id
                .as_deref()
                .map(|id| dto::require_id(id, Kind::File))
                .transpose()?,
            session_id: input.play_session_id,
            position_ticks: input.position_ticks,
            duration_ticks: input.run_time_ticks.or_else(|| {
                input
                    .item
                    .as_ref()
                    .and_then(|v| v.get("RunTimeTicks"))
                    .and_then(Value::as_i64)
            }),
            stopped: uri.path().ends_with("/Stopped"),
            started: uri.path().ends_with("/Playing"),
        },
    )
    .await?;
    if changed.is_some() {
        state
            .realtime_dispatcher
            .publish_resource_immediately(format!(
                "user:{}:continue-watching",
                auth.context.user.user.id
            ))
            .await;
    }
    Ok(StatusCode::NO_CONTENT)
}
