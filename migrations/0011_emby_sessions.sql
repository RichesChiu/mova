-- Compatibility devices share the account session lifecycle. Password changes,
-- disable/delete and existing housekeeping revoke them through the foreign key.
create table server_identity (
    singleton boolean primary key default true check (singleton),
    server_id varchar(32) not null unique
);

create table emby_devices (
    token_hash varchar(64) primary key references user_sessions(token_hash) on delete cascade,
    device_id varchar(256) not null,
    device_name varchar(256) not null,
    client_name varchar(128) not null,
    client_version varchar(64) not null
);

create table playback_sessions (
    id varchar(32) primary key,
    token_hash varchar(64) not null references emby_devices(token_hash) on delete cascade,
    user_id bigint not null references users(id) on delete cascade,
    media_item_id bigint not null references media_items(id) on delete cascade,
    media_file_id bigint not null references media_files(id) on delete cascade,
    created_at timestamptz not null default clock_timestamp(),
    started_at timestamptz,
    updated_at timestamptz not null default clock_timestamp(),
    stopped_at timestamptz,
    expires_at timestamptz not null default (clock_timestamp() + interval '24 hours')
);
create index playback_sessions_user_item on playback_sessions(user_id, media_item_id, started_at desc);
create index playback_sessions_token on playback_sessions(token_hash);
create index playback_sessions_expiry on playback_sessions(expires_at);
