use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{env, fmt};

// 把 migrations 编译进二进制，服务启动时就能自动把数据库升级到期望版本。
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Clone)]
pub struct DatabaseSettings {
    pub url: String,
    pub max_connections: u32,
}

impl fmt::Debug for DatabaseSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseSettings")
            .field("url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .finish()
    }
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
        .context("failed to connect to database")?;

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

#[cfg(test)]
mod tests {
    use super::DatabaseSettings;

    #[test]
    fn database_settings_debug_output_redacts_credentials() {
        let settings = DatabaseSettings {
            url: "postgres://mova:secret@database:5432/mova".to_string(),
            max_connections: 10,
        };

        let debug_output = format!("{settings:?}");

        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("secret"));
        assert!(!debug_output.contains("postgres://"));
    }
}
