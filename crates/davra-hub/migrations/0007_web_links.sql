-- v0.3: link a web account to the Telegram identity of existing members.
alter table users add column tg_user_id bigint unique;

create table web_links (
  code       text primary key,
  user_id    uuid not null references users (id) on delete cascade,
  expires_at timestamptz not null,
  used_at    timestamptz
);
