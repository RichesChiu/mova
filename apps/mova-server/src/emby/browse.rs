use super::{
    auth::{check_user, ClientAuth},
    dto::{self, Kind},
};
use crate::{auth::AuthenticatedUser, error::ApiError, handlers, state::AppState};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use mova_application::client_playback as app;
use serde_json::{json, Value};
use std::collections::HashMap;

pub type Paths = Option<Path<HashMap<String, String>>>;
pub struct Params(pub HashMap<String, String>);
impl Params {
    pub fn new(values: HashMap<String, String>) -> Self {
        Self(
            values
                .into_iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v))
                .collect(),
        )
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }
    pub fn integer(&self, key: &str, default: i64) -> Result<i64, ApiError> {
        self.get(key)
            .map(|s| {
                s.parse()
                    .map_err(|_| ApiError::BadRequest(format!("invalid {key}")))
            })
            .unwrap_or(Ok(default))
    }
    pub fn boolean(&self, key: &str, default: bool) -> Result<bool, ApiError> {
        match self.get(key) {
            None => Ok(default),
            Some(s) if s.eq_ignore_ascii_case("true") || s == "1" => Ok(true),
            Some(s) if s.eq_ignore_ascii_case("false") || s == "0" => Ok(false),
            _ => Err(ApiError::BadRequest(format!("invalid {key}"))),
        }
    }
    pub fn page(&self) -> Result<(i64, i64), ApiError> {
        let offset = self.integer("StartIndex", 0)?;
        let limit = self.integer("Limit", 100)?;
        if offset < 0 || !(0..=500).contains(&limit) {
            return Err(ApiError::BadRequest("invalid pagination".into()));
        }
        Ok((offset, limit))
    }
}
pub fn paths(p: Paths) -> HashMap<String, String> {
    p.map(|Path(p)| p).unwrap_or_default()
}
pub fn authorize_params(
    auth: &ClientAuth,
    path: &HashMap<String, String>,
    q: &Params,
) -> Result<(), ApiError> {
    check_user(&auth.context, path.get("user").map(String::as_str))?;
    check_user(&auth.context, q.get("UserId"))
}
pub fn path_id(path: &HashMap<String, String>, key: &str, kind: Kind) -> Result<i64, ApiError> {
    dto::require_id(
        path.get(key)
            .ok_or_else(|| ApiError::BadRequest("missing resource id".into()))?,
        kind,
    )
}
fn paged(items: Vec<Value>, offset: i64, limit: i64) -> Value {
    let total = items.len();
    json!({"Items":items.into_iter().skip(offset as usize).take(limit as usize).collect::<Vec<_>>(),"TotalRecordCount":total,"StartIndex":offset})
}

pub async fn views(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let q = Params::new(query);
    authorize_params(&auth, &paths(path), &q)?;
    let (offset, limit) = q.page()?;
    let libraries =
        mova_application::list_libraries(&state.db, auth.context.user.library_visibility()).await?;
    let server = app::server_id(&state.db).await?;
    Ok(Json(paged(
        libraries.iter().map(|l| dto::library(l, &server)).collect(),
        offset,
        limit,
    )))
}
pub async fn root(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
) -> Result<Json<Value>, ApiError> {
    check_user(&auth.context, paths(path).get("user").map(String::as_str))?;
    Ok(Json(
        json!({"Id":dto::external_id(Kind::Root,1),"ServerId":app::server_id(&state.db).await?,"Name":"Mova","Type":"Folder","IsFolder":true}),
    ))
}

fn catalog_query(q: &Params, mode: &str) -> Result<app::CatalogQuery, ApiError> {
    let (offset, limit) = q.page()?;
    let recursive = q.boolean("Recursive", false)?;
    let default_types = if mode == "resume" || mode == "latest" {
        "Movie,Episode"
    } else if recursive {
        "Movie,Series,Episode"
    } else {
        "Movie,Series"
    };
    let kinds = q
        .get("IncludeItemTypes")
        .unwrap_or(default_types)
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    let sort = q
        .get("SortBy")
        .unwrap_or(if mode == "resume" {
            "DatePlayed"
        } else if mode == "latest" {
            "DateCreated"
        } else {
            "SortName"
        })
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    let orders = q
        .get("SortOrder")
        .unwrap_or(if mode == "resume" || mode == "latest" {
            "Descending"
        } else {
            "Ascending"
        })
        .split(',')
        .collect::<Vec<_>>();
    if orders
        .iter()
        .any(|s| !s.eq_ignore_ascii_case("Ascending") && !s.eq_ignore_ascii_case("Descending"))
    {
        return Err(ApiError::BadRequest("invalid SortOrder".into()));
    }
    let mut result = app::CatalogQuery {
        offset,
        limit,
        kinds,
        search: q.get("SearchTerm").map(str::to_string),
        resume: mode == "resume",
        sort: sort
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    s.to_string(),
                    orders
                        .get(i)
                        .unwrap_or(&orders[0])
                        .eq_ignore_ascii_case("Descending"),
                )
            })
            .collect(),
        ..Default::default()
    };
    if let Some(filters) = q.get("Filters") {
        for filter in filters.split(',') {
            match filter {
                "IsResumable" => result.resume = true,
                "IsNotFolder" => result.kinds.retain(|k| k != "series"),
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "unsupported filter: {filter}"
                    )))
                }
            }
        }
    }
    if let Some(ids) = q.get("Ids") {
        result.item_ids = Some(
            ids.split(',')
                .map(|s| dto::require_id(s, Kind::Item))
                .collect::<Result<_, _>>()?,
        );
    }
    if let Some(parent) = q.get("ParentId") {
        match dto::parse_id(parent)? {
            (Kind::Library, id) => result.library_id = Some(id),
            (Kind::Season, id) => {
                result.season_id = Some(id);
                result.kinds = vec!["episode".into()];
            }
            (Kind::Item, id) => {
                result.series_id = Some(id);
                result.kinds = vec!["episode".into()];
            }
            (Kind::Root, 1) => {}
            _ => return Err(ApiError::BadRequest("invalid parent".into())),
        }
    }
    if let Some(season) = q.get("SeasonId") {
        result.season_id = Some(dto::require_id(season, Kind::Season)?);
    }
    Ok(result)
}

async fn list(
    state: AppState,
    auth: ClientAuth,
    path: Paths,
    query: HashMap<String, String>,
    mode: &str,
) -> Result<Json<Value>, ApiError> {
    let p = paths(path);
    let q = Params::new(query);
    authorize_params(&auth, &p, &q)?;
    if mode == "items"
        && !q.boolean("Recursive", false)?
        && q.get("IncludeItemTypes").is_none()
        && q.get("SearchTerm").is_none()
    {
        if let Some(parent) = q.get("ParentId") {
            match dto::parse_id(parent)? {
                (Kind::Item, id) => {
                    let (offset, limit) = q.page()?;
                    let values = app::seasons(&state.db, &auth.context.user, id).await?;
                    let server = app::server_id(&state.db).await?;
                    return Ok(Json(paged(
                        values.iter().map(|s| dto::season(s, &server)).collect(),
                        offset,
                        limit,
                    )));
                }
                (Kind::Root, 1) => {
                    return views(State(state), auth, Some(Path(p)), Query(q.0)).await
                }
                _ => {}
            }
        }
    }
    let mut input = catalog_query(&q, mode)?;
    if mode == "episodes" {
        input.series_id = Some(path_id(&p, "item", Kind::Item)?);
        input.kinds = vec!["episode".into()];
        if q.get("SortBy").is_none() {
            input.sort = vec![
                ("ParentIndexNumber".into(), false),
                ("IndexNumber".into(), false),
            ];
        }
    }
    let offset = input.offset;
    let result = app::catalog(&state.db, &auth.context.user, input).await?;
    let details = app::details(&state.db, &auth.context.user, &result.items).await?;
    let server = app::server_id(&state.db).await?;
    let values: Vec<_> = result
        .items
        .iter()
        .map(|i| dto::item(i, &details, &server, &auth.token))
        .collect();
    Ok(Json(if mode == "latest" {
        json!(values)
    } else {
        json!({"Items":values,"TotalRecordCount":result.total,"StartIndex":offset})
    }))
}
pub async fn items(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    list(state, auth, path, q, "items").await
}
pub async fn latest(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    list(state, auth, path, q, "latest").await
}
pub async fn resume(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    list(state, auth, path, q, "resume").await
}
pub async fn episodes(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    list(state, auth, path, q, "episodes").await
}
pub async fn seasons(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let p = paths(path);
    let q = Params::new(q);
    authorize_params(&auth, &p, &q)?;
    let (offset, limit) = q.page()?;
    let seasons = app::seasons(
        &state.db,
        &auth.context.user,
        path_id(&p, "item", Kind::Item)?,
    )
    .await?;
    let server = app::server_id(&state.db).await?;
    Ok(Json(paged(
        seasons.iter().map(|s| dto::season(s, &server)).collect(),
        offset,
        limit,
    )))
}
pub async fn item(
    State(state): State<AppState>,
    auth: ClientAuth,
    path: Paths,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let p = paths(path);
    let q = Params::new(q);
    authorize_params(&auth, &p, &q)?;
    let (kind, id) = dto::parse_id(
        p.get("item")
            .ok_or_else(|| ApiError::BadRequest("missing item".into()))?,
    )?;
    let server = app::server_id(&state.db).await?;
    let value = match kind {
        Kind::Item => {
            let item = app::item(&state.db, &auth.context.user, id).await?;
            let details =
                app::details(&state.db, &auth.context.user, std::slice::from_ref(&item)).await?;
            dto::item(&item, &details, &server, &auth.token)
        }
        Kind::Season => dto::season(
            &app::season(&state.db, &auth.context.user, id).await?,
            &server,
        ),
        Kind::Library => dto::library(
            &mova_application::authorize_library(&state.db, &auth.context.user, id).await?,
            &server,
        ),
        _ => return Err(ApiError::NotFound("item not found".into())),
    };
    Ok(Json(value))
}
pub async fn image(
    State(state): State<AppState>,
    auth: ClientAuth,
    Path(p): Path<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    if p.get("index").is_some_and(|s| s != "0") {
        return Err(ApiError::NotFound("image not found".into()));
    }
    let (kind, id) = dto::parse_id(&p["item"])?;
    let user = AuthenticatedUser(auth.context.user);
    match (kind, p["kind"].to_ascii_lowercase().as_str()) {
        (Kind::Item, "primary") => {
            handlers::media_items::get_media_item_poster(State(state), user, Path(id)).await
        }
        (Kind::Item, "backdrop") => {
            handlers::media_items::get_media_item_backdrop(State(state), user, Path(id)).await
        }
        (Kind::Item, "logo") => {
            handlers::media_items::get_media_item_logo(State(state), user, Path(id)).await
        }
        (Kind::Season, "primary") => {
            handlers::seasons::get_season_poster(State(state), user, Path(id)).await
        }
        (Kind::Season, "backdrop") => {
            handlers::seasons::get_season_backdrop(State(state), user, Path(id)).await
        }
        _ => Err(ApiError::NotFound("image not found".into())),
    }
}
