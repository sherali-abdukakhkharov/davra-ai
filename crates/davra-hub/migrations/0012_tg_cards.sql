-- Telegram cards (rich messages) and the GitHub digest.

-- 'rich' sends cards with sendRichMessage; 'plain' keeps the old sendMessage
-- text, for groups whose Telegram apps cannot show rich messages yet.
alter table projects add column tg_format text not null default 'rich'
  check (tg_format in ('rich', 'plain'));

-- One open digest per project: pushes, PRs and passing CI runs within a short
-- window are lines of one Telegram message, edited as events arrive.
create table tg_digests (
  project_id    uuid primary key references projects (id) on delete cascade,
  tg_message_id bigint not null,
  tg_thread_id  bigint,
  started_at    timestamptz not null default now(),
  lines         jsonb not null default '[]'
);
