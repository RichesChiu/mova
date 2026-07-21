mod app;
mod auth;
mod config;
mod error;
mod handlers;
mod metadata_provider_config;
mod realtime;
mod response;
mod routes;
mod state;
mod sync_runtime;

use crate::config::AppConfig;
use mova_db::{connect, migrate, ping};
use state::AppState;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 本地开发时优先加载 .env，后续读取配置都会基于这里的环境变量。
    dotenvy::dotenv().ok();
    init_tracing();

    let config = AppConfig::from_env()?;
    // 服务启动前先完成数据库连接、迁移和连通性检查，避免带着坏状态对外提供接口。
    let pool = connect(&config.database).await?;
    migrate(&pool).await?;
    info!("database migrations applied");
    ping(&pool).await?;
    info!("database connection established");
    tokio::fs::create_dir_all(&config.cache_dir).await?;
    info!(cache_dir = %config.cache_dir.display(), "artwork cache directory ensured");
    let metadata_provider =
        mova_application::build_metadata_provider(config.metadata_provider.clone())?;
    if metadata_provider.is_enabled() {
        info!(provider = "tmdb", "metadata provider initialized");
    } else {
        warn!(
            environment_variable = "MOVA_TMDB_ACCESS_TOKEN",
            "TMDB metadata scraping is disabled; local scanning and playback remain available"
        );
    }

    let realtime_hub = state::RealtimeHub::default();
    let realtime_dispatcher = realtime::start_realtime_dispatcher(
        pool.clone(),
        realtime_hub.clone(),
        config.api_time.offset,
    );
    let state = AppState {
        db: pool,
        api_time_offset: config.api_time.offset,
        artwork_cache_dir: config.cache_dir.clone(),
        metadata_provider,
        scan_registry: state::ScanRegistry::default(),
        realtime_hub,
        realtime_dispatcher,
        background_jobs: state::BackgroundJobNotifier::default(),
    };

    sync_runtime::start_background_workers(state.clone(), config.worker_concurrency);

    let app = app::build_router(state, config.web_dist_dir.clone());
    let addr = config.socket_addr()?;
    info!("mova-server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    // 没有显式配置 RUST_LOG 时，给一个适合本地排查问题的默认日志级别。
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));

    fmt().with_env_filter(filter).init();
}
