-- Episode outlines are structured server-owned documents. Store them as jsonb
-- so PostgreSQL validates every write and future outline queries can use JSON
-- operators without reparsing untyped text.
alter table series_episode_outline_cache
    alter column outline_json type jsonb
    using outline_json::jsonb;
