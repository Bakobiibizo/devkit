#!/bin/bash
# ngrok startup script with OAuth credentials from 1Password
# Called by: dev ngrok
# Requires: ~/.env with OP_PRODUCTION service account token
# Requires: ~/.config/ngrok/ngrok.template.yml as the base config

set -euo pipefail

source ~/.env
export OP_SERVICE_ACCOUNT_TOKEN="$OP_PRODUCTION"
GITHUB_CLIENT_ID="$(op read "op://production/GITHUB_CLIENT_ID/password")"
GITHUB_CLIENT_SECRET="$(op read "op://production/GITHUB_CLIENT_SECRET/password")"

# Generate ngrok.yml from template with credentials substituted
sed -e "s|\${GITHUB_CLIENT_ID}|${GITHUB_CLIENT_ID}|g" \
    -e "s|\${GITHUB_CLIENT_SECRET}|${GITHUB_CLIENT_SECRET}|g" \
    ~/.config/ngrok/ngrok.template.yml > ~/.config/ngrok/ngrok.yml

exec ngrok start --all
