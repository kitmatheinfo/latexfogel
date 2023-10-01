#!/usr/bin/bash

set -eu

if [[ -z "$1" ]] || [[ -z "$2" ]] || [[ -z "$3" ]] || [[ -z "$4" ]]; then
    echo "Usage: $0 <path to docker tarball> <image name> <base image tag> <comma separated more tags>"
fi

IMAGE_TARBALL="$1"
IMAGE_NAME="$2"
IMAGE_TAG="$3"
IMAGE_TAGS="$4"

# Load our tarball
docker load < "$IMAGE_TARBALL"

IFS=","
for tag in $IMAGE_TAGS; do
    docker tag "$IMAGE_NAME:$IMAGE_TAG" "$tag"
done

docker push --all-tags "$IMAGE_NAME"
