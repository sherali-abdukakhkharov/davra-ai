-- The loop guard stopped agents in a project: one row per streak of agent
-- messages (keyed by the newest message in it), so the Telegram chat gets a
-- single "continue / stop" prompt however often the agents retry.

create table loop_holds (
  id               bigserial primary key,
  org_id           uuid not null references orgs (id) on delete cascade,
  project_id       uuid not null references projects (id) on delete cascade,
  last_message_id  bigint not null,
  agents           text[] not null default '{}',
  tg_message_id    bigint,
  decision         text check (decision in ('continue', 'stop')),
  decided_by       uuid references members (id) on delete set null,
  created_at       timestamptz not null default now(),
  decided_at       timestamptz,
  unique (project_id, last_message_id)
);

do $rls$
begin
  if exists (select 1 from pg_roles where rolname = 'davra_app') then
    grant select, insert, update on loop_holds to davra_app;
    grant usage, select on sequence loop_holds_id_seq to davra_app;
    alter table loop_holds enable row level security;
    create policy tenant on loop_holds to davra_app
      using (org_id = davra_org()) with check (org_id = davra_org());
  end if;
end
$rls$;
