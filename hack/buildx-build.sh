#!/usr/bin/env bash
set -euo pipefail

DOCKERFILE="${1:-}"
CONTEXT="${2:-}"
shift 2 || true
BUILD_ARGS=("$@")

if [[ -z "${DOCKERFILE}" || -z "${CONTEXT}" ]]; then
  echo "Usage: $0 <dockerfile> <context> [BUILD_ARG...]" >&2
  exit 1
fi

if [[ -z "${IMAGE:-}" ]]; then
  echo "IMAGE is required for custom Skaffold builds" >&2
  exit 1
fi

PUSH="${PUSH_IMAGE:-false}"
PLATFORM="${BUILD_PLATFORM:-linux/amd64}"
BUILDER="${BUILDX_BUILDER:-}"
CACHE_DIR="${BUILDX_CACHE_DIR:-/tmp/.buildx-cache}"
CACHE_DIR_NEW="${CACHE_DIR}-new"
KIND_CLUSTER="${KIND_CLUSTER_NAME:-}"

mkdir -p "${CACHE_DIR}"
rm -rf "${CACHE_DIR_NEW}"

if [[ -n "${BUILDER}" ]]; then
  docker buildx use "${BUILDER}"
fi

build_cmd=(
  docker buildx build
  --file "${DOCKERFILE}"
  --tag "${IMAGE}"
  --platform "${PLATFORM}"
  --cache-from "type=local,src=${CACHE_DIR}"
  --cache-to "type=local,dest=${CACHE_DIR_NEW},mode=max"
  --progress=plain
)

for arg in "${BUILD_ARGS[@]}"; do
  build_cmd+=(--build-arg "${arg}")
done

if [[ "${PUSH}" == "true" ]]; then
  build_cmd+=(--push)
else
  build_cmd+=(--load)
fi

build_cmd+=("${CONTEXT}")

echo ">>> Building ${IMAGE} for ${PLATFORM}"
printf '>>> buildx args: %s\n' "${build_cmd[*]}"
"${build_cmd[@]}"

rm -rf "${CACHE_DIR}"
mv "${CACHE_DIR_NEW}" "${CACHE_DIR}"

if [[ "${PUSH}" != "true" && -n "${KIND_CLUSTER}" && -x "$(command -v kind)" ]]; then
  echo ">>> Loading ${IMAGE} into KinD cluster ${KIND_CLUSTER}"
  kind load docker-image --name "${KIND_CLUSTER}" "${IMAGE}"
fi
