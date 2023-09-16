FROM rust:alpine3.17 as builder

RUN apk add --no-cache musl-dev openssl-dev

WORKDIR /home/app

COPY . /home/app

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/home/app/target \
    cargo build --release

FROM alpine:3.17 as runner

RUN addgroup --system app && adduser app --system --ingroup app
USER app

COPY --from=builder /home/app/target/release/openfaas_operato_rs /usr/local/bin/openfaas_operato_rs

RUN chmod +x /usr/local/bin/openfaas_operato_rs

ENTRYPOINT ["/usr/local/bin/openfaas_operato_rs"]

# DOCKER_BUILDKIT=1 docker build -t openfaas_operato_rs:latest . --progress=plain