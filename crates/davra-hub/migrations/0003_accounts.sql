-- v0.3 start: web accounts (email + password) and browser sessions.

create table users (
  id            uuid primary key default gen_random_uuid(),
  email         text not null,
  email_lower   text generated always as (lower(email)) stored unique,
  name          text not null,
  password_hash text not null,
  created_at    timestamptz not null default now(),
  last_login_at timestamptz
);

create table sessions (
  id           uuid primary key default gen_random_uuid(),
  user_id      uuid not null references users (id) on delete cascade,
  token_hash   bytea not null unique,
  user_agent   text,
  created_at   timestamptz not null default now(),
  last_seen_at timestamptz not null default now(),
  expires_at   timestamptz not null
);

create index sessions_user_idx on sessions (user_id);

-- A human member can be linked to a web account (used by the web app).
alter table members add column user_id uuid references users (id) on delete set null;
create unique index members_user_org_idx on members (org_id, user_id) where user_id is not null;
