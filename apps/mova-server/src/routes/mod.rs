mod auth;
mod health;
mod home;
mod libraries;
mod media_files;
mod media_items;
mod notifications;
mod playback_progress;
mod realtime;
mod search;
mod seasons;
mod server;
mod subtitle_files;
mod users;

use axum::Router;

/// 注册健康检查相关路由。
pub fn health() -> Router<crate::state::AppState> {
    health::router()
}

pub fn home() -> Router<crate::state::AppState> {
    home::router()
}

pub fn auth() -> Router<crate::state::AppState> {
    auth::router()
}

/// 注册媒体库管理相关路由。
pub fn libraries() -> Router<crate::state::AppState> {
    libraries::router()
}

/// 注册媒体条目相关路由。
pub fn media_items() -> Router<crate::state::AppState> {
    media_items::router()
}

pub fn notifications() -> Router<crate::state::AppState> {
    notifications::router()
}

/// 注册媒体文件相关路由。
pub fn media_files() -> Router<crate::state::AppState> {
    media_files::router()
}

pub fn subtitle_files() -> Router<crate::state::AppState> {
    subtitle_files::router()
}

pub fn seasons() -> Router<crate::state::AppState> {
    seasons::router()
}

pub fn server() -> Router<crate::state::AppState> {
    server::router()
}

pub fn realtime() -> Router<crate::state::AppState> {
    realtime::router()
}

pub fn search() -> Router<crate::state::AppState> {
    search::router()
}

/// 注册播放进度相关路由。
pub fn playback_progress() -> Router<crate::state::AppState> {
    playback_progress::router()
}

pub fn users() -> Router<crate::state::AppState> {
    users::router()
}
