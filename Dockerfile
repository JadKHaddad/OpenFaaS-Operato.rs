FROM alpine:3.18 as builder

RUN apk update \
    && apk add --no-cache cargo=1.71.1-r0 openssl-dev

WORKDIR /home/app

COPY src /home/app/src
COPY Cargo.toml /home/app/Cargo.toml
COPY Cargo.lock /home/app/Cargo.lock

RUN --mount=type=cache,target=/home/app/target \
    cargo test && cargo build --release && mv /home/app/target/release/openfaas_functions_operato_rs /usr/local/bin/openfaas_functions_operato_rs

FROM alpine:3.18 as runner

# TODO: test if we need libgcc, for real!

RUN apk update \
    && apk add --no-cache libgcc

RUN addgroup --system app && adduser app --system --ingroup app

COPY --from=builder /usr/local/bin/openfaas_functions_operato_rs /usr/local/bin/openfaas_functions_operato_rs

USER app

WORKDIR /home/app

RUN mkdir -p /home/app/.kube

ENTRYPOINT ["openfaas_functions_operato_rs"]

# use openfaas_functions_operato_rs docker command to build and push the current version
# DOCKER_BUILDKIT=1 docker build -t jadkhaddad/openfaas_functions_operato_rs:latest . --progress=plain
# docker push jadkhaddad/openfaas_functions_operato_rs:latest
# docker run --rm -it -v ${USERPROFILE}/.kube:/home/app/.kube openfaas_operato_rs:latest run operator -n openfaas-fn controller
# docker run --rm -it -v ~/.kube:/home/app/.kube openfaas_operato_rs:latest run operator -n openfaas-fn controller