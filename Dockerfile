FROM ubuntu:23.04

RUN mkdir -p /workdir

WORKDIR /workdir

COPY target/release/ambrogio /workdir/ambrogio

RUN chmod +x /workdir/ambrogio

ENTRYPOINT /workdir/ambrogio
