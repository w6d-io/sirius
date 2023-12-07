FROM rust:1.73-bullseye AS build
ARG JOB_TOKEN
ARG JOB_USER
ENV CARGO_NET_GIT_FETCH_WITH_CLI true
WORKDIR /usr/src/sirius
COPY . .
RUN apt-get dist-upgrade && apt-get update -y
RUN apt-get install -y build-essential cmake libpthread-stubs0-dev zlib1g-dev zlib1g protobuf-compiler
# RUN git config --global url."https://${JOB_USER}:${JOB_TOKEN}@gitlab.w6d.io/".insteadOf "https://gitlab.w6d.io/"
RUN ./do_config.sh
RUN rustup component add rustfmt
RUN cargo install --path ./ --locked --features opa

FROM debian:bullseye
WORKDIR /usr/local/bin/
RUN apt-get update -y
RUN apt-get install -y build-essential libpq-dev openssl libssl-dev ca-certificates
COPY --from=build /usr/local/cargo/bin/sirius /usr/local/bin/sirius
CMD ["sirius"]
