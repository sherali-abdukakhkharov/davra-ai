-- Telegram username of a human member, so the hub can use it as the handle
-- (agents' @mentions then notify the person in Telegram).
alter table members add column tg_username text;
