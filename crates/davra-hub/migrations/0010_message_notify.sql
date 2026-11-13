-- Wake waiting inbox requests (long-poll, live agent channels) the moment a
-- message is stored, whichever path stored it: API, Telegram, handoffs, GitHub.
create function davra_message_notify() returns trigger language plpgsql as $$
begin
  perform pg_notify('davra_messages', new.project_id::text);
  return null;
end $$;

create trigger messages_notify after insert on messages
  for each row execute function davra_message_notify();
