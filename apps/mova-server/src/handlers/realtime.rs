use crate::{auth::require_user, error::ApiError, state::AppState};
use axum::{
    extract::State,
    response::sse::{KeepAlive, Sse},
};
use axum_extra::extract::cookie::CookieJar;
use std::{convert::Infallible, time::Duration};
use tokio_stream::{
    wrappers::{errors::BroadcastStreamRecvError, BroadcastStream},
    StreamExt,
};

pub async fn events(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    ApiError,
> {
    let user = require_user(&state, &jar).await?;
    let api_time_offset = state.api_time_offset;
    let stream_user = user.clone();

    let stream = BroadcastStream::new(state.realtime_hub.subscribe()).filter_map(move |message| {
        let user = stream_user.clone();

        match message {
            Ok(event) if event.is_visible_to(&user) => event.to_sse_event(api_time_offset).map(Ok),
            Ok(_) => None,
            Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    user_id = user.user.id,
                    "realtime event stream lagged"
                );
                None
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}
