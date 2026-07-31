# Mova deployment and data maintenance

This document defines the supported Docker Compose deployment, initialization boundary, stable upgrade path, database backup and restore procedure, and rollback rules for Mova 1.x.

**Language / 语言:** [中文](#中文) · [English](#english)

## 中文

### 1. 服务边界

官方 Compose 同时运行：

- `app`：Mova Web 与 API 服务，通过宿主机端口 `36080` 提供访问。
- `database`：PostgreSQL，只连接 Compose 项目内部网络，不向宿主机发布 `5432`。

`MOVA_DATABASE_URL` 与 `database.POSTGRES_PASSWORD` 必须使用同一密码。官方示例把这组内部凭据直接写在一份 Compose 文件中，避免额外的 `.env`；该数据库不接受宿主机或公网连接。如果把数据库加入其他共享 Docker 网络，应同时修改这两个值并避免与不可信容器共享网络。

媒体目录以只读方式挂载到 `/media`。Mova 会写入 `data/postgres/` 和 `data/cache/`，不会修改原始媒体文件。

#### 使用外部 PostgreSQL

如需连接已有的 PostgreSQL：

1. 将 `app.environment.MOVA_DATABASE_URL` 改为容器可访问的数据库地址。
2. 删除 `app.depends_on.database`。
3. 删除整个 `database` 服务。

外部数据库必须提前创建，连接账号需要拥有建表、修改表结构和执行迁移的权限。容器中的 `localhost` 指向 Mova 容器自身，应填写数据库的实际 IP 或域名，并按数据库要求在连接 URL 中配置 TLS。Mova 启动时自动执行迁移。

本文后续使用 `docker compose exec database` 的备份与恢复命令只适用于 Compose 内置 PostgreSQL。使用外部数据库时，应改用数据库提供方支持的备份、恢复和回滚流程。

### 2. 首次初始化安全

数据库中还没有系统管理员时，初始化接口允许创建首个系统管理员。完成初始化前：

- 只从可信本机或受控局域网访问 `36080`。
- 不要把端口直接暴露到公网，也不要提前开放公网反向代理。
- 如果必须从另一台机器初始化，应通过受控局域网、防火墙白名单或 SSH 隧道访问。

创建首个系统管理员后，再配置 HTTPS 反向代理。公网访问时，在 `app.environment` 中加入：

```yaml
MOVA_SESSION_COOKIE_SECURE: "true"
```

`36080` 是 Web/API 端口，不是数据库端口。初始化的安全边界由部署网络保证，不需要额外的 bootstrap token。

### 3. Preview 到 1.0 的一次性重建

1.0 将最终表结构冻结在 `migrations/0001_init.sql`。Preview 数据库使用的是开发期表结构，不能作为 1.0 的迁移起点；第一次启动 1.0 前必须执行最后一次数据库重建并重新扫描媒体库。

先停止 Preview 服务：

```bash
docker compose down
```

保留旧目录作为临时回退副本，不直接删除：

```bash
mv data/postgres "data/postgres.preview-$(date +%Y%m%d)"
mv data/cache "data/cache.preview-$(date +%Y%m%d)"
```

把 Compose 中的应用镜像更新为 `richeschiu/mova:latest`，然后启动：

```bash
docker compose pull
docker compose up -d
```

随后重新创建系统管理员和媒体库，并完成扫描。旧 Preview 数据库备份只用于回退或人工参考，不能导入 1.0；原始媒体目录不受影响。确认 1.0 数据和播放正常后，可以自行删除旧 Preview 数据目录。

### 4. 1.0 及后续稳定版升级

1.0 是数据库迁移基线。1.0 之后的 schema 变更使用顺序迁移，应用启动时自动执行，不要求例行删除数据库或重新扫描。只有具体发布说明明确要求时才重建缓存或重新扫描。

每次升级前先创建数据库备份，然后执行：

```bash
docker compose pull
docker compose up -d
docker compose ps
curl --fail http://127.0.0.1:36080/api/health
```

`richeschiu/mova:latest` 指向当前稳定版本。排障或回滚时，应记录升级前使用的不可变版本标签和镜像 digest。

### 5. 数据库备份

在服务正常运行时，可以通过 PostgreSQL 一致性快照备份数据库：

```bash
mkdir -p backups
docker compose exec -T database \
  pg_dump --username=mova --dbname=mova --format=custom \
  > "backups/mova-$(date +%Y%m%d-%H%M%S).dump"
```

备份文件包含账户、媒体库、元数据、播放进度、通知和后台任务等权威状态，应视为敏感数据并妥善保管。建议同时私下保存当前 `docker-compose.yml`、镜像标签和 digest；Compose 中可能包含 TMDB Token，不应上传到公开仓库。

`data/cache/` 只包含可重建的图片和媒体派生缓存，不是数据库备份的必要组成部分。如需缩短恢复后的重新下载时间，可以额外备份：

```bash
tar -C data -czf "backups/mova-cache-$(date +%Y%m%d-%H%M%S).tar.gz" cache
```

Mova 不负责备份只读挂载的原始媒体目录。

### 6. 数据库恢复

使用与备份版本相同或更新的 Mova 镜像。停止应用写入，但保持数据库容器运行：

```bash
docker compose stop app
docker compose exec -T database \
  dropdb --username=mova --if-exists --force mova
docker compose exec -T database \
  createdb --username=mova --owner=mova mova
docker compose exec -T database \
  pg_restore --username=mova --dbname=mova --no-owner --exit-on-error \
  < backups/mova-YYYYMMDD-HHMMSS.dump
docker compose start app
```

应用启动后会执行该版本需要的顺序迁移。最后检查：

```bash
curl --fail http://127.0.0.1:36080/api/health
docker compose logs --tail=200 app
```

缓存目录缺失时会按需重建，不应为了恢复缓存而覆盖一个已经验证正常的数据库。

### 7. 回滚

升级前的数据库备份是可靠回滚点。需要回滚时：

1. 停止 `app`。
2. 把 Compose 中的应用镜像改回升级前的不可变版本标签或 digest。
3. 按“数据库恢复”步骤恢复升级前创建的数据库备份。
4. 启动 `app` 并检查健康接口和日志。

不要让旧版本应用直接连接已经被新版本迁移过的数据库。缓存通常可以继续使用或按需重建；发布说明明确要求时再清空缓存。

---

## English

### 1. Service boundaries

The official Compose stack runs:

- `app`: the Mova Web and API service, published on host port `36080`.
- `database`: PostgreSQL, connected only to the Compose project network with no host-published `5432` port.

`MOVA_DATABASE_URL` and `database.POSTGRES_PASSWORD` must use the same password. The official example keeps these internal credentials in one Compose file so no additional `.env` file is required. The database does not accept host or public connections. If you attach it to another shared Docker network, change both values and do not share that network with untrusted containers.

The media directory is mounted read-only at `/media`. Mova writes to `data/postgres/` and `data/cache/` and does not modify original media files.

#### Using external PostgreSQL

To connect an existing PostgreSQL database:

1. Replace `app.environment.MOVA_DATABASE_URL` with an address reachable from the container.
2. Remove `app.depends_on.database`.
3. Remove the entire `database` service.

Create the external database first and grant the connection account permission to create and alter tables and run migrations. `localhost` inside the container refers to the Mova container itself, so use the database's actual IP address or DNS name and configure TLS in the connection URL when required. Mova runs migrations automatically during startup.

The backup and restore commands later in this document that use `docker compose exec database` apply only to the bundled PostgreSQL service. For an external database, follow the provider-supported backup, restore, and rollback procedure.

### 2. Initial bootstrap security

While no system administrator exists, the bootstrap endpoint can create the first system administrator. Before completing bootstrap:

- Access port `36080` only from a trusted local machine or controlled LAN.
- Do not expose the port directly to the Internet or enable a public reverse proxy.
- If remote initialization is required, use a controlled LAN, firewall allowlist, or SSH tunnel.

After creating the first system administrator, configure an HTTPS reverse proxy. For public access, add this to `app.environment`:

```yaml
MOVA_SESSION_COOKIE_SECURE: "true"
```

Port `36080` serves Web/API traffic; it is not a database port. The deployment network is the bootstrap trust boundary, so an additional bootstrap token is not required.

### 3. One-time Preview-to-1.0 rebuild

Version 1.0 freezes its final schema in `migrations/0001_init.sql`. Preview databases use development-era schemas and are not supported as a 1.0 migration source. Before starting 1.0 for the first time, perform one final database rebuild and rescan all libraries.

Stop the Preview stack:

```bash
docker compose down
```

Keep the old directories as temporary rollback copies instead of deleting them:

```bash
mv data/postgres "data/postgres.preview-$(date +%Y%m%d)"
mv data/cache "data/cache.preview-$(date +%Y%m%d)"
```

Set the application image to `richeschiu/mova:latest`, then start the stack:

```bash
docker compose pull
docker compose up -d
```

Create the system administrator and libraries again, then complete a rescan. The old Preview database is only for rollback or manual reference and must not be imported into 1.0. Original media files are unaffected. Remove the old Preview data directories only after validating the 1.0 deployment.

### 4. Stable upgrades from 1.0 onward

Version 1.0 is the database migration baseline. Later schema changes use sequential migrations that run automatically at application startup. Routine database deletion and rescanning are not required unless a release note explicitly requests a cache rebuild or media rescan.

Create a database backup before every upgrade, then run:

```bash
docker compose pull
docker compose up -d
docker compose ps
curl --fail http://127.0.0.1:36080/api/health
```

`richeschiu/mova:latest` points to the current stable release. Record the immutable version tag and image digest used before an upgrade so troubleshooting and rollback remain reproducible.

### 5. Database backup

Create a PostgreSQL consistent backup while the stack is running:

```bash
mkdir -p backups
docker compose exec -T database \
  pg_dump --username=mova --dbname=mova --format=custom \
  > "backups/mova-$(date +%Y%m%d-%H%M%S).dump"
```

The dump contains authoritative accounts, libraries, metadata, playback progress, notifications, and background jobs. Treat it as sensitive data. Privately retain the current `docker-compose.yml`, image tag, and digest as well. The Compose file may contain a TMDB token and must not be committed to a public repository.

`data/cache/` contains rebuildable artwork and derived media caches and is not required for a database backup. To reduce downloads after recovery, it can be backed up separately:

```bash
tar -C data -czf "backups/mova-cache-$(date +%Y%m%d-%H%M%S).tar.gz" cache
```

Mova does not back up the read-only mounted original media directory.

### 6. Database restore

Use the same or a newer Mova image than the backup source. Stop application writes while keeping the database container running:

```bash
docker compose stop app
docker compose exec -T database \
  dropdb --username=mova --if-exists --force mova
docker compose exec -T database \
  createdb --username=mova --owner=mova mova
docker compose exec -T database \
  pg_restore --username=mova --dbname=mova --no-owner --exit-on-error \
  < backups/mova-YYYYMMDD-HHMMSS.dump
docker compose start app
```

The application applies any sequential migrations required by that version at startup. Verify the result:

```bash
curl --fail http://127.0.0.1:36080/api/health
docker compose logs --tail=200 app
```

Missing cache data is rebuilt on demand. Do not overwrite a verified database merely to restore cache contents.

### 7. Rollback

The pre-upgrade database backup is the reliable rollback point. To roll back:

1. Stop `app`.
2. Set the application image to the immutable tag or digest used before the upgrade.
3. Restore the pre-upgrade database dump by following “Database restore.”
4. Start `app` and verify the health endpoint and logs.

Do not connect an older application version directly to a database migrated by a newer version. Cache data can normally be reused or rebuilt unless a release note explicitly requires clearing it.
