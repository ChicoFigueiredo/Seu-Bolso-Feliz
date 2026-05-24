#!/usr/bin/env bash

set -euo pipefail

skip_pattern='(\[(skip ci|ci skip|no ci|skip actions|actions skip)\]|skip-checks:[[:space:]]*true)'
push_args=("$@")

branch=$(git symbolic-ref --quiet --short HEAD)

if [[ -z "$branch" ]]; then
  echo "Nao foi possivel identificar a branch atual. Saia do detached HEAD antes de usar este script." >&2
  exit 1
fi

head_message=$(git log -1 --pretty=%B)

if ! grep -Eiq "$skip_pattern" <<<"$head_message"; then
  echo "O commit HEAD precisa conter um marcador de skip de CI, por exemplo: [skip ci]." >&2
  echo "Isso cobre remotos genericos; para GitLab o script tambem adiciona -o ci.skip." >&2
  exit 1
fi

mapfile -t remotes < <(git remote)

if [[ ${#remotes[@]} -eq 0 ]]; then
  echo "Nenhum remoto Git configurado." >&2
  exit 1
fi

for remote in "${remotes[@]}"; do
  remote_url=$(git remote get-url "$remote")
  echo "Push sem CI para $remote ($remote_url) na branch $branch"

  if [[ "$remote_url" == *gitlab* ]]; then
    git push --follow-tags -o ci.skip "${push_args[@]}" "$remote" "HEAD:refs/heads/$branch"
    continue
  fi

  git push --follow-tags "${push_args[@]}" "$remote" "HEAD:refs/heads/$branch"
done