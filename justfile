#!/bin/bash

docker_image := env_var_or_default('DOCKER_IMAGE', 'shitstrap-optimizer:0.17.0')
arch := `if [ "$(uname -m)" = "arm64" ] || [ "$(uname -m)" = "aarch64" ]; then echo "linux/arm64"; else echo "linux/amd64"; fi`

 
schema-codegen:
        @sh scripts/sh/schema-codegen.sh

optimizer-build:
        docker build -t {{docker_image}} optimizer/

workspace-optimize:
	docker run --rm \
		-v $(pwd)/..:/workspace \
		--env PROJECT_DIR=$$(basename $$(pwd)) \
		--mount type=volume,source=terp_optimizer_cache,target=/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		--platform {{arch}} \
		{{docker_image}}

# Clear build caches (useful after toolchain changes or if builds fail)
optimizer-clean:
        docker volume rm dao_contracts_cache registry_cache 2>/dev/null || true%          