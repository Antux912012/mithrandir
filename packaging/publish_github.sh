#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${HOME}/.env"
GITHUB_TOKEN="${GITHUB_TOKEN:-}"

for candidate in "${ENV_FILE}" "${ROOT_DIR}/.env" "${ROOT_DIR}/../.env"; do
  if [ -z "${GITHUB_TOKEN}" ] && [ -f "${candidate}" ]; then
    GITHUB_TOKEN=$(grep -E "^GITHUB_TOKEN=" "${candidate}" | cut -d'=' -f2- | tr -d '\r\n"' || true)
  fi
done

if [ -z "${GITHUB_TOKEN}" ] && [ -f "${HOME}/.config/cachy-kernel-updater/github_token" ]; then
  GITHUB_TOKEN=$(tr -d '[:space:]' < "${HOME}/.config/cachy-kernel-updater/github_token")
fi

if [ -z "${GITHUB_TOKEN}" ] && command -v gh >/dev/null 2>&1; then
  GITHUB_TOKEN=$(gh auth token 2>/dev/null || true)
fi

if [ -z "${GITHUB_TOKEN}" ]; then
  echo "Error: GITHUB_TOKEN not found."
  echo "Please set GITHUB_TOKEN in your environment or store it in ~/.env with:"
  echo "  echo 'GITHUB_TOKEN=your_token_here' >> ~/.env"
  exit 1
fi

echo "==> Verifying GitHub token authentication..."
USER_RESP=$(curl -s -H "Authorization: Bearer ${GITHUB_TOKEN}" \
                 -H "Accept: application/vnd.github.v3+json" \
                 https://api.github.com/user)

USER_LOGIN=$(echo "${USER_RESP}" | grep -o '"login": "[^"]*' | cut -d'"' -f4 || true)

if [ -z "${USER_LOGIN}" ]; then
  echo "Error: GitHub authentication failed. Please check your token permissions (repo scope required)."
  exit 1
fi

echo "==> Authenticated as GitHub user: ${USER_LOGIN}"

REPO_NAME="mithrandir"
REPO_DESC="Unified Native Linux System Optimization, Cache Management, BORE Scheduler & Power Utility in Rust"
VERSION=$(grep -m1 '^version' "${ROOT_DIR}/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')
TAG="v${VERSION}"

echo "==> Checking if repository ${USER_LOGIN}/${REPO_NAME} exists..."
CHECK_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
  -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  -H "Accept: application/vnd.github.v3+json" \
  "https://api.github.com/repos/${USER_LOGIN}/${REPO_NAME}")

if [ "${CHECK_STATUS}" -eq 404 ]; then
  echo "==> Creating public repository ${USER_LOGIN}/${REPO_NAME} on GitHub..."
  curl -s -X POST \
    -H "Authorization: Bearer ${GITHUB_TOKEN}" \
    -H "Accept: application/vnd.github.v3+json" \
    https://api.github.com/user/repos \
    -d "{\"name\":\"${REPO_NAME}\",\"description\":\"${REPO_DESC}\",\"private\":false,\"has_issues\":true,\"has_projects\":true,\"has_wiki\":true}" >/dev/null
  echo "==> Repository created successfully."
elif [ "${CHECK_STATUS}" -eq 200 ]; then
  echo "==> Repository ${USER_LOGIN}/${REPO_NAME} already exists on GitHub."
else
  echo "==> GitHub API returned status ${CHECK_STATUS}."
fi

cd "${ROOT_DIR}"

git branch -M main

echo "==> Staging and committing release ${TAG}..."
git add -A
if ! git diff --cached --quiet; then
  git commit -m "release: ${TAG} — remove sched-ext (SCX) and align Flatpak/AppStream with Flathub policies"
fi
git tag -f "${TAG}"

REMOTE_URL="https://github.com/${USER_LOGIN}/${REPO_NAME}.git"
if git remote | grep -q "^origin$"; then
  git remote set-url origin "${REMOTE_URL}"
else
  git remote add origin "${REMOTE_URL}"
fi

echo "==> Pushing 'main' branch and tag ${TAG} to ${REMOTE_URL}..."
git push "https://x-access-token:${GITHUB_TOKEN}@github.com/${USER_LOGIN}/${REPO_NAME}.git" main:main -u
git push "https://x-access-token:${GITHUB_TOKEN}@github.com/${USER_LOGIN}/${REPO_NAME}.git" "${TAG}" -f
git config branch.main.remote origin
git fetch origin

echo "==> Creating or updating GitHub Release ${TAG}..."
REL_RESP=$(curl -s -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  "https://api.github.com/repos/${USER_LOGIN}/${REPO_NAME}/releases/tags/${TAG}")

REL_ID=$(python3 -c 'import json, sys; d=json.loads(sys.stdin.read()); print(d.get("id", ""))' <<< "${REL_RESP}" 2>/dev/null || true)

if [ -z "${REL_ID}" ]; then
  REL_BODY="Mithrandir ${TAG} — Native GTK4 & Libadwaita GNU/Linux System Optimizer, Deep Cleaner, BORE Scheduler & Power Manager in Rust.

### Changes in ${TAG}
- Removed \`sched-ext\` (SCX) scheduler management and bundled SCX packages to streamline kernel & power management around BORE (\`sched_bore\`), CPU scaling governors, EPP, Turbo Boost, and PCIe ASPM.
- Updated AppStream 1.0 MetaInfo (\`io.github.antux912012.Mithrandir.metainfo.xml\`) and Flatpak packaging to comply with Flathub submission standards.
- Reduced Flatpak and AppImage package footprint."
  CREATE_JSON=$(python3 -c 'import json, sys; print(json.dumps({"tag_name": sys.argv[1], "name": f"Mithrandir {sys.argv[1]}", "body": sys.argv[2], "draft": False, "prerelease": False}))' "${TAG}" "${REL_BODY}")
  REL_RESP=$(curl -s -X POST \
    -H "Authorization: Bearer ${GITHUB_TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${USER_LOGIN}/${REPO_NAME}/releases" \
    -d "${CREATE_JSON}")
  REL_ID=$(python3 -c 'import json, sys; d=json.loads(sys.stdin.read()); print(d.get("id", ""))' <<< "${REL_RESP}" 2>/dev/null || true)
fi

if [ -n "${REL_ID}" ]; then
  for asset in "${ROOT_DIR}/mithrandir_${VERSION}.flatpak" "${ROOT_DIR}/mithrandir_${VERSION}_amd64.deb" "${ROOT_DIR}/mithrandir-${VERSION}-x86_64.AppImage"; do
    if [ -f "${asset}" ]; then
      fname=$(basename "${asset}")
      echo "==> Uploading release asset: ${fname}..."
      # Delete existing asset with same name if present
      EXISTING_ASSET_ID=$(python3 -c '
import json, sys
d = json.loads(sys.stdin.read())
for a in d.get("assets", []):
    if a.get("name") == sys.argv[1]:
        print(a.get("id", ""))
        break
' "${fname}" <<< "${REL_RESP}" 2>/dev/null || true)
      if [ -n "${EXISTING_ASSET_ID}" ]; then
        curl -s -X DELETE \
          -H "Authorization: Bearer ${GITHUB_TOKEN}" \
          -H "Accept: application/vnd.github+json" \
          "https://api.github.com/repos/${USER_LOGIN}/${REPO_NAME}/releases/assets/${EXISTING_ASSET_ID}" >/dev/null || true
      fi
      curl -s -X POST \
        -H "Authorization: Bearer ${GITHUB_TOKEN}" \
        -H "Accept: application/vnd.github+json" \
        -H "Content-Type: application/octet-stream" \
        --data-binary @"${asset}" \
        "https://uploads.github.com/repos/${USER_LOGIN}/${REPO_NAME}/releases/${REL_ID}/assets?name=${fname}" >/dev/null
      echo "    Uploaded ${fname} successfully."
    fi
  done
fi

echo "==> Successfully published ${TAG} to: https://github.com/${USER_LOGIN}/${REPO_NAME}/releases/tag/${TAG}"
