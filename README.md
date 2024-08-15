# tforward

## Description

Telegram bot that forwards messages from a chosen channel to an unlimited\* amount of recepients

## Commands

- `/subscribe` - adds chat to list of recepients

## Environment variables

- `TG_TOKEN` - telegram bot token, refer to [official docs](https://core.telegram.org/bots) for more info
- `BOT_URL` - url for recieving webhooks, should be secret and non-bruteforcable
- `BOT_PORT` - port to listen for webhooks on
- `CHANNEL_ID` - id of a channel to forward messages from
- `UPTRACE_DSN` - telemetry, refer to [uptrace website](https://uptrace.dev/) for more info

## Additional setup

List of recepients is stored in a json file at `/data/tforward_settings.json`
