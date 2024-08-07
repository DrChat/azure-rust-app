FROM rust:bookworm as builder

# RUN apt-key update && apt-get update \
#   && apt-get install build-essential openssl libssl-dev vim -y --force-yes \
#   && echo "root:Docker!" | chpasswd \
#   && chmod 755 /bin/init_container.sh
#   # && apt install openssh-server --no-install-recommends -y

# RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
# ENV PATH ${PATH}:/root/.cargo/bin:/home/site/wwwroot

RUN rustup install nightly

COPY Cargo.lock /build/
COPY Cargo.toml /build/
COPY src /build/src

# Build the default page
WORKDIR /build

RUN cargo +nightly build --release
RUN mkdir -p /app && mv target/release/axum-app /app/

FROM debian:bookworm

COPY --from=builder /app /app
COPY static /app/static
COPY templates /app/templates

RUN apt-get update \
 && apt-get install -y libssl-dev

#WORKDIR /home/site/wwwroot
WORKDIR /app
EXPOSE 8000

ENTRYPOINT [ "/app/axum-app" ]
