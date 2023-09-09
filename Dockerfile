FROM rust:1.72.0-slim-buster

RUN mkdir -p /workdir

WORKDIR /workdir

COPY target/release/ambrogio_bin /workdir/ambrogio

RUN chmod +x /workdir/ambrogio

ENTRYPOINT /workdir/ambrogio
