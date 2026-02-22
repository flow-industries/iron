#!/bin/sh
set -eu

# Called by WUD command trigger when a new image is detected.
# WUD passes container info as env vars: name, image_name, update_kind_remote_value, etc.
# Container names follow Docker Compose v2 format: <project>-<service>-<N>

STACK=$(echo "$name" | cut -d'-' -f1)
COMPOSE_FILE="/opt/flow/${STACK}/docker-compose.yml"

if [ ! -f "$COMPOSE_FILE" ]; then
  echo "ERROR: Compose file not found: $COMPOSE_FILE"
  exit 1
fi

echo "Rolling out ${STACK} (image: ${image_name})"
docker rollout "$STACK" -f "$COMPOSE_FILE"
echo "Rollout complete for ${STACK}"
