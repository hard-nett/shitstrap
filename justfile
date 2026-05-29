#!/bin/bash

docker_image := env_var_or_default('DOCKER_IMAGE', 'shitstrap-optimizer:0.17.0')
arch := `if [ "$(uname -m)" = "arm64" ] || [ "$(uname -m)" = "aarch64" ]; then echo "linux/arm64"; else echo "linux/amd64"; fi`

wasm:
    #!/bin/bash
    if [[ $(uname -m) == 'arm64' ]] || [ $(uname -m) == 'aarch64' ]]; then docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            --platform linux/arm64 \
            cosmwasm/optimizer-arm64:0.17.0; \
    elif [[ $(uname -m) == 'x86_64' ]]; then docker run --rm -v "$(pwd)":/code \
            --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
            --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
            --platform linux/amd64 \
            cosmwasm/optimizer:0.17.0; fi

schema-codegen:
        @sh scripts/sh/schema-codegen.sh

# wasm:
#     docker run --rm \
#             -v "{{justfile_directory()}}/..":/workspace \
#             --mount type=volume,source=shitstraps_cache,target=/target \
#             --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
#             --platform {{arch}} \
#             {{docker_image}}