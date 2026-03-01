#!/bin/sh
set -eu

# Called by WUD command trigger when a new image is detected.
# WUD passes container info as env vars: name, display_name, image_name, etc.
# display_name is the Docker Compose project name (= stack directory name).

STACK="$display_name"
COMPOSE_FILE="/opt/flow/${STACK}/docker-compose.yml"

if [ ! -f "$COMPOSE_FILE" ]; then
  echo "ERROR: Compose file not found: $COMPOSE_FILE"
  exit 1
fi

echo "Rolling out ${STACK} (image: ${image_name})"
docker compose -f "$COMPOSE_FILE" pull
docker rollout "$STACK" -f "$COMPOSE_FILE"
echo "Rollout complete for ${STACK}"
