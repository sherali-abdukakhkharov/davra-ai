-- A project's owners link its Telegram group themselves: the web app hands
-- out a one-time code, the bot is added to a group with it
-- (t.me/<bot>?startgroup=<code>), and that group becomes the project's chat.

alter table projects
  add column tg_chat_title text,
  add column tg_forum boolean not null default false,
  -- The forum topic the bot posts to; null = the whole group (General).
  add column tg_thread_id bigint,
  -- Who linked the group: with the owners, they choose the topic.
  add column tg_linked_by uuid references members (id) on delete set null;

-- Forum topics the bot has seen (the Bot API cannot list them).
create table tg_topics (
  chat_id    bigint not null,
  thread_id  bigint not null,
  name       text,
  seen_at    timestamptz not null default now(),
  primary key (chat_id, thread_id)
);

create table tg_group_links (
  code        text primary key,
  org_id      uuid not null references orgs (id) on delete cascade,
  project_id  uuid not null references projects (id) on delete cascade,
  member_id   uuid not null references members (id) on delete cascade,
  user_id     uuid not null references users (id) on delete cascade,
  expires_at  timestamptz not null,
  used_at     timestamptz
);

create index tg_group_links_project on tg_group_links (project_id);

do $rls$
begin
  if exists (select 1 from pg_roles where rolname = 'davra_app') then
    grant select, insert, update, delete on tg_group_links to davra_app;
    alter table tg_group_links enable row level security;
    create policy tenant on tg_group_links to davra_app
      using (org_id = davra_org()) with check (org_id = davra_org());
  end if;
end
$rls$;
