CREATE TABLE analytics_page_views (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
    date       DATE   NOT NULL,
    hour       TIME   NOT NULL,
    event_id   TEXT   NOT NULL,
    source     TEXT   NOT NULL,
    medium     TEXT   NOT NULL,
    term       TEXT   NOT NULL,
    content    TEXT   NOT NULL,
    platform   TEXT   NOT NULL,
    campaign   TEXT   NOT NULL,
    url        TEXT   NOT NULL,
    code       TEXT   NOT NULL,
    client_id  TEXT   NOT NULL,
    user_agent TEXT   NOT NULL,
    ip_address TEXT  NOT NULL,
    count      BIGINT NOT NULL  DEFAULT 0,
    CONSTRAINT analytics_page_views_unique UNIQUE (date, hour, event_id, source, medium,
                                                   term, content, platform, campaign, url,
                                                   client_id, user_agent, code)
);
