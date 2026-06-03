# ============================================================
# Stage 1: Build custom bob
# ============================================================
FROM rust:1.86.0-alpine AS bob-builder

RUN apk add --no-cache musl-dev

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

WORKDIR /bob_the_builder

COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

RUN RUSTFLAGS='-C link-arg=-s' cargo build --release && \
    ls -lh target/release/bob && \
    mv target/release/bob /usr/local/bin/bob

# ============================================================
# Stage 2: Final optimizer image
# ============================================================
FROM cosmwasm/optimizer-arm64:0.17.0

RUN rustup install 1.86.0 && rustup default 1.86.0
RUN rustup target add wasm32-unknown-unknown --toolchain 1.86.0

COPY --from=bob-builder /usr/local/bin/bob /usr/local/bin/bob
COPY dao-optimize.sh /usr/local/bin/dao-optimize.sh
RUN chmod +x /usr/local/bin/dao-optimize.sh

ENV DAO_WORKSPACE_SUBDIR=shitstrap

ENTRYPOINT ["dao-optimize.sh"]
CMD ["."]