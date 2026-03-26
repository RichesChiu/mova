use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;

// 把 migrations 编译进二进制，服务启动时就能自动把数据库升级到期望版本。
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
}

impl DatabaseSettings {
    /// 启动时从环境变量读取数据库连接配置。
    pub fn from_env() -> Result<Self> {
        let url = env::var("MOVA_DATABASE_URL")
            .context("missing MOVA_DATABASE_URL environment variable")?;

        let max_connections = env::var("MOVA_DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(10);

        Ok(Self {
            url,
            max_connections,
        })
    }
}

/// 创建整个服务共享的 PostgreSQL 连接池。
pub async fn connect(settings: &DatabaseSettings) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(settings.max_connections)
        .connect(&settings.url)
        .await
        .with_context(|| format!("failed to connect to database at {}", settings.url))?;

    Ok(pool)
}

/// 使用轻量查询检测数据库是否可用，供启动阶段和 `/health` 共用。
pub async fn ping(pool: &PgPool) -> Result<()> {
    sqlx::query("select 1")
        .execute(pool)
        .await
        .context("database ping failed")?;

    Ok(())
}

/// 在对外提供服务前执行数据库迁移，保证表结构和当前代码一致。
pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("failed to run database migrations")?;

    Ok(())
}
